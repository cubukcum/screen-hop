//! Local two-source switching over the existing defensive DDC/CI executor.

use screenhop_core::{
    Clock, Delayer, MonitorDriver, SwitchExecutor, SwitchOutcome, SwitchRequest, SwitchResult,
};
use screenhop_quirks::QuirksDb;

use crate::persist::{LocalConfig, SourceSlot};

/// The selected monitor's currently observed source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceState {
    A,
    B,
    /// DDC returned a concrete input value, but it is not either configured source.
    Unknown(u32),
    /// DDC could not read input select, as commonly happens after switching away from this PC.
    Unreadable,
    /// A monitor and two valid, distinct source values have not been configured yet.
    Unconfigured,
}

impl SourceState {
    pub const fn slot(self) -> Option<SourceSlot> {
        match self {
            Self::A => Some(SourceSlot::A),
            Self::B => Some(SourceSlot::B),
            Self::Unknown(_) | Self::Unreadable | Self::Unconfigured => None,
        }
    }
}

/// Why a local request safely stopped before invoking the executor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalNoWriteReason {
    InvalidConfig,
    SafetyPolicyUnavailable,
    NoMonitorSelected,
    SourcesIncomplete,
    UnknownCurrentInput,
    UnreadableCurrentInput,
}

/// Either the untouched executor outcome or an app-level, guaranteed-no-write refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalSwitchStatus {
    Executed(SwitchOutcome),
    NoWrite(LocalNoWriteReason),
}

impl LocalSwitchStatus {
    pub const fn executor_outcome(self) -> Option<SwitchOutcome> {
        match self {
            Self::Executed(outcome) => Some(outcome),
            Self::NoWrite(_) => None,
        }
    }

    pub fn is_effective_success(self) -> bool {
        matches!(self, Self::Executed(outcome) if outcome.is_effective_success())
    }
}

/// Result reported to the local UI/tray layer. No mesh identities or ownership types are involved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSwitchReport {
    pub requested_source: Option<SourceSlot>,
    /// Populated by [`LocalSwitcher::toggle`]; explicit `switch_to` requests do not need a live
    /// pre-read and leave this as `None`.
    pub state_before: Option<SourceState>,
    pub status: LocalSwitchStatus,
    /// Preserved directly from [`SwitchExecutor`]. App-level refusals always report zero.
    pub attempts: u32,
    /// Preserved directly from [`SwitchExecutor`].
    pub observed_value: Option<u32>,
    pub detail: Option<String>,
}

impl LocalSwitchReport {
    fn no_write(
        requested_source: Option<SourceSlot>,
        state_before: Option<SourceState>,
        reason: LocalNoWriteReason,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            requested_source,
            state_before,
            status: LocalSwitchStatus::NoWrite(reason),
            attempts: 0,
            observed_value: None,
            detail: Some(detail.into()),
        }
    }

    fn from_executor(requested_source: SourceSlot, result: SwitchResult) -> Self {
        Self {
            requested_source: Some(requested_source),
            state_before: None,
            status: LocalSwitchStatus::Executed(result.outcome),
            attempts: result.attempts,
            observed_value: result.observed_value,
            detail: result.detail,
        }
    }
}

struct ReadyConfig {
    monitor_id: String,
    model_token: Option<String>,
    values: [u16; 2],
}

/// Single-PC input switcher, generic over the same executor dependencies as the core state machine.
pub struct LocalSwitcher<D: MonitorDriver, L: Delayer, C: Clock> {
    executor: SwitchExecutor<D, L, C>,
    quirks: QuirksDb,
    writes_disabled: Option<String>,
}

impl<D: MonitorDriver, L: Delayer, C: Clock> LocalSwitcher<D, L, C> {
    pub fn new(executor: SwitchExecutor<D, L, C>, quirks: QuirksDb) -> Self {
        Self {
            executor,
            quirks,
            writes_disabled: None,
        }
    }

    pub fn driver_mut(&mut self) -> &mut D {
        self.executor.driver_mut()
    }

    pub fn quirks_mut(&mut self) -> &mut QuirksDb {
        &mut self.quirks
    }

    /// Permanently disable writes for this switcher instance when a safety-policy layer could not
    /// be loaded or validated. Live reads remain available for diagnostics and UI state.
    pub fn disable_writes(&mut self, error: impl Into<String>) {
        let error = error.into();
        let error = if error.trim().is_empty() {
            "safety policy is unavailable".to_owned()
        } else {
            error
        };
        match &mut self.writes_disabled {
            Some(existing) if !existing.contains(&error) => {
                existing.push_str("; ");
                existing.push_str(&error);
            }
            Some(_) => {}
            None => self.writes_disabled = Some(error),
        }
    }

    pub fn writes_disabled_reason(&self) -> Option<&str> {
        self.writes_disabled.as_deref()
    }

    /// Read and classify the selected monitor without issuing a write.
    pub fn read_state(&mut self, config: &LocalConfig) -> SourceState {
        let Ok(ready) = Self::ready_config(config) else {
            return SourceState::Unconfigured;
        };
        Self::classify_read(
            self.executor.driver_mut().try_read_input(&ready.monitor_id),
            ready.values,
        )
    }

    /// Explicitly switch to A or B. The target is therefore impossible to source from anywhere
    /// except the configured two-value allow-list.
    pub fn switch_to(&mut self, config: &mut LocalConfig, source: SourceSlot) -> LocalSwitchReport {
        if let Some(report) = self.safety_policy_refusal(Some(source), None) {
            return report;
        }
        let ready = match Self::ready_config(config) {
            Ok(ready) => ready,
            Err((reason, detail)) => {
                return LocalSwitchReport::no_write(Some(source), None, reason, detail);
            }
        };

        let value = ready.values[source.index()];
        // Both configured values, and exactly those two values, form the executor allow-list.
        // Quirk policy is merged afterward and can only further restrict it; blocked values win in
        // the executor before any write.
        let policy = self.quirks.policy_for_monitor(
            &ready.monitor_id,
            ready.model_token.as_deref(),
            ready.values.map(u32::from),
        );
        let request = SwitchRequest {
            monitor_id: ready.monitor_id,
            input_value: u32::from(value),
        };
        let result = self.executor.execute(&request, &policy);
        let report = LocalSwitchReport::from_executor(source, result);
        if report.status.is_effective_success() {
            config.last_requested = Some(source);
        }
        report
    }

    /// Toggle to the opposite of a live A/B read. A previous effective request is consulted only
    /// when input read-back is unavailable; a concrete unknown value always stops without writing.
    pub fn toggle(&mut self, config: &mut LocalConfig) -> LocalSwitchReport {
        if let Some(report) = self.safety_policy_refusal(None, None) {
            return report;
        }
        let ready = match Self::ready_config(config) {
            Ok(ready) => ready,
            Err((reason, detail)) => {
                return LocalSwitchReport::no_write(
                    None,
                    Some(SourceState::Unconfigured),
                    reason,
                    detail,
                );
            }
        };
        let state = Self::classify_read(
            self.executor.driver_mut().try_read_input(&ready.monitor_id),
            ready.values,
        );

        let target = match state {
            SourceState::A => SourceSlot::B,
            SourceState::B => SourceSlot::A,
            SourceState::Unknown(value) => {
                return LocalSwitchReport::no_write(
                    None,
                    Some(state),
                    LocalNoWriteReason::UnknownCurrentInput,
                    format!(
                        "live input value {value} is neither configured source; refusing to guess"
                    ),
                );
            }
            SourceState::Unreadable => match config.last_requested {
                Some(previous) => previous.opposite(),
                None => {
                    return LocalSwitchReport::no_write(
                        None,
                        Some(state),
                        LocalNoWriteReason::UnreadableCurrentInput,
                        "input is unreadable and there is no last successful request to invert",
                    );
                }
            },
            SourceState::Unconfigured => unreachable!("ready config cannot be unconfigured"),
        };

        let mut report = self.switch_to(config, target);
        report.state_before = Some(state);
        report
    }

    fn classify_read(value: Option<u32>, values: [u16; 2]) -> SourceState {
        match value {
            Some(value) if value == u32::from(values[SourceSlot::A.index()]) => SourceState::A,
            Some(value) if value == u32::from(values[SourceSlot::B.index()]) => SourceState::B,
            Some(value) => SourceState::Unknown(value),
            None => SourceState::Unreadable,
        }
    }

    fn ready_config(config: &LocalConfig) -> Result<ReadyConfig, (LocalNoWriteReason, String)> {
        if let Err(error) = config.validate() {
            return Err((LocalNoWriteReason::InvalidConfig, error.to_string()));
        }
        let monitor_id = config.selected_monitor.clone().ok_or_else(|| {
            (
                LocalNoWriteReason::NoMonitorSelected,
                "no monitor is selected".to_owned(),
            )
        })?;
        let values = config.confirmed_values().ok_or_else(|| {
            (
                LocalNoWriteReason::SourcesIncomplete,
                "both source input values must be confirmed before switching".to_owned(),
            )
        })?;
        Ok(ReadyConfig {
            monitor_id,
            model_token: config.selected_monitor_model_token.clone(),
            values,
        })
    }

    fn safety_policy_refusal(
        &self,
        requested_source: Option<SourceSlot>,
        state_before: Option<SourceState>,
    ) -> Option<LocalSwitchReport> {
        self.writes_disabled.as_ref().map(|error| {
            LocalSwitchReport::no_write(
                requested_source,
                state_before,
                LocalNoWriteReason::SafetyPolicyUnavailable,
                format!("safety policy unavailable; writes are disabled: {error}"),
            )
        })
    }
}
