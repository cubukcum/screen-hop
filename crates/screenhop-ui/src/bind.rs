//! Small, deterministic conversions between the pure local view model and generated Slint types.

use screenhop_app::{LocalConfig, SourceSlot};
use slint::SharedString;

use crate::controller::LocalView;
use crate::{DetectedMonitor, SourceChoice};

/// Values ready to assign to the matching `AppWindow` properties.
#[derive(Debug, Clone)]
pub struct AppBinding {
    pub monitor_name: SharedString,
    pub monitor_detail: SharedString,
    pub sources: Vec<SourceChoice>,
    pub active_source: i32,
    pub pending_source: i32,
    pub ready: bool,
    pub status_kind: i32,
    pub status_text: SharedString,
    pub message: SharedString,
}

/// Convert without retaining a component handle, thread-affine model, or backend reference.
pub fn build_binding(view: &LocalView) -> AppBinding {
    AppBinding {
        monitor_name: view.monitor_name.as_str().into(),
        monitor_detail: view.monitor_detail.as_str().into(),
        sources: view
            .sources
            .iter()
            .map(|source| SourceChoice {
                label: source.label.as_str().into(),
                detail: source.detail.as_str().into(),
                configured: source.configured,
            })
            .collect(),
        active_source: view.active_source,
        pending_source: view.pending_source,
        ready: view.ready,
        status_kind: view.status_kind.as_i32(),
        status_text: view.status_text.as_str().into(),
        message: view.message.as_str().into(),
    }
}

/// Resolve the index supplied by Slint to one of exactly two configured source slots.
///
/// Negative/stale indices and unconfirmed sources all return `None`, so a UI event can never
/// bypass the same two-value allow-list enforced by the local switcher.
pub fn resolve_source(config: &LocalConfig, index: i32) -> Option<SourceSlot> {
    let slot = match index {
        0 => SourceSlot::A,
        1 => SourceSlot::B,
        _ => return None,
    };
    config.source(slot).confirmed_value?;
    Some(slot)
}

/// Build setup rows from `(friendly name, diagnostic detail, recommendation)` values returned by
/// monitor probing.
pub fn build_detected_monitors(rows: &[(String, String, bool)]) -> Vec<DetectedMonitor> {
    rows.iter()
        .map(|(name, detail, recommended)| DetectedMonitor {
            name: name.as_str().into(),
            detail: detail.as_str().into(),
            recommended: *recommended,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{build_view, StatusKind};
    use screenhop_app::SourceState;

    fn ready_config() -> LocalConfig {
        let mut config = LocalConfig {
            selected_monitor: Some("display-1".to_owned()),
            ..LocalConfig::default()
        };
        config.source_mut(SourceSlot::A).confirmed_value = Some(0x0f);
        config.source_mut(SourceSlot::B).confirmed_value = Some(0x11);
        config
    }

    #[test]
    fn binding_preserves_unknown_and_pending_sentinels() {
        let config = ready_config();
        let view = build_view(&config, SourceState::Unknown(0x1b), Some(SourceSlot::B));
        let binding = build_binding(&view);
        assert_eq!(binding.active_source, -1);
        assert_eq!(binding.pending_source, 1);
        assert_eq!(binding.status_kind, StatusKind::Switching.as_i32());
        assert_eq!(binding.sources.len(), 2);
    }

    #[test]
    fn source_index_guard_rejects_negative_stale_and_unconfigured_values() {
        let mut config = ready_config();
        assert_eq!(resolve_source(&config, 0), Some(SourceSlot::A));
        assert_eq!(resolve_source(&config, 1), Some(SourceSlot::B));
        assert_eq!(resolve_source(&config, -1), None);
        assert_eq!(resolve_source(&config, 2), None);
        assert_eq!(resolve_source(&config, i32::MAX), None);

        config.source_mut(SourceSlot::B).confirmed_value = None;
        assert_eq!(resolve_source(&config, 1), None);
    }

    #[test]
    fn monitor_rows_preserve_friendly_and_diagnostic_copy() {
        let rows = build_detected_monitors(&[(
            "Studio monitor".to_owned(),
            "AOC Q27G3XMN · DISPLAY\\AOC1234".to_owned(),
            true,
        )]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name.as_str(), "Studio monitor");
        assert!(rows[0].detail.as_str().contains("DISPLAY"));
        assert!(rows[0].recommended);
    }
}
