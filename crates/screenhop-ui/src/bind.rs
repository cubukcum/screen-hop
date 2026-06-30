//! Binding layer (M5): map the backend [`Controller`] view models into the Slint tray structs, and
//! translate the tray's **index-based** callbacks back into backend **ids**.
//!
//! The Slint model is positional (`MonitorRow.active` is an index into `peers`; `switch(mi, pi)`
//! passes row/segment indices), while the backend speaks `monitor_id` / `peer_id`. [`TrayBinding`]
//! keeps the ordered id lists alongside the rendered rows so that translation is exact and tested.

use slint::{Color, SharedString};

use crate::controller::{Controller, MonitorUiState, MonitorView};
use crate::{MonitorRow, Peer, Preset};

/// Per-peer brand colors for the tray segments (cycled by peer index).
const PEER_PALETTE: [(u8, u8, u8); 6] = [
    (0x2f, 0x6f, 0xed), // blue
    (0x8b, 0x5c, 0xf6), // violet
    (0x12, 0x9a, 0x5e), // green
    (0xe0, 0x7b, 0x1a), // amber
    (0xd9, 0x3a, 0x6a), // pink
    (0x16, 0x8a, 0x98), // teal
];

fn peer_color(i: usize) -> Color {
    let (r, g, b) = PEER_PALETTE[i % PEER_PALETTE.len()];
    Color::from_rgb_u8(r, g, b)
}

/// A short status line for a monitor row (the `spec` slot), derived from its ownership state.
fn status_line(mv: &MonitorView) -> String {
    match mv.state {
        MonitorUiState::Mine => "This PC".to_string(),
        MonitorUiState::OwnedByPeer => mv.owner.clone().unwrap_or_else(|| "Another PC".to_string()),
        MonitorUiState::Unowned => "Unassigned".to_string(),
        MonitorUiState::Stranded => "Stranded — use the monitor's input button".to_string(),
        MonitorUiState::DdcDisabled => "DDC/CI disabled in the monitor's menu".to_string(),
    }
}

/// The tray view models plus the ordered id lists needed to translate callbacks back to ids.
pub struct TrayBinding {
    /// `monitor_ids` in the same order as the `monitors` rows.
    pub monitor_ids: Vec<String>,
    /// `peer_ids` in the same order as the `peers` segments.
    pub peer_ids: Vec<String>,
    pub monitors: Vec<MonitorRow>,
    pub peers: Vec<Peer>,
}

impl TrayBinding {
    /// Resolve a tray `switch(monitor_index, peer_index)` callback to `(monitor_id, peer_id)`.
    /// Returns `None` for out-of-range indices (so a stale UI event can't actuate the wrong panel).
    pub fn resolve_switch(&self, monitor_index: i32, peer_index: i32) -> Option<(String, String)> {
        let m = usize::try_from(monitor_index).ok()?;
        let p = usize::try_from(peer_index).ok()?;
        Some((
            self.monitor_ids.get(m)?.clone(),
            self.peer_ids.get(p)?.clone(),
        ))
    }
}

/// Build the tray view models from the controller. `peer_ids` is the ordered set of peers shown as
/// segments (e.g. this PC first, then known peers); `peer_labels` are the short segment captions
/// (falls back to the id when missing); `monitor_ids` the ordered monitors to render.
pub fn build_tray(
    controller: &Controller,
    monitor_ids: &[String],
    peer_ids: &[String],
    peer_labels: &[String],
) -> TrayBinding {
    let views = controller.monitor_views(monitor_ids);
    let peer_index = |id: &str| peer_ids.iter().position(|p| p == id);

    let monitors = views
        .iter()
        .map(|mv| MonitorRow {
            name: SharedString::from(mv.label.as_str()),
            spec: SharedString::from(status_line(mv)),
            active: mv
                .owner
                .as_deref()
                .and_then(peer_index)
                .map_or(-1, |i| i as i32),
            switching: false,
        })
        .collect();

    let peers = peer_ids
        .iter()
        .enumerate()
        .map(|(i, id)| Peer {
            label: SharedString::from(
                peer_labels
                    .get(i)
                    .map(String::as_str)
                    .unwrap_or(id.as_str()),
            ),
            color: peer_color(i),
        })
        .collect();

    TrayBinding {
        monitor_ids: monitor_ids.to_vec(),
        peer_ids: peer_ids.to_vec(),
        monitors,
        peers,
    }
}

/// Convenience: build the Slint `Preset` rows from preset names and which one is active.
pub fn build_presets(names: &[String], active: Option<usize>) -> Vec<Preset> {
    names
        .iter()
        .enumerate()
        .map(|(i, n)| Preset {
            name: SharedString::from(n.as_str()),
            active: Some(i) == active,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenhop_app::MeshState;
    use std::sync::{Arc, Mutex};

    fn controller_with(owners: &[(&str, Option<&str>)]) -> Controller {
        let st = Arc::new(Mutex::new(MeshState::default()));
        {
            let mut s = st.lock().unwrap();
            for (i, (mon, owner)) in owners.iter().enumerate() {
                s.ownership
                    .observe(mon, owner.map(|o| o.to_string()), (i + 1) as u64);
            }
        }
        Controller::new("A", st, 10_000)
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn build_tray_maps_owner_to_segment_index() {
        let c = controller_with(&[("m1", Some("A")), ("m2", Some("B")), ("m3", None)]);
        let b = build_tray(
            &c,
            &ids(&["m1", "m2", "m3"]),
            &ids(&["A", "B"]),
            &ids(&["This PC", "Laptop"]),
        );
        assert_eq!(b.monitors.len(), 3);
        assert_eq!(b.monitors[0].active, 0); // m1 -> A (segment 0)
        assert_eq!(b.monitors[1].active, 1); // m2 -> B (segment 1)
        assert_eq!(b.monitors[2].active, -1); // m3 unowned -> no segment
        assert_eq!(b.peers.len(), 2);
        assert_eq!(b.peers[1].label.as_str(), "Laptop");
    }

    #[test]
    fn resolve_switch_translates_indices_to_ids_and_rejects_out_of_range() {
        let c = controller_with(&[("m1", Some("A"))]);
        let b = build_tray(&c, &ids(&["m1"]), &ids(&["A", "B"]), &[]);
        assert_eq!(
            b.resolve_switch(0, 1),
            Some(("m1".to_string(), "B".to_string()))
        );
        assert_eq!(b.resolve_switch(0, 9), None); // peer index out of range
        assert_eq!(b.resolve_switch(-1, 0), None); // negative index
    }

    #[test]
    fn status_line_surfaces_stranded_and_ddc_states() {
        let st = Arc::new(Mutex::new(MeshState::default()));
        {
            let mut s = st.lock().unwrap();
            s.ownership.mark_stranded("m1", 1);
            s.ownership.mark_ddc_disabled("m2", 1);
        }
        let c = Controller::new("A", st, 10_000);
        let b = build_tray(&c, &ids(&["m1", "m2"]), &ids(&["A"]), &[]);
        assert!(b.monitors[0].spec.as_str().contains("Stranded"));
        assert!(b.monitors[1].spec.as_str().to_lowercase().contains("ddc"));
    }

    #[test]
    fn build_presets_marks_the_active_one() {
        let p = build_presets(&ids(&["Trading", "Work", "Couch"]), Some(0));
        assert!(p[0].active);
        assert!(!p[1].active);
        assert_eq!(p[2].name.as_str(), "Couch");
    }
}
