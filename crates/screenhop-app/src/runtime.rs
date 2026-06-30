//! The live agent runtime — the piece that makes a tray click actually move a monitor.
//!
//! Threading model (from the adversarially-reviewed blueprint):
//! - The mesh [`Node`] serves inbound connections on its own thread; a sync thread periodically
//!   discovers peers and exchanges presence. Both only ever mutate the shared `MeshState` behind its
//!   `Arc<Mutex>`.
//! - The UI thread NEVER blocks on mesh I/O: a tray callback drops a [`UiIntent`] on a channel and
//!   returns; a Slint `Timer` polls `MeshState` (via the `Controller`) to refresh the view.
//! - Outbound switches are **transactional** (connect → send → recv → close); we don't pool
//!   `Session`s (a `TcpStream` isn't shareable/clonable and pooling adds reconnect/retry complexity).
//!
//! Verification: this is exercised end-to-end only on a real 2-PC rig (see docs/REMAINING-CHECKLIST.md).
//! The pure routing decision ([`resolve_target`]) is unit-tested here.

use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use screenhop_core::SwitchOutcome;
use screenhop_net::{Message, RecvError};

use crate::discovery::{Discovery, ManualHosts, MdnsDiscovery};
use crate::mesh::{ActuationReport, Actuator, MeshState, Node};

/// A request to the dedicated actuator thread (which owns the non-`Send` DDC driver). The UI spawns
/// that thread and services these; here we only define the wire so the rest of the agent stays
/// driver-agnostic.
pub enum ActuatorRequest {
    /// Perform a pull-to-self switch and reply with the outcome.
    Switch {
        monitor_id: String,
        reply: Sender<ActuationReport>,
    },
    /// Read a panel's live `0x60` and reply (used by the reconcile trigger).
    Read {
        monitor_id: String,
        reply: Sender<Option<u32>>,
    },
}

/// A `Send` [`Actuator`] that forwards each call to the actuator thread over a channel and waits for
/// the reply. This is what the [`Node`] holds, so the real (non-`Send`) `DdcHiDriver` can stay
/// pinned to its own thread (`DdcHiDriver` holds raw OS handles and is not `Send`).
pub struct ChannelActuator {
    tx: Sender<ActuatorRequest>,
}

impl ChannelActuator {
    pub fn new(tx: Sender<ActuatorRequest>) -> Self {
        Self { tx }
    }
}

impl Actuator for ChannelActuator {
    fn switch_to_self(&mut self, monitor_id: &str) -> ActuationReport {
        let (reply, rx) = channel();
        if self
            .tx
            .send(ActuatorRequest::Switch {
                monitor_id: monitor_id.to_owned(),
                reply,
            })
            .is_err()
        {
            return ActuationReport::new(SwitchOutcome::Failed, None);
        }
        rx.recv()
            .unwrap_or_else(|_| ActuationReport::new(SwitchOutcome::Failed, None))
    }
}

/// A UI-originated intent, handed to the agent worker so the UI thread never blocks on mesh I/O.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiIntent {
    /// Make `target_peer_id` the active source on `monitor_id` (pull-to-self, routed to the target).
    Switch {
        monitor_id: String,
        target_peer_id: String,
    },
}

/// Peer id → last-known address, populated by the sync thread and read by the switch worker.
type PeerAddrs = Arc<Mutex<HashMap<String, SocketAddr>>>;

fn lock<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Wall-clock ms — the cross-peer-comparable clock domain for ownership LWW (§8.6).
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Resolve a switch target to an address: ourselves routes over loopback (so the one tested
/// `handle_message` path performs the lease + actuation + reconcile uniformly); a remote peer uses
/// the last address learned from discovery/sync. `None` if the peer isn't reachable yet.
fn resolve_target(
    me: &str,
    self_addr: SocketAddr,
    peer_addrs: &PeerAddrs,
    target: &str,
) -> Option<SocketAddr> {
    if target == me {
        Some(self_addr)
    } else {
        lock(peer_addrs).get(target).copied()
    }
}

/// Sleep up to `total`, waking early if `shutdown` is set.
fn sleep_until(total: Duration, shutdown: &AtomicBool) {
    let step = Duration::from_millis(200);
    let mut left = total;
    while left > Duration::ZERO && !shutdown.load(Ordering::Relaxed) {
        let s = step.min(left);
        thread::sleep(s);
        left = left.saturating_sub(s);
    }
}

/// The live agent. Build the [`Node`] (identity, secret, pins, actuator) in the UI layer where the
/// concrete DDC driver lives, then hand it here; the agent erases the actuator behind the Node.
pub struct LiveAgent {
    node: Arc<Node>,
    me: String,
    /// Friendly display name announced to peers (e.g. hostname), shown in their tray.
    name: String,
    self_addr: SocketAddr,
    manual: ManualHosts,
    mdns: Option<MdnsDiscovery>,
    peer_addrs: PeerAddrs,
    shutdown: Arc<AtomicBool>,
}

impl LiveAgent {
    pub fn new(
        node: Node,
        name: impl Into<String>,
        self_addr: SocketAddr,
        manual: ManualHosts,
        mdns: Option<MdnsDiscovery>,
    ) -> Self {
        let me = node.peer_id();
        Self {
            node: Arc::new(node),
            me,
            name: name.into(),
            self_addr,
            manual,
            mdns,
            peer_addrs: Arc::new(Mutex::new(HashMap::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Shared mesh state — wrap it in a `Controller` to render the UI.
    pub fn state(&self) -> Arc<Mutex<MeshState>> {
        self.node.state()
    }

    pub fn me(&self) -> &str {
        &self.me
    }

    /// A flag the UI sets on exit to ask the background loops to wind down (best-effort; the serve
    /// loop's blocking `accept` is reaped by process exit).
    pub fn shutdown_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Run the agent: spawn the serve + sync threads, then process switch intents until the channel
    /// closes or shutdown is requested. Intended to be called on a dedicated background thread.
    pub fn run(self, listener: TcpListener, intents: Receiver<UiIntent>) {
        let LiveAgent {
            node,
            me,
            name,
            self_addr,
            manual,
            mdns,
            peer_addrs,
            shutdown,
        } = self;

        // Accept loop on its own thread (blocks forever; reaped on process exit).
        {
            let node = Arc::clone(&node);
            thread::spawn(move || node.serve(listener));
        }

        // Sync thread: announce ourselves, then periodically learn peer addresses and push our
        // presence. Owns `mdns` so browsing stays alive for the agent's lifetime.
        {
            let node = Arc::clone(&node);
            let me = me.clone();
            let peer_addrs = Arc::clone(&peer_addrs);
            let shutdown = Arc::clone(&shutdown);
            thread::spawn(move || {
                if let Some(m) = &mdns {
                    let _ = m.announce(&me, self_addr.port());
                }
                while !shutdown.load(Ordering::Relaxed) {
                    let mut candidates: Vec<SocketAddr> =
                        manual.peers().into_iter().map(|p| p.addr).collect();
                    if let Some(m) = &mdns {
                        candidates.extend(m.peers().into_iter().map(|p| p.addr));
                    }
                    for addr in candidates {
                        if addr == self_addr {
                            continue;
                        }
                        if let Ok(mut session) = node.connect(addr) {
                            let pid = session.peer_id().to_string();
                            lock(&peer_addrs).insert(pid, addr);
                            // Push our presence; the peer's handler records us in its registry.
                            let _ = session.send(Message::Announce {
                                name: name.clone(),
                                endpoints: vec![self_addr.to_string()],
                                can_actuate: true,
                                state_version: 0,
                            });
                        }
                    }
                    sleep_until(Duration::from_secs(5), &shutdown);
                }
            });
        }

        // Switch worker: drain intents and route each as a transactional mesh round-trip.
        loop {
            match intents.recv_timeout(Duration::from_millis(500)) {
                Ok(UiIntent::Switch {
                    monitor_id,
                    target_peer_id,
                }) => {
                    route_switch(
                        &node,
                        &me,
                        self_addr,
                        &peer_addrs,
                        &monitor_id,
                        &target_peer_id,
                    );
                }
                Err(RecvTimeoutError::Timeout) => {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                }
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    }
}

/// Perform one switch: resolve the target, connect, send `SwitchCommand`, await `SwitchResult`, and
/// on success reflect the new owner into our own ownership cache (so the initiator's UI updates;
/// other peers converge via gossip/reconcile).
fn route_switch(
    node: &Node,
    me: &str,
    self_addr: SocketAddr,
    peer_addrs: &PeerAddrs,
    monitor_id: &str,
    target_peer_id: &str,
) {
    let Some(addr) = resolve_target(me, self_addr, peer_addrs, target_peer_id) else {
        eprintln!("screen-hop: target peer {target_peer_id} not reachable yet (no address)");
        return;
    };
    let mut session = match node.connect(addr) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("screen-hop: connect to {addr} failed: {e:?}");
            return;
        }
    };
    // `input_value` is advisory only — the actuator writes its OWN calibrated value (D4).
    if session
        .send(Message::SwitchCommand {
            monitor_id: monitor_id.to_owned(),
            target: target_peer_id.to_owned(),
            input_value: 0,
        })
        .is_err()
    {
        return;
    }
    match session.recv() {
        Ok(Message::SwitchResult {
            outcome, observed, ..
        }) => {
            eprintln!(
                "screen-hop: switch {monitor_id} -> {target_peer_id}: {outcome} (observed={observed:?})"
            );
            if outcome == "success" || outcome == "assumed-success" {
                lock(&node.state()).ownership.observe(
                    monitor_id,
                    Some(target_peer_id.to_owned()),
                    wall_ms(),
                );
            }
        }
        Ok(other) => eprintln!("screen-hop: unexpected reply to switch: {other:?}"),
        Err(RecvError::Io(_)) => eprintln!("screen-hop: no switch result (timeout/disconnect)"),
        Err(_) => eprintln!("screen-hop: switch reply decode error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addrs(pairs: &[(&str, &str)]) -> PeerAddrs {
        let map = pairs
            .iter()
            .map(|(p, a)| (p.to_string(), a.parse().unwrap()))
            .collect();
        Arc::new(Mutex::new(map))
    }

    #[test]
    fn resolve_routes_self_to_loopback() {
        let self_addr: SocketAddr = "127.0.0.1:7777".parse().unwrap();
        let pa = addrs(&[]);
        assert_eq!(resolve_target("me", self_addr, &pa, "me"), Some(self_addr));
    }

    #[test]
    fn resolve_uses_learned_addr_for_a_known_peer() {
        let self_addr: SocketAddr = "127.0.0.1:7777".parse().unwrap();
        let pa = addrs(&[("B", "10.0.0.5:7777")]);
        assert_eq!(
            resolve_target("me", self_addr, &pa, "B"),
            Some("10.0.0.5:7777".parse().unwrap())
        );
    }

    #[test]
    fn resolve_is_none_for_an_unknown_peer() {
        let self_addr: SocketAddr = "127.0.0.1:7777".parse().unwrap();
        let pa = addrs(&[]);
        assert_eq!(resolve_target("me", self_addr, &pa, "ghost"), None);
    }
}
