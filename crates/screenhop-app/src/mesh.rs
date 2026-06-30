//! The per-peer mesh node: accept loop + handshake + message handling, wiring
//! `screenhop-net` (transport/identity) to `screenhop-state` (ownership/locks).

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use screenhop_core::SwitchOutcome;
use screenhop_net::{
    handshake, HandshakeError, Message, PeerIdentity, PinStore, RecvError, SecureChannel,
    SecureConnection, Verified,
};
use screenhop_state::{LockManager, LockOutcome, OwnershipMap, DEFAULT_LEASE_MS};

use crate::peers::PeerRegistry;

/// Inbound read timeout — must be shorter than the 30 s lease TTL (D5) and bounds the
/// pre-auth stall an unpaired host could cause (net security review, HIGH finding).
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Outbound TCP connect timeout. Discovery often surfaces multiple addresses per peer (real LAN IP
/// plus virtual adapters from Hyper-V/VMware/WSL/VPNs); a dead one would otherwise block on the OS
/// SYN timeout (~20 s) and stall the discovery/refresh loop past the liveness window. Fail fast.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);

/// Cap on concurrent inbound connection-handler threads. The accept loop spawns one thread per
/// connection *before* the handshake, so without a cap an unpaired LAN host (the v1 threat model)
/// could open thousands of sockets and exhaust threads/memory pre-auth. A one-operator mesh needs
/// only a handful of real connections, so this is generous while still bounding the blast radius.
const MAX_CONNECTIONS: usize = 32;

/// Recover a mutex guard even if a previous holder panicked. The replicated state is plain data
/// (no broken invariant survives a panic mid-update that matters here), so poisoning must NOT
/// permanently brick every future switch on this node — we take the inner guard and carry on.
fn lock_or_recover<T: ?Sized>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Monotonic milliseconds since the node started — the clock domain for this node's own leases.
fn elapsed_ms(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// Wall-clock UTC milliseconds — the cross-peer-comparable clock domain for ownership LWW (§8.6).
fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Stable, wire-safe outcome code (never `Debug` formatting, which is not a stable contract).
fn outcome_code(o: SwitchOutcome) -> &'static str {
    match o {
        SwitchOutcome::Success => "success",
        SwitchOutcome::AssumedSuccessReadbackInconclusive => "assumed-success",
        SwitchOutcome::Failed => "failed",
        SwitchOutcome::BlockedValue => "blocked-value",
        SwitchOutcome::NeedsCalibration => "needs-calibration",
        SwitchOutcome::DdcUnavailable => "ddc-unavailable",
        SwitchOutcome::Unsupported => "unsupported",
    }
}

/// Performs this peer's local DDC actuation. The production impl wraps the M1 `SwitchExecutor` +
/// a `MonitorDriver` + the calibration allow-list; tests use a fake.
pub trait Actuator: Send {
    /// Make THIS machine the active source on `monitor_id` (pull-to-self). Returns the outcome and
    /// the observed `0x60` value if the panel reported one (for ground-truth reconciliation).
    fn switch_to_self(&mut self, monitor_id: &str) -> ActuationReport;
}

/// What an [`Actuator`] reports back: the outcome plus any observed live input value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActuationReport {
    pub outcome: SwitchOutcome,
    pub observed: Option<u32>,
}

impl ActuationReport {
    pub fn new(outcome: SwitchOutcome, observed: Option<u32>) -> Self {
        Self { outcome, observed }
    }
}

type SharedActuator = Arc<Mutex<dyn Actuator>>;

/// Shared, replicated mesh state behind one mutex.
#[derive(Default)]
pub struct MeshState {
    pub ownership: OwnershipMap,
    pub locks: LockManager,
    pub pins: PinStore,
    /// Peer presence/liveness, updated from Announce/Heartbeat (M3) and read by the partition guard.
    pub peers: PeerRegistry,
}

/// A mesh peer: its identity, the derived group key, shared state, and a monotonic clock origin.
pub struct Node {
    me: Arc<PeerIdentity>,
    key: Arc<[u8; 32]>,
    state: Arc<Mutex<MeshState>>,
    start: Instant,
    actuator: Option<SharedActuator>,
    /// Where the TOFU pin map is persisted, if configured (durable across restarts — D2).
    pins_path: Option<PathBuf>,
    /// Live count of in-flight connection-handler threads (DoS cap, see [`MAX_CONNECTIONS`]).
    active_conns: Arc<AtomicUsize>,
}

#[derive(Debug)]
pub enum ConnectError {
    Io(io::Error),
    Handshake(HandshakeError),
    /// The peer's identity key differs from the one previously pinned.
    PinMismatch,
}

impl From<io::Error> for ConnectError {
    fn from(e: io::Error) -> Self {
        ConnectError::Io(e)
    }
}

impl Node {
    pub fn new(identity: PeerIdentity, passphrase: &str) -> Self {
        let key = screenhop_net::derive_group_key(passphrase);
        Self {
            me: Arc::new(identity),
            key: Arc::new(key),
            state: Arc::new(Mutex::new(MeshState::default())),
            start: Instant::now(),
            actuator: None,
            pins_path: None,
            active_conns: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Attach the local DDC actuator so this node can perform switches it is asked to make.
    pub fn with_actuator<A: Actuator + 'static>(mut self, actuator: A) -> Self {
        let shared: SharedActuator = Arc::new(Mutex::new(actuator));
        self.actuator = Some(shared);
        self
    }

    /// Persist (and load) the TOFU pin store at `path`, so pinned peer keys — and the
    /// revocation / MITM-detection guarantee they provide (D2) — survive process restarts.
    pub fn with_pin_store(mut self, path: PathBuf) -> Self {
        if let Ok(loaded) = PinStore::load_from(&path) {
            lock_or_recover(&self.state).pins = loaded;
        }
        self.pins_path = Some(path);
        self
    }

    pub fn peer_id(&self) -> String {
        self.me.peer_id()
    }

    pub fn state(&self) -> Arc<Mutex<MeshState>> {
        Arc::clone(&self.state)
    }

    /// Blocking accept loop: handshake each inbound connection, then handle its messages on a
    /// dedicated thread — up to [`MAX_CONNECTIONS`] concurrently (excess connections are dropped).
    pub fn serve(&self, listener: TcpListener) {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };

            // Reserve a slot; if we're at the cap, close this connection rather than spawning an
            // unbounded thread (pre-auth DoS guard).
            let n = self.active_conns.fetch_add(1, Ordering::AcqRel);
            if n >= MAX_CONNECTIONS {
                self.active_conns.fetch_sub(1, Ordering::AcqRel);
                drop(stream); // closes the socket
                continue;
            }

            let me = Arc::clone(&self.me);
            let key = Arc::clone(&self.key);
            let state = Arc::clone(&self.state);
            let start = self.start;
            let actuator = self.actuator.clone();
            let pins_path = self.pins_path.clone();
            let counter = Arc::clone(&self.active_conns);
            thread::spawn(move || {
                let _slot = ConnSlot(counter); // releases the slot on thread exit
                let _ = handle_connection(
                    stream,
                    &me,
                    &key,
                    &state,
                    start,
                    actuator,
                    pins_path.as_deref(),
                );
            });
        }
    }

    /// Connect to a peer, complete the mutual handshake, and return a [`Session`] for messaging.
    pub fn connect(&self, addr: SocketAddr) -> Result<Session<TcpStream>, ConnectError> {
        let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;

        let channel = SecureChannel::new(&self.key);
        let verified = run_handshake(
            &mut stream,
            &channel,
            &self.me,
            &self.state,
            self.pins_path.as_deref(),
        )
        .map_err(|e| match e {
            RunHsError::Handshake(h) => ConnectError::Handshake(h),
            RunHsError::PinMismatch => ConnectError::PinMismatch,
        })?;

        Ok(Session {
            conn: SecureConnection::new(stream, channel),
            me: self.me.peer_id(),
            peer: verified,
        })
    }
}

/// RAII slot that decrements the live-connection counter when a handler thread exits.
struct ConnSlot(Arc<AtomicUsize>);
impl Drop for ConnSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// An authenticated session with one peer.
pub struct Session<S> {
    conn: SecureConnection<S>,
    me: String,
    peer: Verified,
}

impl<S: Read + Write> Session<S> {
    pub fn peer_id(&self) -> &str {
        &self.peer.peer_id
    }

    pub fn send(&mut self, body: Message) -> io::Result<()> {
        self.conn.send(&self.me, body)
    }

    /// Receive the next message that is genuinely from the handshake-verified peer (frames whose
    /// asserted `from` doesn't match the verified identity are dropped). Each iteration blocks on
    /// the stream, so a mismatched frame costs one read, not a CPU spin.
    pub fn recv(&mut self) -> Result<Message, RecvError> {
        loop {
            let env = self.conn.recv()?;
            if env.from == self.peer.peer_id {
                return Ok(env.body);
            }
        }
    }
}

// ---- connection handling ----------------------------------------------------

enum RunHsError {
    Handshake(HandshakeError),
    PinMismatch,
}

/// Run the handshake against a throwaway pin store, then enforce TOFU against the shared store
/// (briefly locked — no network I/O is held under the lock). A newly-pinned peer is written
/// through to `pins_path` so the pin survives a restart.
fn run_handshake<S: Read + Write>(
    stream: &mut S,
    channel: &SecureChannel,
    me: &PeerIdentity,
    state: &Arc<Mutex<MeshState>>,
    pins_path: Option<&Path>,
) -> Result<Verified, RunHsError> {
    let mut local_pins = PinStore::new();
    let verified =
        handshake(stream, channel, me, &mut local_pins).map_err(RunHsError::Handshake)?;
    let mut st = lock_or_recover(state);
    let newly_pinned = !st.pins.is_pinned(&verified.peer_id);
    if st
        .pins
        .check_or_pin(&verified.peer_id, verified.public_bytes)
        .is_err()
    {
        return Err(RunHsError::PinMismatch);
    }
    if newly_pinned {
        if let Some(path) = pins_path {
            let _ = st.pins.save_to(path);
        }
    }
    Ok(verified)
}

fn handle_connection(
    mut stream: TcpStream,
    me: &PeerIdentity,
    key: &[u8; 32],
    state: &Arc<Mutex<MeshState>>,
    start: Instant,
    actuator: Option<SharedActuator>,
    pins_path: Option<&Path>,
) -> Result<(), ()> {
    stream
        .set_read_timeout(Some(READ_TIMEOUT))
        .map_err(|_| ())?;
    let channel = SecureChannel::new(key);
    let verified = run_handshake(&mut stream, &channel, me, state, pins_path).map_err(|_| ())?;

    let my_id = me.peer_id();
    let mut conn = SecureConnection::new(stream, channel);
    loop {
        match conn.recv() {
            Ok(env) => {
                // Identity binding: only act on frames from the verified peer.
                if env.from != verified.peer_id {
                    continue;
                }
                if let Some(reply) = handle_message(
                    state,
                    &verified.peer_id,
                    &my_id,
                    env.body,
                    start,
                    actuator.as_ref(),
                ) {
                    if conn.send(&my_id, reply).is_err() {
                        break;
                    }
                }
            }
            Err(RecvError::Io(_)) => break, // disconnect or read timeout
            Err(_) => continue,             // decrypt/parse/replay: drop this frame
        }
    }
    Ok(())
}

fn handle_message(
    state: &Arc<Mutex<MeshState>>,
    from: &str,
    my_id: &str,
    body: Message,
    start: Instant,
    actuator: Option<&SharedActuator>,
) -> Option<Message> {
    match body {
        Message::Announce {
            name,
            endpoints,
            can_actuate,
            state_version,
        } => {
            // Peer liveness is read by other layers (UI/reconcile) using wall-clock ms, so it must
            // be stamped in the SAME domain — not the node-monotonic `elapsed_ms` (which would make
            // every peer look ~decades stale and the mesh permanently "degraded").
            let now = wall_ms();
            lock_or_recover(state).peers.observe_announce(
                from,
                name,
                endpoints,
                can_actuate,
                state_version,
                now,
            );
            None
        }
        Message::Heartbeat { state_version } => {
            let now = wall_ms();
            lock_or_recover(state)
                .peers
                .observe_heartbeat(from, state_version, now);
            None
        }
        Message::OwnershipGossip {
            monitor_id,
            owner,
            updated_ms,
        } => {
            lock_or_recover(state)
                .ownership
                .merge(&monitor_id, owner, updated_ms);
            None
        }
        Message::LockRequest {
            monitor_id,
            lease_secs,
        } => {
            let now = elapsed_ms(start);
            let lease_ms = u64::from(lease_secs) * 1000;
            let mut st = lock_or_recover(state);
            match st.locks.acquire(&monitor_id, from, now, lease_ms) {
                LockOutcome::Granted(l) => Some(Message::LockGrant {
                    monitor_id,
                    holder: from.to_owned(),
                    // Ship the RELATIVE duration; the requester anchors it to its own clock (D5).
                    lease_ms: l.granted_ms,
                }),
                LockOutcome::Denied { current_holder } => Some(Message::LockDeny {
                    monitor_id,
                    reason: format!("held by {current_holder}"),
                }),
            }
        }
        Message::SwitchCommand {
            monitor_id, target, ..
        } => {
            // Only the chosen actuator (the target, for pull-to-self) performs the write.
            if target != my_id {
                return Some(switch_result(&monitor_id, "not-actuator"));
            }
            let Some(actuator) = actuator else {
                return Some(switch_result(&monitor_id, "no-actuator"));
            };

            // D1/D5: serialize actuation behind the per-monitor lease. Acquire (or renew) THIS
            // node's own lease before writing; refuse if another peer currently holds it, so two
            // actuators can never race a 0x60 write on the same panel.
            let now = elapsed_ms(start);
            {
                let mut st = lock_or_recover(state);
                if let LockOutcome::Denied { .. } =
                    st.locks.acquire(&monitor_id, my_id, now, DEFAULT_LEASE_MS)
                {
                    return Some(switch_result(&monitor_id, "locked-by-other"));
                }
            } // release the state lock before the slow DDC write

            let report = lock_or_recover(actuator).switch_to_self(&monitor_id);
            eprintln!(
                "screen-hop: actuate {monitor_id} -> {} (observed={:?})",
                outcome_code(report.outcome),
                report.observed
            );

            {
                let mut st = lock_or_recover(state);
                if report.outcome.is_effective_success() {
                    // Reconcile: this machine now drives the panel (wall-clock ts for cross-peer LWW).
                    st.ownership
                        .observe(&monitor_id, Some(my_id.to_owned()), wall_ms());
                } else if report.outcome == SwitchOutcome::DdcUnavailable {
                    // DDC/CI is off in the OSD — record the distinct, persistent DDC-disabled state
                    // so the UX can tell the user to re-enable it (M4.4d) rather than show "unknown".
                    st.ownership.mark_ddc_disabled(&monitor_id, wall_ms());
                }
                // Hand the lease back now that the (best-effort) switch is done.
                st.locks.release(&monitor_id, my_id);
            }

            Some(Message::SwitchResult {
                monitor_id,
                outcome: outcome_code(report.outcome).to_owned(),
                observed: report.observed,
            })
        }
        _ => None,
    }
}

fn switch_result(monitor_id: &str, outcome: &str) -> Message {
    Message::SwitchResult {
        monitor_id: monitor_id.to_owned(),
        outcome: outcome.to_owned(),
        observed: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenhop_state::DEFAULT_LEASE_MS;

    fn node(pass: &str) -> Node {
        Node::new(PeerIdentity::generate(), pass)
    }

    struct FakeActuator {
        switched: Arc<Mutex<Vec<String>>>,
    }
    impl Actuator for FakeActuator {
        fn switch_to_self(&mut self, monitor_id: &str) -> ActuationReport {
            self.switched.lock().unwrap().push(monitor_id.to_owned());
            ActuationReport::new(SwitchOutcome::Success, Some(0x0F))
        }
    }

    #[test]
    fn remote_switch_command_actuates_and_reconciles_ownership() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let switched = Arc::new(Mutex::new(Vec::new()));
        let node_b = node("mesh").with_actuator(FakeActuator {
            switched: Arc::clone(&switched),
        });
        let b_state = node_b.state();
        let b_id = node_b.peer_id();
        thread::spawn(move || node_b.serve(listener));

        let node_a = node("mesh");
        let mut session = node_a.connect(addr).unwrap();
        session
            .send(Message::SwitchCommand {
                monitor_id: "m1".into(),
                target: b_id.clone(),
                input_value: 0x0F,
            })
            .unwrap();

        match session.recv().unwrap() {
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

        assert_eq!(switched.lock().unwrap().as_slice(), ["m1".to_owned()]);
        assert_eq!(
            b_state.lock().unwrap().ownership.owner("m1"),
            Some(b_id.as_str())
        );
        // The lease was released after the switch, so the monitor is free again.
        assert_eq!(b_state.lock().unwrap().locks.holder("m1", 0), None);
    }

    #[test]
    fn switch_command_is_refused_when_monitor_is_locked_by_another_peer() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let switched = Arc::new(Mutex::new(Vec::new()));
        let node_b = node("mesh").with_actuator(FakeActuator {
            switched: Arc::clone(&switched),
        });
        let b_state = node_b.state();
        let b_id = node_b.peer_id();
        // Some other peer already holds m1's lease on B.
        b_state
            .lock()
            .unwrap()
            .locks
            .acquire("m1", "other-peer", 0, DEFAULT_LEASE_MS);
        thread::spawn(move || node_b.serve(listener));

        let node_a = node("mesh");
        let mut session = node_a.connect(addr).unwrap();
        session
            .send(Message::SwitchCommand {
                monitor_id: "m1".into(),
                target: b_id.clone(),
                input_value: 0x0F,
            })
            .unwrap();

        match session.recv().unwrap() {
            Message::SwitchResult {
                monitor_id,
                outcome,
                ..
            } => {
                assert_eq!(monitor_id, "m1");
                assert_eq!(outcome, "locked-by-other");
            }
            other => panic!("expected SwitchResult, got {other:?}"),
        }
        // The actuator must NOT have been invoked.
        assert!(
            switched.lock().unwrap().is_empty(),
            "no write may occur without the lease"
        );
    }

    #[test]
    fn two_peers_gossip_and_lock_over_the_mesh() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let node_b = node("mesh");
        let b_state = node_b.state();
        let b_id = node_b.peer_id();
        thread::spawn(move || node_b.serve(listener));

        let node_a = node("mesh");
        let a_id = node_a.peer_id();
        let mut session = node_a.connect(addr).expect("connect + handshake");
        assert_eq!(session.peer_id(), b_id);

        // A claims it owns mon1, then asks B for the lock on mon1.
        session
            .send(Message::OwnershipGossip {
                monitor_id: "mon1".into(),
                owner: Some(a_id.clone()),
                updated_ms: 100,
            })
            .unwrap();
        session
            .send(Message::LockRequest {
                monitor_id: "mon1".into(),
                lease_secs: 30,
            })
            .unwrap();

        match session.recv().unwrap() {
            Message::LockGrant {
                monitor_id,
                holder,
                lease_ms,
            } => {
                assert_eq!(monitor_id, "mon1");
                assert_eq!(holder, a_id);
                assert!(lease_ms >= 30_000, "relative ttl shipped, got {lease_ms}");
            }
            other => panic!("expected LockGrant, got {other:?}"),
        }

        // The reply is a barrier: B has now applied both messages.
        let st = b_state.lock().unwrap();
        assert_eq!(st.ownership.owner("mon1"), Some(a_id.as_str()));
        assert_eq!(st.locks.holder("mon1", 0), Some(a_id.as_str()));
    }

    /// An actuator that blocks inside `switch_to_self` until released — to simulate a long
    /// (e.g. ~10 s DisplayPort push-release) hang deterministically, without real sleeps.
    struct BlockingActuator {
        started: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }
    impl Actuator for BlockingActuator {
        fn switch_to_self(&mut self, _monitor_id: &str) -> ActuationReport {
            self.started.send(()).unwrap();
            let _ = self.release.lock().unwrap().recv(); // hang here, lease held
            ActuationReport::new(SwitchOutcome::Success, Some(0x0F))
        }
    }

    #[test]
    fn lease_is_held_for_the_whole_switch_and_blocks_other_peers_midway() {
        // D1/D5: a peer holds the per-monitor lease for the ENTIRE switch — including a long DDC
        // hang — so no other peer can grab it and race a second 0x60 write mid-switch.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let node_b = node("mesh").with_actuator(BlockingActuator {
            started: started_tx,
            release: Mutex::new(release_rx),
        });
        let b_id = node_b.peer_id();
        thread::spawn(move || node_b.serve(listener));

        // Peer A triggers the switch; B acquires the lease, then blocks inside the actuator.
        let node_a = node("mesh");
        let mut sa = node_a.connect(addr).unwrap();
        sa.send(Message::SwitchCommand {
            monitor_id: "m1".into(),
            target: b_id.clone(),
            input_value: 0x0F,
        })
        .unwrap();
        started_rx.recv().unwrap(); // B is now mid-switch with the lease held

        // Peer C asks for the same monitor's lease WHILE B is mid-switch -> must be denied.
        let node_c = node("mesh");
        let mut sc = node_c.connect(addr).unwrap();
        sc.send(Message::LockRequest {
            monitor_id: "m1".into(),
            lease_secs: 30,
        })
        .unwrap();
        match sc.recv().unwrap() {
            Message::LockDeny { monitor_id, .. } => assert_eq!(monitor_id, "m1"),
            other => panic!("lease must be denied mid-switch, got {other:?}"),
        }

        // Finish the switch; A gets its success.
        release_tx.send(()).unwrap();
        match sa.recv().unwrap() {
            Message::SwitchResult { outcome, .. } => assert_eq!(outcome, "success"),
            other => panic!("expected SwitchResult, got {other:?}"),
        }

        // The lease was released after the switch — C can acquire it now.
        sc.send(Message::LockRequest {
            monitor_id: "m1".into(),
            lease_secs: 30,
        })
        .unwrap();
        match sc.recv().unwrap() {
            Message::LockGrant { monitor_id, .. } => assert_eq!(monitor_id, "m1"),
            other => panic!("lease should be free after the switch, got {other:?}"),
        }
    }

    struct DdcDisabledActuator;
    impl Actuator for DdcDisabledActuator {
        fn switch_to_self(&mut self, _monitor_id: &str) -> ActuationReport {
            ActuationReport::new(SwitchOutcome::DdcUnavailable, None)
        }
    }

    #[test]
    fn switch_reporting_ddc_unavailable_marks_panel_ddc_disabled() {
        use screenhop_state::OwnershipState;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let node_b = node("mesh").with_actuator(DdcDisabledActuator);
        let b_state = node_b.state();
        let b_id = node_b.peer_id();
        thread::spawn(move || node_b.serve(listener));

        let node_a = node("mesh");
        let mut s = node_a.connect(addr).unwrap();
        s.send(Message::SwitchCommand {
            monitor_id: "m1".into(),
            target: b_id.clone(),
            input_value: 0x0F,
        })
        .unwrap();
        match s.recv().unwrap() {
            Message::SwitchResult { outcome, .. } => assert_eq!(outcome, "ddc-unavailable"),
            other => panic!("expected SwitchResult, got {other:?}"),
        }
        // The panel is now in the distinct, persistent DDC-disabled state (not Unknown/Owned).
        assert_eq!(
            b_state.lock().unwrap().ownership.state("m1"),
            OwnershipState::DdcDisabled
        );
    }

    #[test]
    fn announce_updates_the_peer_registry() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let node_b = node("mesh");
        let b_state = node_b.state();
        thread::spawn(move || node_b.serve(listener));

        let node_a = node("mesh");
        let a_id = node_a.peer_id();
        let mut s = node_a.connect(addr).unwrap();
        s.send(Message::Announce {
            name: "Work PC".into(),
            endpoints: vec!["192.168.1.5:7777".into()],
            can_actuate: true,
            state_version: 7,
        })
        .unwrap();
        // Announce has no reply; use a LockRequest's reply as a barrier (same connection, ordered).
        s.send(Message::LockRequest {
            monitor_id: "m1".into(),
            lease_secs: 30,
        })
        .unwrap();
        let _ = s.recv().unwrap();

        let st = b_state.lock().unwrap();
        let p = st.peers.get(&a_id).expect("peer recorded from announce");
        assert_eq!(p.name, "Work PC");
        assert!(p.can_actuate);
        assert_eq!(p.state_version, 7);
    }

    #[test]
    fn connect_fails_on_wrong_passphrase() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let node_b = node("RIGHT");
        thread::spawn(move || node_b.serve(listener));

        let node_a = node("WRONG");
        assert!(node_a.connect(addr).is_err());
    }
}
