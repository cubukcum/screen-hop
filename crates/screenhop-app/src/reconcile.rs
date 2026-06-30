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
