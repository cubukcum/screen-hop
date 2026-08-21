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
use screenhop_state::OwnershipState;

use crate::discovery::{merge, DiscoveredPeer, Discovery, ManualHosts, MdnsDiscovery};
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
        // Do not time this channel out independently: the platform DDC call cannot currently be
        // cancelled, so returning early would release the mesh lease while a late write could
        // still complete. The proper hardening fix is a cancellable/per-call driver boundary.
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
    /// Whether this process is allowed to perform local DDC writes. This is advertised to peers so
    /// they do not offer a read-only node as a switch target.
    can_actuate: bool,
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
        can_actuate: bool,
        self_addr: SocketAddr,
        manual: ManualHosts,
        mdns: Option<MdnsDiscovery>,
    ) -> Self {
        let me = node.peer_id();
        Self {
            node: Arc::new(node),
            me,
            name: name.into(),
            can_actuate,
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
            can_actuate,
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
                    let mut sources: Vec<&dyn Discovery> = vec![&manual];
                    if let Some(m) = &mdns {
                        sources.push(m);
                    }
                    let candidates = merge(&sources);
                    // Snapshot under the mutex, then release it before any connect/send. Replaying
                    // the current LWW facts every sync pass provides anti-entropy: peers that were
                    // offline during a switch catch up as soon as discovery can reach them again.
                    let ownership = ownership_gossip_snapshot(&node.state());
                    for candidate in candidates {
                        if is_self_candidate(&candidate, &me, self_addr) {
                            continue;
                        }
                        let _ = sync_peer(
                            &node,
                            candidate.addr,
                            &name,
                            self_addr,
                            can_actuate,
                            &peer_addrs,
                            &ownership,
                        );
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

fn announce_message(name: &str, self_addr: SocketAddr, can_actuate: bool) -> Message {
    Message::Announce {
        name: name.to_owned(),
        endpoints: vec![self_addr.to_string()],
        can_actuate,
        state_version: 0,
    }
}

fn is_self_candidate(candidate: &DiscoveredPeer, me: &str, self_addr: SocketAddr) -> bool {
    candidate.addr == self_addr || candidate.peer_id.as_deref() == Some(me)
}

/// Build a stable wire snapshot while the state mutex is held briefly. Only positive ownership is
/// replicated: calibration is intentionally peer-local, so an inactive peer can read an input it
/// cannot identify and infer `Unknown`; gossiping that negative inference could erase another
/// peer's valid owner. Stranded/DDC-disabled are richer local states that this wire message also
/// cannot represent without flattening them.
fn ownership_gossip_snapshot(state: &Arc<Mutex<MeshState>>) -> Vec<Message> {
    lock(state)
        .ownership
        .snapshot()
        .into_iter()
        .filter_map(|(monitor_id, record)| match record.state {
            OwnershipState::Owned => Some(Message::OwnershipGossip {
                monitor_id,
                owner: record.owner,
                updated_ms: record.updated_ms,
            }),
            OwnershipState::Unknown | OwnershipState::Stranded | OwnershipState::DdcDisabled => {
                None
            }
        })
        .collect()
}

/// Push presence plus the current ownership snapshot over the address that discovery actually
/// reached. This deliberately does not use `Announce.endpoints` for routing: production currently
/// advertises a loopback self-address there, whereas `addr` is the real mDNS/manual LAN endpoint.
fn sync_peer(
    node: &Node,
    addr: SocketAddr,
    name: &str,
    self_addr: SocketAddr,
    can_actuate: bool,
    peer_addrs: &PeerAddrs,
    ownership: &[Message],
) -> Option<String> {
    let mut session = node.connect(addr).ok()?;
    let peer_id = session.peer_id().to_owned();
    lock(peer_addrs).insert(peer_id.clone(), addr);

    if session
        .send(announce_message(name, self_addr, can_actuate))
        .is_err()
    {
        return None;
    }
    for gossip in ownership {
        if session.send(gossip.clone()).is_err() {
            return None;
        }
    }
    Some(peer_id)
}

fn record_switch_result(
    state: &Arc<Mutex<MeshState>>,
    monitor_id: &str,
    target_peer_id: &str,
    outcome: &str,
    updated_ms: u64,
) {
    let mut state = lock(state);
    match outcome {
        "success" | "assumed-success" => {
            state
                .ownership
                .observe(monitor_id, Some(target_peer_id.to_owned()), updated_ms);
        }
        _ => {}
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
            record_switch_result(
                &node.state(),
                monitor_id,
                target_peer_id,
                &outcome,
                wall_ms(),
            );
        }
        Ok(other) => eprintln!("screen-hop: unexpected reply to switch: {other:?}"),
        Err(RecvError::Io(_)) => eprintln!("screen-hop: no switch result (timeout/disconnect)"),
        Err(_) => eprintln!("screen-hop: switch reply decode error"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenhop_net::PeerIdentity;

    fn addrs(pairs: &[(&str, &str)]) -> PeerAddrs {
        let map = pairs
            .iter()
            .map(|(p, a)| (p.to_string(), a.parse().unwrap()))
            .collect();
        Arc::new(Mutex::new(map))
    }

    struct SuccessfulActuator;

    impl Actuator for SuccessfulActuator {
        fn switch_to_self(&mut self, _monitor_id: &str) -> ActuationReport {
            ActuationReport::new(SwitchOutcome::Success, Some(0x0F))
        }
    }

    fn wait_for_owner(state: &Arc<Mutex<MeshState>>, monitor_id: &str, expected: &str) {
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if lock(state).ownership.owner(monitor_id) == Some(expected) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let actual = lock(state).ownership.owner(monitor_id).map(str::to_owned);
        panic!("timed out waiting for {monitor_id} owner {expected}, got {actual:?}");
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

    #[test]
    fn announce_reports_the_configured_actuation_capability() {
        let addr: SocketAddr = "127.0.0.1:7777".parse().unwrap();
        assert_eq!(
            announce_message("Read-only PC", addr, false),
            Message::Announce {
                name: "Read-only PC".into(),
                endpoints: vec![addr.to_string()],
                can_actuate: false,
                state_version: 0,
            }
        );
    }

    #[test]
    fn ownership_snapshot_only_replicates_positive_owner_facts() {
        let state = Arc::new(Mutex::new(MeshState::default()));
        {
            let mut st = lock(&state);
            st.ownership.observe("owned", Some("peer-a".into()), 100);
            st.ownership.merge("unknown", None, 110);
            st.ownership.mark_stranded("stranded", 120);
            st.ownership.mark_ddc_disabled("ddc-off", 130);
        }

        let mut snapshot = ownership_gossip_snapshot(&state);
        snapshot.sort_by_key(|message| match message {
            Message::OwnershipGossip { monitor_id, .. } => monitor_id.clone(),
            _ => unreachable!(),
        });
        assert_eq!(
            snapshot,
            vec![Message::OwnershipGossip {
                monitor_id: "owned".into(),
                owner: Some("peer-a".into()),
                updated_ms: 100,
            }]
        );
    }

    #[test]
    fn switch_result_records_success_but_does_not_overclassify_ddc_unavailable() {
        let state = Arc::new(Mutex::new(MeshState::default()));

        record_switch_result(&state, "m1", "peer-b", "success", 100);
        assert_eq!(lock(&state).ownership.owner("m1"), Some("peer-b"));

        record_switch_result(&state, "m1", "peer-b", "ddc-unavailable", 200);
        let state = lock(&state);
        assert_eq!(state.ownership.owner("m1"), Some("peer-b"));
        assert_eq!(state.ownership.state("m1"), OwnershipState::Owned);
        assert_eq!(state.ownership.record("m1").unwrap().updated_ms, 100);
    }

    #[test]
    fn self_candidates_are_filtered_by_address_or_advertised_peer_id() {
        let self_addr: SocketAddr = "127.0.0.1:7777".parse().unwrap();
        let by_addr = DiscoveredPeer {
            peer_id: None,
            addr: self_addr,
            source: crate::discovery::PeerSource::Manual,
        };
        let by_id = DiscoveredPeer {
            peer_id: Some("me".into()),
            addr: "10.0.0.5:7777".parse().unwrap(),
            source: crate::discovery::PeerSource::Mdns,
        };
        let remote = DiscoveredPeer {
            peer_id: Some("peer-b".into()),
            addr: "10.0.0.6:7777".parse().unwrap(),
            source: crate::discovery::PeerSource::Mdns,
        };

        assert!(is_self_candidate(&by_addr, "me", self_addr));
        assert!(is_self_candidate(&by_id, "me", self_addr));
        assert!(!is_self_candidate(&remote, "me", self_addr));
    }

    #[test]
    fn successful_remote_switch_snapshot_converges_three_peer_mesh() {
        let listener_a = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr_a = listener_a.local_addr().unwrap();
        let listener_b = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr_b = listener_b.local_addr().unwrap();
        let listener_c = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr_c = listener_c.local_addr().unwrap();

        let node_a = Arc::new(Node::new(PeerIdentity::generate(), "mesh"));
        let node_b =
            Arc::new(Node::new(PeerIdentity::generate(), "mesh").with_actuator(SuccessfulActuator));
        let node_c = Arc::new(Node::new(PeerIdentity::generate(), "mesh"));
        let state_a = node_a.state();
        let state_b = node_b.state();
        let state_c = node_c.state();
        let id_b = node_b.peer_id();

        for (node, listener) in [
            (Arc::clone(&node_a), listener_a),
            (Arc::clone(&node_b), listener_b),
            (Arc::clone(&node_c), listener_c),
        ] {
            thread::spawn(move || node.serve(listener));
        }

        // A performs the established transactional remote switch against B.
        let mut switch_session = node_a.connect(addr_b).unwrap();
        switch_session
            .send(Message::SwitchCommand {
                monitor_id: "m1".into(),
                target: id_b.clone(),
                input_value: 0,
            })
            .unwrap();
        match switch_session.recv().unwrap() {
            Message::SwitchResult {
                monitor_id,
                outcome,
                observed,
            } => {
                assert_eq!(monitor_id, "m1");
                assert_eq!(outcome, "success");
                assert_eq!(observed, Some(0x0F));
            }
            other => panic!("expected SwitchResult, got {other:?}"),
        }

        // The next production sync pass snapshots B's newly-observed owner, then reaches A and C
        // through discovered addresses (not the loopback endpoint carried in Announce).
        let snapshot = ownership_gossip_snapshot(&state_b);
        let learned = addrs(&[]);
        assert_eq!(
            sync_peer(&node_b, addr_a, "Peer B", addr_b, true, &learned, &snapshot),
            Some(node_a.peer_id())
        );
        assert_eq!(
            sync_peer(&node_b, addr_c, "Peer B", addr_b, true, &learned, &snapshot),
            Some(node_c.peer_id())
        );

        wait_for_owner(&state_b, "m1", &id_b);
        wait_for_owner(&state_a, "m1", &id_b);
        wait_for_owner(&state_c, "m1", &id_b);
        assert_eq!(lock(&learned).len(), 2);
    }
}
