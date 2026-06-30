use std::collections::{HashMap, HashSet};

use crate::fingerprint::MonitorFingerprint;

/// One enumerated panel: its [`MonitorFingerprint`] plus an out-of-band physical discriminator.
///
/// The fingerprint alone cannot tell "the **same** panel reached via two backends" (safe to
/// de-dup) from "two **distinct** identical-model panels that collide on id" (must be labeled) —
/// especially when the per-unit serial is blank or *model-constant* (the same nonzero serial across
/// units). The `device_path` (Windows `monitorDevicePath`, the GDI device name, or a stable
/// position) is the discriminator: distinct physical connections ⇒ distinct panels.
#[derive(Debug, Clone)]
pub struct EnumeratedPanel {
    pub fingerprint: MonitorFingerprint,
    /// Stable per-machine physical path/slot, when the platform exposes one.
    pub device_path: Option<String>,
}

impl EnumeratedPanel {
    pub fn new(fingerprint: MonitorFingerprint, device_path: Option<String>) -> Self {
        Self {
            fingerprint,
            device_path,
        }
    }
}

/// Group fingerprint indices by their stable `monitor_id`.
///
/// An id appearing more than once is either the **same** physical panel seen via multiple
/// backends (safe to de-dup) OR **distinct** identical-model panels that collide — told apart by
/// the device path (see [`collisions_needing_labels`]).
pub fn group_by_id(fps: &[MonitorFingerprint]) -> HashMap<String, Vec<usize>> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, fp) in fps.iter().enumerate() {
        map.entry(fp.monitor_id()).or_default().push(i);
    }
    map
}

/// Monitor ids that must be **labeled** by the user because screen-hop cannot prove their colliding
/// members are one physical panel (§7.3: identical fingerprints are never silently re-bound).
///
/// For each `monitor_id` shared by more than one enumerated panel:
/// - if the members occupy **more than one distinct device path** → distinct physical panels →
///   labeling required (this is the model-constant-serial false-merge the naive serial check missed);
/// - if every member shares **one** device path → the same panel via multiple backends → de-dup;
/// - if device-path info is **absent** → fall back to the serial heuristic: a real per-unit serial
///   is assumed unique (de-dup), but a serial-less (ambiguous) fingerprint must be labeled.
pub fn collisions_needing_labels(panels: &[EnumeratedPanel]) -> Vec<String> {
    let mut by_id: HashMap<String, Vec<&EnumeratedPanel>> = HashMap::new();
    for p in panels {
        by_id.entry(p.fingerprint.monitor_id()).or_default().push(p);
    }

    let mut ids: Vec<String> = by_id
        .into_iter()
        .filter(|(_, members)| members.len() > 1)
        .filter(|(_, members)| {
            let distinct_paths: HashSet<&str> = members
                .iter()
                .filter_map(|m| m.device_path.as_deref())
                .collect();
            if !distinct_paths.is_empty() {
                // We have physical-path evidence: >1 distinct path ⇒ genuinely distinct panels.
                distinct_paths.len() > 1
            } else {
                // No path evidence at all: can only fall back to the serial. A serial-less
                // fingerprint is unprovable-same ⇒ label; a real serial is assumed unique ⇒ de-dup.
                members[0].fingerprint.is_ambiguous()
            }
        })
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

    fn panel(serial: u32, ascii: Option<&str>, path: Option<&str>) -> EnumeratedPanel {
        EnumeratedPanel::new(fp(serial, ascii), path.map(str::to_owned))
    }

    #[test]
    fn duplicates_with_real_serial_group_as_one_dedup() {
        // Same panel via two backends -> same id, NOT a labeling collision.
        let fps = vec![fp(1598, None), fp(1598, None)];
        let groups = group_by_id(&fps);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups.values().next().unwrap().len(), 2);
        // Without device paths, a real per-unit serial is assumed unique -> de-dup.
        let panels = vec![panel(1598, None, None), panel(1598, None, None)];
        assert!(collisions_needing_labels(&panels).is_empty());
    }

    #[test]
    fn identical_model_without_serial_needs_labels() {
        // Two different panels, both serial-less -> same id, ambiguous -> collision.
        let panels = vec![panel(0, None, None), panel(0, None, None)];
        let collisions = collisions_needing_labels(&panels);
        assert_eq!(collisions.len(), 1);
        assert_eq!(collisions[0], fp(0, None).monitor_id());
    }

    #[test]
    fn same_serial_but_distinct_device_paths_needs_labels() {
        // The model-constant-serial hazard: two PHYSICAL panels share a non-unique serial, so they
        // collide on id, but their device paths differ -> must be labeled, not silently merged.
        let panels = vec![
            panel(42, None, Some(r"\\.\DISPLAY1")),
            panel(42, None, Some(r"\\.\DISPLAY2")),
        ];
        let collisions = collisions_needing_labels(&panels);
        assert_eq!(collisions.len(), 1);
        assert_eq!(collisions[0], fp(42, None).monitor_id());
    }

    #[test]
    fn same_serial_and_same_device_path_is_a_dedup() {
        // The same physical panel surfaced twice (e.g. two backends) shares its device path.
        let panels = vec![
            panel(42, None, Some(r"\\.\DISPLAY1")),
            panel(42, None, Some(r"\\.\DISPLAY1")),
        ];
        assert!(collisions_needing_labels(&panels).is_empty());
    }

    #[test]
    fn distinct_serials_do_not_collide() {
        let panels = vec![
            panel(1, None, None),
            panel(2, None, None),
            panel(3, None, None),
        ];
        assert_eq!(
            group_by_id(&[fp(1, None), fp(2, None), fp(3, None)]).len(),
            3
        );
        assert!(collisions_needing_labels(&panels).is_empty());
    }
}
