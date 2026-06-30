use std::collections::{HashMap, HashSet};

/// Per-`(peer, monitor)` confirmed `0x60` input value.
///
/// Per decision **D4**, a value is the selector for *one peer's own cable* on a panel and is
/// **never** shared with or used by another peer (writing another peer's value could pick the
/// wrong input or risk a soft-brick). A peer that has never been the active source on a panel
/// has no value here — the "value unknown until first active" state.
///
/// Stored as a nested `peer -> monitor -> value` map so lookups borrow `&str` keys directly,
/// without allocating an owned `(String, String)` tuple on every read.
#[derive(Debug, Default, Clone)]
pub struct CalibrationStore {
    values: HashMap<String, HashMap<String, u32>>,
}

impl CalibrationStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record this peer's own confirmed input value for a monitor (learned while it was active).
    pub fn record(&mut self, peer_id: &str, monitor_id: &str, input_value: u32) {
        self.values
            .entry(peer_id.to_owned())
            .or_default()
            .insert(monitor_id.to_owned(), input_value);
    }

    /// This peer's confirmed value for a monitor, or `None` if unknown-until-first-active.
    pub fn confirmed_value(&self, peer_id: &str, monitor_id: &str) -> Option<u32> {
        self.values
            .get(peer_id)
            .and_then(|m| m.get(monitor_id))
            .copied()
    }

    pub fn is_calibrated(&self, peer_id: &str, monitor_id: &str) -> bool {
        self.confirmed_value(peer_id, monitor_id).is_some()
    }

    /// The confirmed value(s) as the soft-brick allow-list for the actuation policy
    /// (`ActuationPolicy::confirmed_values`). Empty when uncalibrated.
    pub fn confirmed_set(&self, peer_id: &str, monitor_id: &str) -> HashSet<u32> {
        self.confirmed_value(peer_id, monitor_id)
            .into_iter()
            .collect()
    }

    /// Reverse lookup: which peer (if any) has `value` confirmed for `monitor_id`. Reconciliation
    /// uses this to map a live `0x60` read back to the owner that value belongs to. Iterates in a
    /// stable order (sorted by peer id) so a tie resolves deterministically. This stays within D4 —
    /// it only *reads* the per-peer values, it never lets one peer use another's value.
    pub fn owner_for(&self, monitor_id: &str, value: u32) -> Option<String> {
        let mut owners: Vec<&String> = self
            .values
            .iter()
            .filter(|(_, mons)| mons.get(monitor_id) == Some(&value))
            .map(|(peer, _)| peer)
            .collect();
        owners.sort();
        owners.first().map(|s| (*s).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_and_reads_back_per_peer_monitor() {
        let mut s = CalibrationStore::new();
        s.record("peerA", "mon1", 0x0F);
        assert_eq!(s.confirmed_value("peerA", "mon1"), Some(0x0F));
        assert!(s.is_calibrated("peerA", "mon1"));
    }

    #[test]
    fn value_is_never_visible_to_another_peer_or_monitor() {
        // D4: PC-A's value must not leak to PC-B, nor to a different monitor.
        let mut s = CalibrationStore::new();
        s.record("peerA", "mon1", 0x0F);
        assert_eq!(s.confirmed_value("peerB", "mon1"), None);
        assert_eq!(s.confirmed_value("peerA", "mon2"), None);
        assert!(!s.is_calibrated("peerB", "mon1"));
    }

    #[test]
    fn uncalibrated_is_unknown_until_first_active() {
        let s = CalibrationStore::new();
        assert_eq!(s.confirmed_value("peerA", "mon1"), None);
        assert!(s.confirmed_set("peerA", "mon1").is_empty());
    }

    #[test]
    fn confirmed_set_feeds_the_actuation_allow_list() {
        let mut s = CalibrationStore::new();
        s.record("peerA", "mon1", 0x0F);
        let set = s.confirmed_set("peerA", "mon1");
        assert!(set.contains(&0x0F));
        assert_eq!(set.len(), 1);
    }

    #[test]
    fn owner_for_reverse_maps_value_to_peer() {
        let mut s = CalibrationStore::new();
        s.record("peerA", "mon1", 0x0F);
        s.record("peerB", "mon1", 0x11);
        assert_eq!(s.owner_for("mon1", 0x0F).as_deref(), Some("peerA"));
        assert_eq!(s.owner_for("mon1", 0x11).as_deref(), Some("peerB"));
        assert_eq!(s.owner_for("mon1", 0x99), None); // no peer has this value
        assert_eq!(s.owner_for("mon2", 0x0F), None); // value is for mon1, not mon2
    }
}
