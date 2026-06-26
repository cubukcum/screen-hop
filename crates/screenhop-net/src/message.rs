use serde::{Deserialize, Serialize};

pub type PeerId = String;
pub type MonitorId = String;

/// Outer wrapper carried in every mesh packet: the sender, its per-sender sequence number
/// (for anti-replay), and the typed body. Serialized to bytes, then sealed by a `SecureChannel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Envelope {
    pub from: PeerId,
    pub seq: u64,
    pub body: Message,
}

impl Envelope {
    pub fn to_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("envelope serializes to JSON")
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    /// Associated data binding a sealed frame to its `sender:seq`, so a captured frame cannot be
    /// re-attributed to a different identity even by a holder of the group key.
    pub fn aad(&self) -> Vec<u8> {
        format!("{}:{}", self.from, self.seq).into_bytes()
    }
}

/// The mesh wire protocol (docs/PLAN-screen-hop.md §8.4). Externally tagged by `type`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// Presence + addresses + whether this peer can currently actuate DDC.
    Announce {
        name: String,
        endpoints: Vec<String>,
        can_actuate: bool,
        state_version: u64,
    },
    /// Periodic liveness.
    Heartbeat { state_version: u64 },
    /// Cache update for who currently drives a panel (ground truth remains the live 0x60).
    OwnershipGossip {
        monitor_id: MonitorId,
        owner: Option<PeerId>,
        updated_ms: u64,
    },
    /// Request the per-monitor lease lock before actuating.
    LockRequest {
        monitor_id: MonitorId,
        lease_secs: u32,
    },
    LockGrant {
        monitor_id: MonitorId,
        holder: PeerId,
        lease_expires_ms: u64,
    },
    LockDeny {
        monitor_id: MonitorId,
        reason: String,
    },
    /// Tell the actuating peer to perform the 0x60 write.
    SwitchCommand {
        monitor_id: MonitorId,
        target: PeerId,
        input_value: u32,
    },
    SwitchResult {
        monitor_id: MonitorId,
        outcome: String,
        observed: Option<u32>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip(env: &Envelope) {
        let bytes = env.to_bytes();
        assert_eq!(Envelope::from_bytes(&bytes).as_ref(), Some(env));
    }

    #[test]
    fn envelope_round_trips_for_variants() {
        round_trip(&Envelope {
            from: "peerA".into(),
            seq: 1,
            body: Message::Heartbeat { state_version: 5 },
        });
        round_trip(&Envelope {
            from: "peerA".into(),
            seq: 2,
            body: Message::Announce {
                name: "Work PC".into(),
                endpoints: vec!["192.168.1.5:7777".into()],
                can_actuate: true,
                state_version: 5,
            },
        });
        round_trip(&Envelope {
            from: "peerB".into(),
            seq: 3,
            body: Message::SwitchCommand {
                monitor_id: "abc123".into(),
                target: "peerB".into(),
                input_value: 0x0F,
            },
        });
        round_trip(&Envelope {
            from: "peerB".into(),
            seq: 4,
            body: Message::LockGrant {
                monitor_id: "abc123".into(),
                holder: "peerB".into(),
                lease_expires_ms: 1234,
            },
        });
    }

    #[test]
    fn message_is_tagged_by_type() {
        let env = Envelope {
            from: "p".into(),
            seq: 1,
            body: Message::Heartbeat { state_version: 1 },
        };
        let json = String::from_utf8(env.to_bytes()).unwrap();
        assert!(json.contains("\"type\":\"heartbeat\""), "{json}");
    }

    #[test]
    fn from_bytes_rejects_garbage() {
        assert!(Envelope::from_bytes(b"not json").is_none());
    }

    #[test]
    fn aad_binds_sender_and_seq() {
        let env = Envelope {
            from: "peerA".into(),
            seq: 7,
            body: Message::Heartbeat { state_version: 1 },
        };
        assert_eq!(env.aad(), b"peerA:7");
    }

    #[test]
    fn envelope_seals_and_opens_through_secure_channel() {
        use crate::crypto::SecureChannel;
        let ch = SecureChannel::from_passphrase("mesh");
        let env = Envelope {
            from: "peerA".into(),
            seq: 9,
            body: Message::Heartbeat { state_version: 2 },
        };
        let frame = ch.seal(&env.to_bytes(), &env.aad());
        let opened = ch.open(&frame, &env.aad()).expect("opens with matching aad");
        assert_eq!(Envelope::from_bytes(&opened).as_ref(), Some(&env));
    }
}
