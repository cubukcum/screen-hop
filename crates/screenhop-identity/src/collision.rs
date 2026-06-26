use std::collections::HashMap;

use crate::fingerprint::MonitorFingerprint;

/// Group fingerprint indices by their stable `monitor_id`.
///
/// An id appearing more than once is either the **same** physical panel seen via multiple
/// backends (safe to de-dup) OR **distinct** identical-model panels that collide — told apart by
/// [`MonitorFingerprint::is_ambiguous`] (a real per-unit serial means de-dup; no serial means
/// genuine collision needing user labeling).
pub fn group_by_id(fps: &[MonitorFingerprint]) -> HashMap<String, Vec<usize>> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, fp) in fps.iter().enumerate() {
        map.entry(fp.monitor_id()).or_default().push(i);
    }
    map
}

/// Monitor ids that are **ambiguous** (no per-unit serial) AND appear more than once — genuine
/// identical-model collisions the user must label. Non-ambiguous duplicates are de-dups, not
/// collisions, and are intentionally excluded.
pub fn collisions_needing_labels(fps: &[MonitorFingerprint]) -> Vec<String> {
    let mut ids: Vec<String> = group_by_id(fps)
        .into_iter()
        .filter(|(_, idxs)| idxs.len() > 1 && fps[idxs[0]].is_ambiguous())
        .map(|(id, _)| id)
        .collect();
    ids.sort();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(serial: u32, ascii: Option<&str>) -> MonitorFingerprint {
        MonitorFingerprint::from_parts("AOC", 0x1234, serial, ascii.map(str::to_owned))
    }

    #[test]
    fn duplicates_with_real_serial_group_as_one_dedup() {
        // Same panel via two backends -> same id, NOT a labeling collision.
        let fps = vec![fp(1598, None), fp(1598, None)];
        let groups = group_by_id(&fps);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups.values().next().unwrap().len(), 2);
        assert!(collisions_needing_labels(&fps).is_empty());
    }

    #[test]
    fn identical_model_without_serial_needs_labels() {
        // Two different panels, both serial-less -> same id, ambiguous -> collision.
        let fps = vec![fp(0, None), fp(0, None)];
        let collisions = collisions_needing_labels(&fps);
        assert_eq!(collisions.len(), 1);
        assert_eq!(collisions[0], fps[0].monitor_id());
    }

    #[test]
    fn distinct_serials_do_not_collide() {
        let fps = vec![fp(1, None), fp(2, None), fp(3, None)];
        assert_eq!(group_by_id(&fps).len(), 3);
        assert!(collisions_needing_labels(&fps).is_empty());
    }
}
