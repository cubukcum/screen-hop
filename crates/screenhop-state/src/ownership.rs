use std::collections::HashMap;

/// Distinct ownership states (§8.6). `owner == None` alone is ambiguous; this disambiguates the
/// three cases the UX and reconciliation care about:
/// - `Owned`: a peer is the believed live source.
/// - `Unknown`: never observed / unowned — a transient "we don't know yet" state.
/// - `Stranded`: the owning PC is unreachable and no online peer can read the panel, so there is
///   no software recovery (the operator must press the monitor's physical input button). This is a
///   *persistent* state, not transient unknown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnershipState {
    Owned,
    Unknown,
    Stranded,
}

/// Who currently drives a monitor, with the timestamp the fact was established. This is a
/// **cache**; the panel's live `0x60` is ground truth (§8.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipRecord {
    pub owner: Option<String>,
    pub updated_ms: u64,
    pub state: OwnershipState,
}

/// Last-writer-wins ownership map. Gossip and local observations both flow through [`merge`];
/// the newest `updated_ms` wins, so a fresh live-`0x60` observation overrides stale gossip.
///
/// IMPORTANT: `updated_ms` must be a **cross-peer-comparable wall-clock** timestamp (UTC ms,
/// §8.6 `updatedUtc`). LWW is only correct if timestamps from different peers share an epoch — a
/// per-node monotonic clock (`Instant::elapsed`) is NOT comparable across peers and must never be
/// fed here. The mesh layer is responsible for supplying a wall-clock value.
///
/// [`merge`]: OwnershipMap::merge
#[derive(Debug, Default, Clone)]
pub struct OwnershipMap {
    records: HashMap<String, OwnershipRecord>,
}

impl OwnershipMap {
    pub fn new() -> Self {
        Self::default()
    }

    fn apply(&mut self, monitor: &str, owner: Option<String>, updated_ms: u64, state: OwnershipState) -> bool {
        if let Some(existing) = self.records.get(monitor) {
            if existing.updated_ms >= updated_ms {
                return false;
            }
        }
        self.records.insert(
            monitor.to_owned(),
            OwnershipRecord {
                owner,
                updated_ms,
                state,
            },
        );
        true
    }

    /// Apply a gossiped record. Returns true if it changed state (strictly newer than what we held
    /// — ties keep the existing record so the map converges deterministically across peers).
    pub fn merge(&mut self, monitor: &str, owner: Option<String>, updated_ms: u64) -> bool {
        let state = if owner.is_some() {
            OwnershipState::Owned
        } else {
            OwnershipState::Unknown
        };
        self.apply(monitor, owner, updated_ms, state)
    }

    /// Reconcile from a live `0x60` observation taken at `now_ms` (wall-clock ms). Because a fresh
    /// observation carries the latest timestamp, the observed hardware truth wins over older gossip
    /// (§8.6 "live `0x60` deterministically wins").
    pub fn observe(&mut self, monitor: &str, owner: Option<String>, now_ms: u64) -> bool {
        self.merge(monitor, owner, now_ms)
    }

    /// Mark a monitor **stranded**: its owner is unreachable and no online peer can read it, so
    /// there is no software recovery. Persistent until a fresh observation supersedes it (§8.6).
    pub fn mark_stranded(&mut self, monitor: &str, now_ms: u64) -> bool {
        self.apply(monitor, None, now_ms, OwnershipState::Stranded)
    }

    /// Current believed owner of a monitor, if known and non-empty.
    pub fn owner(&self, monitor: &str) -> Option<&str> {
        self.records
            .get(monitor)
            .and_then(|r| r.owner.as_deref())
    }

    /// The distinct ownership state of a monitor (`Unknown` if never seen).
    pub fn state(&self, monitor: &str) -> OwnershipState {
        self.records
            .get(monitor)
            .map(|r| r.state)
            .unwrap_or(OwnershipState::Unknown)
    }

    pub fn record(&self, monitor: &str) -> Option<&OwnershipRecord> {
        self.records.get(monitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_sets_a_new_record() {
        let mut m = OwnershipMap::new();
        assert!(m.merge("mon", Some("A".into()), 100));
        assert_eq!(m.owner("mon"), Some("A"));
    }

    #[test]
    fn newer_wins_older_is_ignored() {
        let mut m = OwnershipMap::new();
        m.merge("mon", Some("A".into()), 100);
        assert!(m.merge("mon", Some("B".into()), 200)); // newer
        assert_eq!(m.owner("mon"), Some("B"));
        assert!(!m.merge("mon", Some("A".into()), 150)); // stale, ignored
        assert_eq!(m.owner("mon"), Some("B"));
    }

    #[test]
    fn ties_keep_existing_for_deterministic_convergence() {
        let mut m = OwnershipMap::new();
        m.merge("mon", Some("A".into()), 100);
        assert!(!m.merge("mon", Some("B".into()), 100));
        assert_eq!(m.owner("mon"), Some("A"));
    }

    #[test]
    fn observation_overrides_stale_gossip() {
        let mut m = OwnershipMap::new();
        m.merge("mon", Some("A".into()), 100);
        // Someone pressed the OSD button; we read the live 0x60 and see B now owns it.
        assert!(m.observe("mon", Some("B".into()), 500));
        assert_eq!(m.owner("mon"), Some("B"));
    }

    #[test]
    fn owner_none_when_unknown_or_unowned() {
        let mut m = OwnershipMap::new();
        assert_eq!(m.owner("mon"), None);
        assert_eq!(m.state("mon"), OwnershipState::Unknown);
        m.merge("mon", None, 100); // known to be unowned
        assert_eq!(m.owner("mon"), None);
        assert_eq!(m.state("mon"), OwnershipState::Unknown);
        assert!(m.record("mon").is_some());
    }

    #[test]
    fn stranded_is_a_distinct_persistent_state() {
        let mut m = OwnershipMap::new();
        m.merge("mon", Some("A".into()), 100);
        assert_eq!(m.state("mon"), OwnershipState::Owned);
        // Owner went unreachable; nobody can read the panel.
        assert!(m.mark_stranded("mon", 200));
        assert_eq!(m.state("mon"), OwnershipState::Stranded);
        assert_eq!(m.owner("mon"), None);
        // Stranded persists against older gossip, but a fresh live read supersedes it.
        assert!(!m.merge("mon", Some("A".into()), 150)); // stale, ignored
        assert_eq!(m.state("mon"), OwnershipState::Stranded);
        assert!(m.observe("mon", Some("B".into()), 300));
        assert_eq!(m.state("mon"), OwnershipState::Owned);
        assert_eq!(m.owner("mon"), Some("B"));
    }
}
