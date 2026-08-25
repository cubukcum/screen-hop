//! Pure local view-model construction for the single-monitor/two-source UI.
//!
//! No mesh, peer, ownership, or network state belongs here. The controller translates the
//! persisted local configuration and an observed [`SourceState`] into honest display state. A
//! pending request never changes the shown active source; only a confirmed backend state does.

use screenhop_app::{LocalConfig, LocalSwitchReport, LocalSwitchStatus, SourceSlot, SourceState};
use screenhop_core::SwitchOutcome;

/// Numeric values intentionally match the `status-kind` contract in `ui/app.slint`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum StatusKind {
    Neutral = 0,
    Confirmed = 1,
    Switching = 2,
    Warning = 3,
    Error = 4,
}

impl StatusKind {
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

/// One of the two configured destinations as rendered by Slint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceView {
    pub label: String,
    pub detail: String,
    pub configured: bool,
}

/// Complete backend-independent state for the flyout and local settings page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalView {
    pub monitor_name: String,
    pub monitor_detail: String,
    pub sources: [SourceView; 2],
    /// 0 = A, 1 = B, -1 = unknown/unreadable/unconfigured.
    pub active_source: i32,
    /// 0 = A, 1 = B, -1 = no operation in flight.
    pub pending_source: i32,
    pub ready: bool,
    pub status_kind: StatusKind,
    pub status_text: String,
    pub message: String,
}

/// Stateless adapter kept as a named type so the binary has one obvious UI policy entry point.
#[derive(Debug, Default, Clone, Copy)]
pub struct Controller;

impl Controller {
    pub const fn new() -> Self {
        Self
    }

    /// Build the local flyout/settings model. `pending` is visual progress only and deliberately
    /// does not alter `active_source`; the caller must provide a newly observed/confirmed state.
    pub fn view(
        &self,
        config: &LocalConfig,
        state: SourceState,
        pending: Option<SourceSlot>,
    ) -> LocalView {
        build_view(config, state, pending)
    }

    /// Overlay the final switch report on a freshly-built view. Confirmed success may update the
    /// active slot because the executor read it back. Inconclusive success stays explicitly
    /// unknown and warning-colored instead of pretending confirmation.
    pub fn view_after_report(
        &self,
        config: &LocalConfig,
        state: SourceState,
        report: &LocalSwitchReport,
    ) -> LocalView {
        with_switch_report(build_view(config, state, None), config, report)
    }
}

pub fn build_view(
    config: &LocalConfig,
    state: SourceState,
    pending: Option<SourceSlot>,
) -> LocalView {
    let (monitor_name, monitor_detail) = monitor_copy(config);
    let sources = SourceSlot::ALL.map(|slot| {
        let source = config.source(slot);
        SourceView {
            label: source.label.clone(),
            detail: source
                .confirmed_value
                .map(|value| format!("Input code 0x{value:02X}"))
                .unwrap_or_else(|| "Not captured".to_owned()),
            configured: source.confirmed_value.is_some(),
        }
    });

    let ready = config.is_ready();
    let (active_source, status_kind, status_text, message) = match state {
        SourceState::A => active_copy(config, SourceSlot::A),
        SourceState::B => active_copy(config, SourceSlot::B),
        SourceState::Unknown(value) if ready => (
            -1,
            StatusKind::Warning,
            "Current input is not one of the captured sources".to_owned(),
            format!(
                "The monitor reported 0x{value:02X}. Choose source A or B explicitly; screen-hop will not guess."
            ),
        ),
        SourceState::Unreadable if ready => (
            -1,
            StatusKind::Warning,
            "Current input could not be confirmed".to_owned(),
            "The monitor did not answer the input read. Choose a destination explicitly, or use the monitor's source control."
                .to_owned(),
        ),
        SourceState::Unconfigured | SourceState::Unknown(_) | SourceState::Unreadable => (
            -1,
            StatusKind::Warning,
            "Setup required".to_owned(),
            "Choose one local monitor and capture two distinct input sources before switching."
                .to_owned(),
        ),
    };

    let mut view = LocalView {
        monitor_name,
        monitor_detail,
        sources,
        active_source,
        pending_source: pending.map_or(-1, |slot| slot.index() as i32),
        ready,
        status_kind,
        status_text,
        message,
    };

    if let Some(slot) = pending {
        let label = config.source(slot).label.trim();
        let label = if label.is_empty() {
            fallback_slot_label(slot)
        } else {
            label
        };
        view.status_kind = StatusKind::Switching;
        view.status_text = format!("Switching to {label}…");
        view.message =
            "Waiting for the monitor to confirm the new input. The current source has not been changed in the UI yet."
                .to_owned();
    }

    view
}

/// Translate an executor/app result into honest final copy. This is kept pure so the binary can
/// use the same wording for synchronous and worker-thread completion paths.
pub fn with_switch_report(
    mut view: LocalView,
    config: &LocalConfig,
    report: &LocalSwitchReport,
) -> LocalView {
    view.pending_source = -1;
    let requested = report.requested_source;
    let target_label = requested
        .map(|slot| config.source(slot).label.trim())
        .filter(|label| !label.is_empty())
        .map(str::to_owned)
        .or_else(|| requested.map(|slot| fallback_slot_label(slot).to_owned()))
        .unwrap_or_else(|| "the requested source".to_owned());
    let detail = report.detail.as_deref().unwrap_or("").trim();

    match report.status {
        LocalSwitchStatus::Executed(SwitchOutcome::Success) => {
            if let Some(slot) = requested {
                view.active_source = slot.index() as i32;
            }
            view.status_kind = StatusKind::Confirmed;
            view.status_text = format!("Switched to {target_label}");
            view.message = "The monitor confirmed the new input.".to_owned();
        }
        LocalSwitchStatus::Executed(SwitchOutcome::AssumedSuccessReadbackInconclusive) => {
            view.active_source = -1;
            view.status_kind = StatusKind::Warning;
            view.status_text = format!("Switch to {target_label} was sent but not confirmed");
            view.message = if detail.is_empty() {
                "The write succeeded, but the monitor stopped reporting its input. Use an explicit source button if the picture is not where expected."
                    .to_owned()
            } else {
                detail.to_owned()
            };
        }
        LocalSwitchStatus::Executed(
            SwitchOutcome::Failed
            | SwitchOutcome::BlockedValue
            | SwitchOutcome::NeedsCalibration
            | SwitchOutcome::DdcUnavailable
            | SwitchOutcome::Unsupported,
        )
        | LocalSwitchStatus::NoWrite(_) => {
            view.status_kind = StatusKind::Error;
            view.status_text = format!("Could not switch to {target_label}");
            view.message = if detail.is_empty() {
                "No confirmed input change was made. Try again, rerun setup, or use the monitor's source control."
                    .to_owned()
            } else {
                detail.to_owned()
            };
        }
    }

    view
}

fn monitor_copy(config: &LocalConfig) -> (String, String) {
    let Some(id) = config.selected_monitor.as_deref() else {
        return (
            "No monitor selected".to_owned(),
            "Run setup to choose a local DDC/CI display".to_owned(),
        );
    };

    match config.monitor_aliases.get(id) {
        Some(alias) => (alias.clone(), id.to_owned()),
        None => (id.to_owned(), "Local DDC/CI monitor".to_owned()),
    }
}

fn active_copy(config: &LocalConfig, slot: SourceSlot) -> (i32, StatusKind, String, String) {
    if !config.is_ready() {
        return (
            -1,
            StatusKind::Warning,
            "Setup required".to_owned(),
            "Choose one local monitor and capture two distinct input sources before switching."
                .to_owned(),
        );
    }

    let label = config.source(slot).label.trim();
    let label = if label.is_empty() {
        fallback_slot_label(slot)
    } else {
        label
    };
    (
        slot.index() as i32,
        StatusKind::Confirmed,
        format!("Showing {label}"),
        String::new(),
    )
}

const fn fallback_slot_label(slot: SourceSlot) -> &'static str {
    match slot {
        SourceSlot::A => "Source A",
        SourceSlot::B => "Source B",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready_config() -> LocalConfig {
        let mut config = LocalConfig {
            selected_monitor: Some("display-1".to_owned()),
            ..LocalConfig::default()
        };
        config
            .monitor_aliases
            .insert("display-1".to_owned(), "Studio monitor".to_owned());
        config.source_mut(SourceSlot::A).label = "Desk PC".to_owned();
        config.source_mut(SourceSlot::A).confirmed_value = Some(0x0f);
        config.source_mut(SourceSlot::B).label = "Laptop".to_owned();
        config.source_mut(SourceSlot::B).confirmed_value = Some(0x11);
        config
    }

    #[test]
    fn ready_view_uses_the_confirmed_local_source() {
        let view = build_view(&ready_config(), SourceState::A, None);
        assert!(view.ready);
        assert_eq!(view.monitor_name, "Studio monitor");
        assert_eq!(view.monitor_detail, "display-1");
        assert_eq!(view.active_source, 0);
        assert_eq!(view.pending_source, -1);
        assert_eq!(view.status_kind, StatusKind::Confirmed);
        assert_eq!(view.sources[1].label, "Laptop");
        assert!(view.sources.iter().all(|source| source.configured));
    }

    #[test]
    fn unconfigured_view_invites_setup_and_actuates_nothing() {
        let view = build_view(&LocalConfig::default(), SourceState::Unconfigured, None);
        assert!(!view.ready);
        assert_eq!(view.active_source, -1);
        assert_eq!(view.pending_source, -1);
        assert!(view.status_text.contains("Setup"));
        assert!(view.sources.iter().all(|source| !source.configured));
    }

    #[test]
    fn unknown_value_stays_unknown_and_explains_the_exact_read() {
        let view = build_view(&ready_config(), SourceState::Unknown(0x1b), None);
        assert!(view.ready);
        assert_eq!(view.active_source, -1);
        assert_eq!(view.status_kind, StatusKind::Warning);
        assert!(view.message.contains("0x1B"));
        assert!(view.message.contains("will not guess"));
    }

    #[test]
    fn pending_target_does_not_optimistically_change_the_active_source() {
        let view = build_view(&ready_config(), SourceState::A, Some(SourceSlot::B));
        assert_eq!(view.active_source, 0);
        assert_eq!(view.pending_source, 1);
        assert_eq!(view.status_kind, StatusKind::Switching);
        assert!(view.status_text.contains("Laptop"));
        assert!(view.message.contains("not been changed"));
    }

    #[test]
    fn inconclusive_report_never_claims_a_confirmed_active_source() {
        let config = ready_config();
        let report = LocalSwitchReport {
            requested_source: Some(SourceSlot::B),
            state_before: Some(SourceState::A),
            status: LocalSwitchStatus::Executed(SwitchOutcome::AssumedSuccessReadbackInconclusive),
            attempts: 1,
            observed_value: None,
            detail: None,
        };
        let view = Controller::new().view_after_report(&config, SourceState::Unreadable, &report);
        assert_eq!(view.active_source, -1);
        assert_eq!(view.status_kind, StatusKind::Warning);
        assert!(view.status_text.contains("not confirmed"));
    }

    #[test]
    fn failed_report_is_an_error_and_preserves_the_last_confirmed_source() {
        let config = ready_config();
        let report = LocalSwitchReport {
            requested_source: Some(SourceSlot::B),
            state_before: Some(SourceState::A),
            status: LocalSwitchStatus::Executed(SwitchOutcome::Failed),
            attempts: 3,
            observed_value: Some(0x0f),
            detail: Some("monitor remained on source A".to_owned()),
        };
        let view = Controller::new().view_after_report(&config, SourceState::A, &report);
        assert_eq!(view.active_source, 0);
        assert_eq!(view.status_kind, StatusKind::Error);
        assert_eq!(view.message, "monitor remained on source A");
    }
}
