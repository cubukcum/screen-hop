//! M2 integration test: the SAME physical panel, enumerated by two different backends on two PCs,
//! must resolve to the SAME `monitor_id`, and that correlated id must route a switch to the correct
//! actuator across a real (in-proc) 2-peer mesh.
//!
//! This closes the audit gap that the mesh tests used synthetic `"m1"` ids rather than a real
//! cross-backend-correlated `MonitorFingerprint`.

use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;

use screenhop_app::{ActuationReport, Actuator, Node};
use screenhop_core::SwitchOutcome;
use screenhop_identity::MonitorFingerprint;
use screenhop_net::{Message, PeerIdentity};

/// Minimal valid 128-byte EDID base block for "AOC", product 0x1234, with a numeric serial.
/// Mirrors a panel a Linux/raw-EDID backend would hand us.
fn aoc_edid(numeric_serial: u32) -> Vec<u8> {
    let mut e = vec![0u8; 128];
    e[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
    // "AOC" packed -> 0x05E3.
    e[8] = 0x05;
    e[9] = 0xE3;
    e[10..12].copy_from_slice(&0x1234u16.to_le_bytes());
    e[12..16].copy_from_slice(&numeric_serial.to_le_bytes());
    e[16] = 10; // week
    e[17] = (2021 - 1990) as u8; // year byte
    e
}

/// A stand-in for the local DDC actuator that just records the switch and reports success.
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
fn same_panel_correlates_across_backends_and_routes_a_switch() {
    // PC-A (Linux/raw EDID) and PC-B (Windows/parts) see the same physical AOC panel.
    let via_edid = MonitorFingerprint::from_edid(&aoc_edid(1598)).expect("valid edid");
    let via_parts = MonitorFingerprint::from_parts("AOC", 0x1234, 1598, None);

    // The cross-PC join key MUST agree, despite different backends / available fields.
    assert_eq!(
        via_edid.monitor_id(),
        via_parts.monitor_id(),
        "the same panel must get the same id regardless of enumerating backend"
    );
    let monitor_id = via_edid.monitor_id();

    // Now drive a switch over a real 2-peer mesh, keyed by that correlated id.
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();

    let switched = Arc::new(Mutex::new(Vec::new()));
    let node_b = Node::new(PeerIdentity::generate(), "mesh").with_actuator(FakeActuator {
        switched: Arc::clone(&switched),
    });
    let b_state = node_b.state();
    let b_id = node_b.peer_id();
    thread::spawn(move || node_b.serve(listener));

    let node_a = Node::new(PeerIdentity::generate(), "mesh");
    let mut session = node_a.connect(addr).expect("connect + handshake");

    session
        .send(Message::SwitchCommand {
            monitor_id: monitor_id.clone(),
            target: b_id.clone(),
            input_value: 0x0F,
        })
        .unwrap();

    match session.recv().unwrap() {
        Message::SwitchResult {
            monitor_id: got,
            outcome,
            observed,
        } => {
            assert_eq!(got, monitor_id);
            assert_eq!(outcome, "success");
            assert_eq!(observed, Some(0x0F));
        }
        other => panic!("expected SwitchResult, got {other:?}"),
    }

    // The actuator switched THE correlated panel, and ownership reconciled to B under that id.
    assert_eq!(
        switched.lock().unwrap().as_slice(),
        std::slice::from_ref(&monitor_id)
    );
    assert_eq!(
        b_state.lock().unwrap().ownership.owner(&monitor_id),
        Some(b_id.as_str())
    );
}
