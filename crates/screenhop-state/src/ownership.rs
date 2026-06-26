use std::collections::HashMap;

/// Who currently drives a monitor, with the timestamp the fact was established. This is a
/// **cache**; the panel's live `0x60` is ground truth (§8.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnershipRecord {
    pub owner: Option<String>,
    pub updated_ms: u64,
}

/// Last-writer-wins ownership map. Gossip and local observations both flow through [`merge`];
/// the newest `updated_ms` wins, so a fresh live-`0x60` observation overrides stale gossip.
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

    /// Apply a record. Returns true if it changed state (strictly newer than what we held — ties
    /// keep the existing record so the map converges deterministically across peers).
    pub fn merge(&mut self, monitor: &str, owner: Option<String>, updated_ms: u64) -> bool {
        if let Some(existing) = self.records.get(monitor) {
            if existing.updated_ms >= updated_ms {
                return false;
            }
        }
        self.records
            .insert(monitor.to_owned(), OwnershipRecord { owner, updated_ms });
        true
    }

    /// Reconcile from a live `0x60` observation taken at `now_ms`. Because `now_ms` is the latest
    /// timestamp, the observed hardware truth wins over any older gossip (§8.6).
    pub fn observe(&mut self, monitor: &str, owner: Option<String>, now_ms: u64) -> bool {
        self.merge(monitor, owner, now_ms)
    }

    /// Current believed owner of a monitor, if known and non-empty.
    pub fn owner(&self, monitor: &str) -> Option<&str> {
        self.records
            .get(monitor)
            .and_then(|r| r.owner.as_deref())
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
        m.merge("mon", None, 100); // known to be unowned/stranded
        assert_eq!(m.owner("mon"), None);
        assert!(m.record("mon").is_some());
    }
}
