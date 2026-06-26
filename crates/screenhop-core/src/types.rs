use std::collections::HashSet;

/// Which physical path performs the DDC/CI switch (see docs/PLAN-screen-hop.md §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchDirection {
    /// Target machine writes its own input value over its own cable. Default, reliable path.
    PullToSelf,
    /// Current owner writes the target's value to hand the panel away. Flaky fallback.
    PushRelease,
}

/// Result of attempting a single DDC/CI input switch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchOutcome {
    /// Write succeeded and read-back confirmed the target input.
    Success,
    /// Write succeeded; read-back could not confirm (unreliable on some panels) — success, flagged.
    AssumedSuccessReadbackInconclusive,
    /// Write was issued but the switch could not be confirmed within the retry budget.
    Failed,
    /// Refused before any write: the value is on the monitor's blocked list (soft-brick guard).
    BlockedValue,
    /// Refused before any write: the value is not self-confirmed for this peer+monitor.
    NeedsCalibration,
    /// Refused: DDC/CI is unavailable (disabled in OSD or unresponsive).
    DdcUnavailable,
    /// The monitor/OS reported the VCP code unsupported; the caller should try a fallback path.
    Unsupported,
}

impl SwitchOutcome {
    /// True for outcomes that mean "the monitor most likely switched".
    pub fn is_effective_success(self) -> bool {
        matches!(
            self,
            SwitchOutcome::Success | SwitchOutcome::AssumedSuccessReadbackInconclusive
        )
    }
}

/// Low-level outcome of a single DDC/CI write, as reported by the driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DdcWriteResult {
    Ok,
    Failed,
    /// The code was reported unsupported — the caller may fall back (e.g. a vendor tool).
    Unsupported,
}

/// A resolved request to switch one monitor to one input value.
#[derive(Debug, Clone)]
pub struct SwitchRequest {
    /// Stable monitor identity (EDID fingerprint id; provisional id pre-M2).
    pub monitor_id: String,
    /// The VCP 0x60 value to write — MUST be self-confirmed for the acting peer.
    pub input_value: u32,
    pub direction: SwitchDirection,
}

/// Per-switch policy plus the soft-brick guard inputs. `confirmed_values` are the values
/// self-calibrated as real for the acting peer+monitor; only these may ever be written (D7).
#[derive(Debug, Clone)]
pub struct ActuationPolicy {
    pub confirmed_values: HashSet<u32>,
    pub blocked_values: HashSet<u32>,
    /// Total write attempts before giving up.
    pub max_attempts: u32,
    /// Delay after a successful write before reading back (DDC is slow to settle).
    pub settle_ms: u32,
    /// Base backoff between retries (multiplied by attempt number).
    pub backoff_ms: u32,
    /// When false (panel quirk), skip read-back and report assumed-success after a good write.
    pub readback_reliable: bool,
}

impl ActuationPolicy {
    pub fn new(
        confirmed: impl IntoIterator<Item = u32>,
        blocked: impl IntoIterator<Item = u32>,
    ) -> Self {
        Self {
            confirmed_values: confirmed.into_iter().collect(),
            blocked_values: blocked.into_iter().collect(),
            max_attempts: 3,
            settle_ms: 1500,
            backoff_ms: 400,
            readback_reliable: true,
        }
    }
}

/// Outcome of executing a [`SwitchRequest`].
#[derive(Debug, Clone)]
pub struct SwitchResult {
    pub outcome: SwitchOutcome,
    pub attempts: u32,
    pub observed_value: Option<u32>,
    pub detail: Option<String>,
}
