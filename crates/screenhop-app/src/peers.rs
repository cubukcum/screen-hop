//! Mesh peer presence / liveness registry (M3), fed by `Announce` and `Heartbeat` messages. It is
//! also the basis for the M4 peer-loss → degraded detector: a peer not seen within a TTL is treated
//! as offline, which feeds the partition guard (§8.6).

use std::collections::HashMap;

pub type PeerId = String;

/// What we know about one peer from its last Announce/Heartbeat.
#[derive(Debug, Clone)]
pub struct PeerPresence {
    pub name: String,
    pub endpoints: Vec<String>,
    pub can_actuate: bool,
    pub state_version: u64,
    /// Node-monotonic ms (this node's clock domain) when we last heard from the peer.
    pub last_seen_ms: u64,
}

#[derive(Debug, Default, Clone)]
pub struct PeerRegistry {
    peers: HashMap<PeerId, PeerPresence>,
}

impl PeerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a full Announce (presence + addresses + capability). Announce carries the peer's
    /// whole presence, so it replaces any prior entry.
    pub fn observe_announce(
        &mut self,
        peer_id: &str,
        name: String,
        endpoints: Vec<String>,
        can_actuate: bool,
        state_version: u64,
        now_ms: u64,
    ) {
        self.peers.insert(
            peer_id.to_owned(),
            PeerPresence {
                name,
                endpoints,
                can_actuate,
                state_version,
                last_seen_ms: now_ms,
            },
        );
    }

    /// Record a Heartbeat (liveness only). If we never saw an Announce first, record minimal
    /// presence so the peer still counts as online.
    pub fn observe_heartbeat(&mut self, peer_id: &str, state_version: u64, now_ms: u64) {
        match self.peers.get_mut(peer_id) {
            Some(entry) => {
                entry.state_version = state_version;
                entry.last_seen_ms = now_ms;
            }
            None => {
                self.peers.insert(
                    peer_id.to_owned(),
                    PeerPresence {
                        name: String::new(),
                        endpoints: Vec::new(),
                        can_actuate: false,
                        state_version,
                        last_seen_ms: now_ms,
                    },
                );
            }
        }
    }

    pub fn get(&self, peer_id: &str) -> Option<&PeerPresence> {
        self.peers.get(peer_id)
    }

    /// All known peer ids (any liveness). For the UI to render the peer list.
    pub fn ids(&self) -> Vec<PeerId> {
        self.peers.keys().cloned().collect()
    }

    pub fn last_seen_ms(&self, peer_id: &str) -> Option<u64> {
        self.peers.get(peer_id).map(|p| p.last_seen_ms)
    }

    /// True if `peer_id` was seen within `ttl_ms` of `now_ms`.
    pub fn is_online(&self, peer_id: &str, now_ms: u64, ttl_ms: u64) -> bool {
        self.peers
            .get(peer_id)
            .is_some_and(|p| now_ms.saturating_sub(p.last_seen_ms) <= ttl_ms)
    }

    /// Peer ids seen within `ttl_ms` of `now_ms` (the set of currently-online peers).
    pub fn online(&self, now_ms: u64, ttl_ms: u64) -> Vec<PeerId> {
        self.peers
            .iter()
            .filter(|(_, p)| now_ms.saturating_sub(p.last_seen_ms) <= ttl_ms)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Known peers NOT seen within `ttl_ms` of `now_ms` — believed lost (partition / peer-loss).
    pub fn lost(&self, now_ms: u64, ttl_ms: u64) -> Vec<PeerId> {
        self.peers
            .iter()
            .filter(|(_, p)| now_ms.saturating_sub(p.last_seen_ms) > ttl_ms)
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// True if any *known* peer is currently lost. This is the mesh's degraded/partition signal
    /// (§8.6, M4.4e): the orchestrator pauses disruptive ops while it holds, so a peer acting on a
    /// stale ownership cache can't race a double-write — without a caller setting `degraded` by hand.
    /// An empty registry (we know of no peers) is *not* degraded.
    pub fn is_degraded(&self, now_ms: u64, ttl_ms: u64) -> bool {
        self.peers
            .values()
            .any(|p| now_ms.saturating_sub(p.last_seen_ms) > ttl_ms)
    }

    pub fn len(&self) -> usize {
        self.peers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.peers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: u64 = 10_000;

    #[test]
    fn announce_records_presence_and_online_within_ttl() {
        let mut r = PeerRegistry::new();
        r.observe_announce(
            "A",
            "Work PC".into(),
            vec!["10.0.0.5:7777".into()],
            true,
            3,
            1_000,
        );
        let p = r.get("A").unwrap();
        assert_eq!(p.name, "Work PC");
        assert!(p.can_actuate);
        assert_eq!(p.state_version, 3);
        assert!(r.is_online("A", 5_000, TTL)); // 4s since last seen
        assert!(!r.is_online("A", 20_000, TTL)); // 19s since last seen -> stale
    }

    #[test]
    fn heartbeat_updates_last_seen_and_version() {
        let mut r = PeerRegistry::new();
        r.observe_announce("A", "PC".into(), vec![], true, 1, 1_000);
        r.observe_heartbeat("A", 9, 8_000);
        let p = r.get("A").unwrap();
        assert_eq!(p.state_version, 9);
        assert_eq!(p.last_seen_ms, 8_000);
        assert!(
            p.can_actuate,
            "heartbeat must not clobber capability from the announce"
        );
    }

    #[test]
    fn heartbeat_before_announce_records_minimal_presence() {
        let mut r = PeerRegistry::new();
        r.observe_heartbeat("B", 2, 500);
        assert!(r.is_online("B", 1_000, TTL));
        assert_eq!(r.get("B").unwrap().name, "");
    }

    #[test]
    fn degraded_when_a_known_peer_goes_silent() {
        let mut r = PeerRegistry::new();
        assert!(!r.is_degraded(0, TTL), "no known peers is not degraded");
        r.observe_announce("A", "a".into(), vec![], true, 1, 0);
        r.observe_announce("B", "b".into(), vec![], true, 1, 0);
        // Both fresh -> healthy.
        assert!(!r.is_degraded(5_000, TTL));
        // A keeps heartbeating but B goes silent past the TTL -> degraded (B is lost).
        r.observe_heartbeat("A", 2, 12_000);
        assert!(r.is_degraded(15_000, TTL));
        assert_eq!(r.lost(15_000, TTL), vec!["B".to_string()]);
        // B comes back -> healthy again.
        r.observe_heartbeat("B", 2, 15_000);
        r.observe_heartbeat("A", 3, 15_000);
        assert!(!r.is_degraded(16_000, TTL));
    }

    #[test]
    fn lost_lists_only_silent_peers() {
        let mut r = PeerRegistry::new();
        r.observe_announce("A", "a".into(), vec![], true, 1, 0);
        r.observe_announce("B", "b".into(), vec![], true, 1, 12_000);
        let lost = r.lost(15_000, TTL); // A: 15s silent -> lost; B: 3s -> fine
        assert_eq!(lost, vec!["A".to_string()]);
    }

    #[test]
    fn online_lists_only_recent_peers() {
        let mut r = PeerRegistry::new();
        r.observe_announce("A", "a".into(), vec![], true, 1, 0);
        r.observe_announce("B", "b".into(), vec![], true, 1, 9_000);
        let mut online = r.online(10_000, TTL); // A: 10s stale (== ttl, online), B: 1s
        online.sort();
        assert_eq!(online, vec!["A".to_string(), "B".to_string()]);
        let online_later = r.online(15_000, TTL); // A: 15s stale -> dropped, B: 6s
        assert_eq!(online_later, vec!["B".to_string()]);
    }
}
