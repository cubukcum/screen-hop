//! The per-peer mesh node: accept loop + handshake + message handling, wiring
//! `screenhop-net` (transport/identity) to `screenhop-state` (ownership/locks).

use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use screenhop_net::{
    handshake, HandshakeError, Message, PeerIdentity, PinStore, RecvError, SecureChannel,
    SecureConnection, Verified,
};
use screenhop_core::SwitchOutcome;
use screenhop_state::{LockManager, LockOutcome, OwnershipMap};

/// Inbound read timeout — must be shorter than the 30 s lease TTL (D5) and bounds the
/// pre-auth stall an unpaired host could cause (net security review, HIGH finding).
const READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Performs this peer's local DDC actuation. The production impl wraps the M1 `SwitchExecutor` +
/// a `MonitorDriver` + the calibration allow-list; tests use a fake.
pub trait Actuator: Send {
    /// Make THIS machine the active source on `monitor_id` (pull-to-self).
    fn switch_to_self(&mut self, monitor_id: &str) -> SwitchOutcome;
}

type SharedActuator = Arc<Mutex<dyn Actuator>>;

/// Shared, replicated mesh state behind one mutex.
#[derive(Default)]
pub struct MeshState {
    pub ownership: OwnershipMap,
    pub locks: LockManager,
    pub pins: PinStore,
}

/// A mesh peer: its identity, the derived group key, shared state, and a monotonic clock origin.
pub struct Node {
    me: Arc<PeerIdentity>,
    key: Arc<[u8; 32]>,
    state: Arc<Mutex<MeshState>>,
    start: Instant,
    actuator: Option<SharedActuator>,
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
        }
    }

    /// Attach the local DDC actuator so this node can perform switches it is asked to make.
    pub fn with_actuator<A: Actuator + 'static>(mut self, actuator: A) -> Self {
        let shared: SharedActuator = Arc::new(Mutex::new(actuator));
        self.actuator = Some(shared);
        self
    }

    pub fn peer_id(&self) -> String {
        self.me.peer_id()
    }

    pub fn state(&self) -> Arc<Mutex<MeshState>> {
        Arc::clone(&self.state)
    }

    /// Blocking accept loop: handshake each inbound connection, then handle its messages on a
    /// dedicated thread.
    pub fn serve(&self, listener: TcpListener) {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let me = Arc::clone(&self.me);
            let key = Arc::clone(&self.key);
            let state = Arc::clone(&self.state);
            let start = self.start;
            let actuator = self.actuator.clone();
            thread::spawn(move || {
                let _ = handle_connection(stream, &me, &key, &state, start, actuator);
            });
        }
    }

    /// Connect to a peer, complete the mutual handshake, and return a [`Session`] for messaging.
    pub fn connect(&self, addr: SocketAddr) -> Result<Session<TcpStream>, ConnectError> {
        let mut stream = TcpStream::connect(addr)?;
        stream.set_read_timeout(Some(READ_TIMEOUT))?;

        let channel = SecureChannel::new(&self.key);
        let verified = run_handshake(&mut stream, &channel, &self.me, &self.state).map_err(|e| match e {
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
    /// asserted `from` doesn't match the verified identity are dropped).
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
/// (briefly locked — no network I/O is held under the lock).
fn run_handshake<S: Read + Write>(
    stream: &mut S,
    channel: &SecureChannel,
    me: &PeerIdentity,
    state: &Arc<Mutex<MeshState>>,
) -> Result<Verified, RunHsError> {
    let mut local_pins = PinStore::new();
    let verified = handshake(stream, channel, me, &mut local_pins).map_err(RunHsError::Handshake)?;
    let mut st = state.lock().expect("state mutex");
    if st.pins.check_or_pin(&verified.peer_id, verified.public_bytes).is_err() {
        return Err(RunHsError::PinMismatch);
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
) -> Result<(), ()> {
    stream.set_read_timeout(Some(READ_TIMEOUT)).map_err(|_| ())?;
    let channel = SecureChannel::new(key);
    let verified = run_handshake(&mut stream, &channel, me, state).map_err(|_| ())?;

    let my_id = me.peer_id();
    let mut conn = SecureConnection::new(stream, channel);
    loop {
        match conn.recv() {
            Ok(env) => {
                // Identity binding: only act on frames from the verified peer.
                if env.from != verified.peer_id {
                    continue;
                }
                if let Some(reply) =
                    handle_message(state, &verified.peer_id, &my_id, env.body, start, actuator.as_ref())
                {
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
        Message::OwnershipGossip {
            monitor_id,
            owner,
            updated_ms,
        } => {
            state.lock().expect("state").ownership.merge(&monitor_id, owner, updated_ms);
            None
        }
        Message::LockRequest {
            monitor_id,
            lease_secs,
        } => {
            let now = start.elapsed().as_millis() as u64;
            let lease_ms = u64::from(lease_secs) * 1000;
            let mut st = state.lock().expect("state");
            match st.locks.acquire(&monitor_id, from, now, lease_ms) {
                LockOutcome::Granted(l) => Some(Message::LockGrant {
                    monitor_id,
                    holder: from.to_owned(),
                    lease_expires_ms: l.expires_ms,
                }),
                LockOutcome::Denied { current_holder } => Some(Message::LockDeny {
                    monitor_id,
                    reason: format!("held by {current_holder}"),
                }),
            }
        }
        Message::SwitchCommand { monitor_id, target, .. } => {
            // Only the chosen actuator (the target, for pull-to-self) performs the write.
            if target != my_id {
                return Some(Message::SwitchResult {
                    monitor_id,
                    outcome: "not-actuator".to_owned(),
                    observed: None,
                });
            }
            let Some(actuator) = actuator else {
                return Some(Message::SwitchResult {
                    monitor_id,
                    outcome: "no-actuator".to_owned(),
                    observed: None,
                });
            };

            let outcome = actuator.lock().expect("actuator").switch_to_self(&monitor_id);
            if outcome.is_effective_success() {
                // Reconcile: this machine now drives the panel.
                let now = start.elapsed().as_millis() as u64;
                state
                    .lock()
                    .expect("state")
                    .ownership
                    .observe(&monitor_id, Some(my_id.to_owned()), now);
            }
            Some(Message::SwitchResult {
                monitor_id,
                outcome: format!("{outcome:?}"),
                observed: None,
            })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(pass: &str) -> Node {
        Node::new(PeerIdentity::generate(), pass)
    }

    struct FakeActuator {
        switched: Arc<Mutex<Vec<String>>>,
    }
    impl Actuator for FakeActuator {
        fn switch_to_self(&mut self, monitor_id: &str) -> SwitchOutcome {
            self.switched.lock().unwrap().push(monitor_id.to_owned());
            SwitchOutcome::Success
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
            Message::SwitchResult { monitor_id, outcome, .. } => {
                assert_eq!(monitor_id, "m1");
                assert!(outcome.contains("Success"), "outcome was {outcome}");
            }
            other => panic!("expected SwitchResult, got {other:?}"),
        }

        assert_eq!(switched.lock().unwrap().as_slice(), ["m1".to_owned()]);
        assert_eq!(b_state.lock().unwrap().ownership.owner("m1"), Some(b_id.as_str()));
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
                monitor_id, holder, ..
            } => {
                assert_eq!(monitor_id, "mon1");
                assert_eq!(holder, a_id);
            }
            other => panic!("expected LockGrant, got {other:?}"),
        }

        // The reply is a barrier: B has now applied both messages.
        let st = b_state.lock().unwrap();
        assert_eq!(st.ownership.owner("mon1"), Some(a_id.as_str()));
        assert_eq!(st.locks.holder("mon1", 0), Some(a_id.as_str()));
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
