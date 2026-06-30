//! Reconciliation (M4.4, §8.6): fold live `0x60` reads back into the ownership cache so an external
//! OSD-button change (or a panel whose DDC got disabled) is detected and corrected.
//!
//! The OS *trigger* — a periodic sweep AND a `WM_DISPLAYCHANGE` hook on Windows — lives in the
//! platform/UI layer (wired in M5, hardware-verified). This module is the pure logic that trigger
//! calls, so the convergence rules are unit-tested here independently of any OS event source.
//!
//! Callers MUST pass a current wall-clock `now_ms` (the same epoch as ownership LWW): a fresh read
//! carries the latest timestamp, which is what lets the observed hardware truth win over stale
//! gossip (`OwnershipMap::observe`).

use std::collections::HashSet;

use screenhop_identity::CalibrationStore;
use screenhop_state::OwnershipMap;

/// What a live read of one panel found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveRead {
    /// The panel's live `0x60` maps to this peer (`Some`) or is unowned/unmapped (`None`).
    Owner(Option<String>),
    /// DDC/CI is disabled in the OSD — the panel can be neither read nor switched over the wire.
    DdcDisabled,
    /// The panel can't be read and no online peer can drive it — stranded (physical button only).
    Stranded,
}

/// A monitor whose believed owner changed during reconciliation (e.g. someone pressed the OSD
/// button), so the UI can surface "monitor X moved on its own".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileChange {
    pub monitor_id: String,
    pub previous_owner: Option<String>,
    pub new_owner: Option<String>,
}

/// Apply one live read for `monitor_id` at `now_ms`. Returns `Some(change)` if the believed owner
/// changed as a result.
pub fn reconcile_one(
    map: &mut OwnershipMap,
    monitor_id: &str,
    read: &LiveRead,
    now_ms: u64,
) -> Option<ReconcileChange> {
    let previous_owner = map.owner(monitor_id).map(str::to_owned);
    match read {
        LiveRead::Owner(owner) => {
            map.observe(monitor_id, owner.clone(), now_ms);
        }
        LiveRead::DdcDisabled => {
            map.mark_ddc_disabled(monitor_id, now_ms);
        }
        LiveRead::Stranded => {
            map.mark_stranded(monitor_id, now_ms);
        }
    }
    let new_owner = map.owner(monitor_id).map(str::to_owned);
    (previous_owner != new_owner).then(|| ReconcileChange {
        monitor_id: monitor_id.to_owned(),
        previous_owner,
        new_owner,
    })
}

/// Reconcile a batch of live reads — one periodic sweep, or the reads triggered by a single
/// `WM_DISPLAYCHANGE`. Returns the monitors whose owner changed externally.
pub fn reconcile_all(
    map: &mut OwnershipMap,
    reads: &[(String, LiveRead)],
    now_ms: u64,
) -> Vec<ReconcileChange> {
    reads
        .iter()
        .filter_map(|(id, read)| reconcile_one(map, id, read, now_ms))
        .collect()
}

/// Map one live `0x60` read of `monitor_id` into a [`LiveRead`], or `None` when the read is
/// inconclusive and the cached state should be left untouched.
///
/// - A **successful** read maps the value back to its owner via [`CalibrationStore::owner_for`]; an
///   unrecognized value is `Owner(None)` (the panel is driven, but not by a peer we can identify).
/// - A **failed** read concludes [`LiveRead::Stranded`] only when the *believed owner is offline*.
///   A failed read on an unowned / not-yet-calibrated panel is transient/unknown — returning `None`
///   so a fresh mesh never false-strands a working monitor.
///
/// Note: this is read-only — the caller does the (brief) locked `reconcile_*` afterwards, so the
/// slow DDC read never happens while the `MeshState` lock is held.
pub fn read_to_live_read(
    ownership: &OwnershipMap,
    calibration: &CalibrationStore,
    online_peers: &HashSet<String>,
    monitor_id: &str,
    read: Option<u32>,
) -> Option<LiveRead> {
    match read {
        Some(value) => Some(LiveRead::Owner(calibration.owner_for(monitor_id, value))),
        None => match ownership.owner(monitor_id) {
            Some(owner) if !online_peers.contains(owner) => Some(LiveRead::Stranded),
            _ => None,
        },
    }
}

/// Reconcile a batch of raw live reads `(monitor_id, Option<value>)` taken at `now_ms`: map each
/// through [`read_to_live_read`], skip the inconclusive ones, and apply the rest. Returns the
/// externally-changed monitors. The caller collects the reads WITHOUT holding any lock, then calls
/// this under a brief `MeshState` lock.
pub fn reconcile_reads(
    map: &mut OwnershipMap,
    calibration: &CalibrationStore,
    online_peers: &HashSet<String>,
    reads: &[(String, Option<u32>)],
    now_ms: u64,
) -> Vec<ReconcileChange> {
    let mut changes = Vec::new();
    for (id, read) in reads {
        if let Some(live) = read_to_live_read(map, calibration, online_peers, id, *read) {
            if let Some(change) = reconcile_one(map, id, &live, now_ms) {
                changes.push(change);
            }
        }
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenhop_state::OwnershipState;

    #[test]
    fn detects_an_external_owner_change() {
        let mut map = OwnershipMap::new();
        map.merge("m1", Some("A".into()), 100);
        // Someone pressed the OSD button; the live read now shows B drives it.
        let change = reconcile_one(&mut map, "m1", &LiveRead::Owner(Some("B".into())), 500);
        assert_eq!(
            change,
            Some(ReconcileChange {
                monitor_id: "m1".into(),
                previous_owner: Some("A".into()),
                new_owner: Some("B".into()),
            })
        );
        assert_eq!(map.owner("m1"), Some("B"));
    }

    #[test]
    fn no_change_when_live_matches_cache() {
        let mut map = OwnershipMap::new();
        map.merge("m1", Some("A".into()), 100);
        assert_eq!(
            reconcile_one(&mut map, "m1", &LiveRead::Owner(Some("A".into())), 500),
            None
        );
    }

    #[test]
    fn ddc_disabled_read_sets_state_and_reports_owner_loss() {
        let mut map = OwnershipMap::new();
        map.merge("m1", Some("A".into()), 100);
        let change = reconcile_one(&mut map, "m1", &LiveRead::DdcDisabled, 500).unwrap();
        assert_eq!(change.previous_owner, Some("A".into()));
        assert_eq!(change.new_owner, None);
        assert_eq!(map.state("m1"), OwnershipState::DdcDisabled);
    }

    fn online(p: &[&str]) -> HashSet<String> {
        p.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn read_maps_value_to_owner_via_calibration() {
        let map = OwnershipMap::new();
        let mut cal = CalibrationStore::new();
        cal.record("A", "m1", 0x0F);
        assert_eq!(
            read_to_live_read(&map, &cal, &online(&["A"]), "m1", Some(0x0F)),
            Some(LiveRead::Owner(Some("A".to_string())))
        );
        // A read of a value no peer has calibrated -> driven but unmapped.
        assert_eq!(
            read_to_live_read(&map, &cal, &online(&["A"]), "m1", Some(0x77)),
            Some(LiveRead::Owner(None))
        );
    }

    #[test]
    fn failed_read_strands_only_when_the_owner_is_offline() {
        let mut map = OwnershipMap::new();
        map.merge("m1", Some("A".into()), 100);
        let cal = CalibrationStore::new();
        // Owner A is offline + the read failed -> stranded.
        assert_eq!(
            read_to_live_read(&map, &cal, &online(&["B"]), "m1", None),
            Some(LiveRead::Stranded)
        );
        // Owner A is online -> inconclusive, leave the cache alone.
        assert_eq!(
            read_to_live_read(&map, &cal, &online(&["A"]), "m1", None),
            None
        );
        // Unowned / uncalibrated panel + failed read -> NOT stranded (fresh-mesh safety).
        assert_eq!(
            read_to_live_read(&map, &cal, &online(&["A"]), "unowned", None),
            None
        );
    }

    #[test]
    fn reconcile_reads_applies_only_conclusive_changes() {
        let mut map = OwnershipMap::new();
        map.merge("m1", Some("A".into()), 100);
        map.merge("m2", Some("A".into()), 100);
        let mut cal = CalibrationStore::new();
        cal.record("B", "m1", 0x0F); // B's calibrated value on m1
        let reads = vec![
            ("m1".to_string(), Some(0x0F)), // now shows B's value -> A→B change
            ("m2".to_string(), None),       // failed read, owner A online -> skipped
        ];
        let changes = reconcile_reads(&mut map, &cal, &online(&["A", "B"]), &reads, 500);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].monitor_id, "m1");
        assert_eq!(map.owner("m1"), Some("B"));
        assert_eq!(map.owner("m2"), Some("A")); // unchanged
    }

    #[test]
    fn reconcile_all_returns_only_the_changed_monitors() {
        let mut map = OwnershipMap::new();
        map.merge("m1", Some("A".into()), 100);
        map.merge("m2", Some("B".into()), 100);
        let reads = vec![
            ("m1".to_string(), LiveRead::Owner(Some("A".into()))), // unchanged
            ("m2".to_string(), LiveRead::Owner(Some("A".into()))), // B -> A, changed
        ];
        let changes = reconcile_all(&mut map, &reads, 500);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].monitor_id, "m2");
        assert_eq!(map.owner("m2"), Some("A"));
    }
}
