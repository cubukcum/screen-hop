use crate::driver::{Delayer, MonitorDriver};
use crate::types::{
    ActuationPolicy, DdcWriteResult, SwitchOutcome, SwitchRequest, SwitchResult,
};

/// VCP feature code for Input Select.
pub const VCP_INPUT_SELECT: u8 = 0x60;

/// Executes a single, already-resolved [`SwitchRequest`] against a [`MonitorDriver`],
/// implementing the actuation state machine from docs/PLAN-screen-hop.md §6.1:
///
/// ```text
/// capability gate -> soft-brick guards -> write -> settle -> verify -> retry/commit/fail
/// ```
///
/// Direction selection and value resolution happen upstream (orchestrator, M4); this
/// type owns the defensive write/verify loop and the soft-brick guarantees.
pub struct SwitchExecutor<D: MonitorDriver, L: Delayer> {
    driver: D,
    delayer: L,
}

impl<D: MonitorDriver, L: Delayer> SwitchExecutor<D, L> {
    pub fn new(driver: D, delayer: L) -> Self {
        Self { driver, delayer }
    }

    /// Access to the underlying driver (e.g. for calibration reads).
    pub fn driver_mut(&mut self) -> &mut D {
        &mut self.driver
    }

    pub fn execute(&mut self, request: &SwitchRequest, policy: &ActuationPolicy) -> SwitchResult {
        // 1. Capability gate.
        if !self.driver.is_ddc_available(&request.monitor_id) {
            return refusal(
                SwitchOutcome::DdcUnavailable,
                "DDC/CI unavailable (disabled in OSD or unresponsive).",
            );
        }

        // 2. Soft-brick guards (D7): never write a blocked or non-self-confirmed value.
        if policy.blocked_values.contains(&request.input_value) {
            return refusal(
                SwitchOutcome::BlockedValue,
                &format!(
                    "Input 0x{:02X} is on this monitor's blocked list.",
                    request.input_value
                ),
            );
        }
        if !policy.confirmed_values.contains(&request.input_value) {
            return refusal(
                SwitchOutcome::NeedsCalibration,
                &format!(
                    "Input 0x{:02X} is not self-confirmed for this peer+monitor.",
                    request.input_value
                ),
            );
        }

        // 3. Write / settle / verify with retries.
        let mut attempts = 0u32;
        for i in 0..policy.max_attempts {
            attempts += 1;

            match self.driver.write_input(&request.monitor_id, request.input_value) {
                DdcWriteResult::Unsupported => {
                    // The OS/monitor rejected the code outright; retrying won't help.
                    return SwitchResult {
                        outcome: SwitchOutcome::Unsupported,
                        attempts,
                        observed_value: None,
                        detail: Some(
                            "Set input reported the code unsupported; use a fallback path.".into(),
                        ),
                    };
                }
                DdcWriteResult::Failed => {
                    self.backoff(policy, i);
                    continue;
                }
                DdcWriteResult::Ok => {}
            }

            // Write accepted — let the panel settle before verifying.
            self.delayer.delay(policy.settle_ms);

            if !policy.readback_reliable {
                return SwitchResult {
                    outcome: SwitchOutcome::AssumedSuccessReadbackInconclusive,
                    attempts,
                    observed_value: None,
                    detail: Some(
                        "Write OK; read-back skipped (panel marked read-back-unreliable).".into(),
                    ),
                };
            }

            match self.driver.try_read_input(&request.monitor_id) {
                Some(observed) if observed == request.input_value => {
                    return SwitchResult {
                        outcome: SwitchOutcome::Success,
                        attempts,
                        observed_value: Some(observed),
                        detail: None,
                    };
                }
                Some(_) => {
                    // Confirmed-wrong: the write didn't take. Retry.
                    self.backoff(policy, i);
                    continue;
                }
                None => {
                    // Read-back FAILED after a successful write. Per the plan this is
                    // INCONCLUSIVE, not failure — re-issuing risks flapping, so commit.
                    return SwitchResult {
                        outcome: SwitchOutcome::AssumedSuccessReadbackInconclusive,
                        attempts,
                        observed_value: None,
                        detail: Some("Write OK; read-back failed (inconclusive).".into()),
                    };
                }
            }
        }

        SwitchResult {
            outcome: SwitchOutcome::Failed,
            attempts,
            observed_value: None,
            detail: Some(format!("Switch not confirmed after {attempts} attempt(s).")),
        }
    }

    fn backoff(&self, policy: &ActuationPolicy, attempt_index: u32) {
        if policy.backoff_ms > 0 {
            self.delayer.delay(policy.backoff_ms * (attempt_index + 1));
        }
    }
}

fn refusal(outcome: SwitchOutcome, detail: &str) -> SwitchResult {
    SwitchResult {
        outcome,
        attempts: 0,
        observed_value: None,
        detail: Some(detail.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::Delayer;
    use std::collections::VecDeque;

    const MON: &str = "MON#0";
    const TARGET: u32 = 0x0F; // DisplayPort-1

    struct NoDelay;
    impl Delayer for NoDelay {
        fn delay(&self, _ms: u32) {}
    }

    struct Fake {
        ddc_available: bool,
        current: u32,
        write_result: DdcWriteResult,
        write_queue: VecDeque<DdcWriteResult>,
        read_succeeds: bool,
        apply_write: bool,
        write_calls: u32,
        read_calls: u32,
    }

    impl Fake {
        fn new() -> Self {
            Self {
                ddc_available: true,
                current: 0x11,
                write_result: DdcWriteResult::Ok,
                write_queue: VecDeque::new(),
                read_succeeds: true,
                apply_write: true,
                write_calls: 0,
                read_calls: 0,
            }
        }
    }

    impl MonitorDriver for Fake {
        fn is_ddc_available(&mut self, _id: &str) -> bool {
            self.ddc_available
        }
        fn try_read_input(&mut self, _id: &str) -> Option<u32> {
            self.read_calls += 1;
            if self.read_succeeds {
                Some(self.current)
            } else {
                None
            }
        }
        fn write_input(&mut self, _id: &str, value: u32) -> DdcWriteResult {
            self.write_calls += 1;
            let r = self.write_queue.pop_front().unwrap_or(self.write_result);
            if r == DdcWriteResult::Ok && self.apply_write {
                self.current = value;
            }
            r
        }
    }

    fn policy(readback_reliable: bool, max_attempts: u32, blocked: &[u32], confirmed: &[u32]) -> ActuationPolicy {
        let mut p = ActuationPolicy::new(confirmed.iter().copied(), blocked.iter().copied());
        p.max_attempts = max_attempts;
        p.settle_ms = 0;
        p.backoff_ms = 0;
        p.readback_reliable = readback_reliable;
        p
    }

    fn default_policy() -> ActuationPolicy {
        policy(true, 3, &[], &[TARGET])
    }

    fn req() -> SwitchRequest {
        SwitchRequest {
            monitor_id: MON.to_string(),
            input_value: TARGET,
            direction: SwitchDirection::PullToSelf,
        }
    }
    use crate::types::SwitchDirection;

    fn exec(fake: Fake) -> SwitchExecutor<Fake, NoDelay> {
        SwitchExecutor::new(fake, NoDelay)
    }

    #[test]
    fn succeeds_when_write_and_readback_match() {
        let mut e = exec(Fake::new());
        let r = e.execute(&req(), &default_policy());
        assert_eq!(r.outcome, SwitchOutcome::Success);
        assert_eq!(r.attempts, 1);
        assert_eq!(r.observed_value, Some(TARGET));
        assert_eq!(e.driver_mut().write_calls, 1);
    }

    #[test]
    fn refuses_when_ddc_unavailable_without_writing() {
        let mut fake = Fake::new();
        fake.ddc_available = false;
        let mut e = exec(fake);
        let r = e.execute(&req(), &default_policy());
        assert_eq!(r.outcome, SwitchOutcome::DdcUnavailable);
        assert_eq!(e.driver_mut().write_calls, 0);
    }

    #[test]
    fn refuses_blocked_value_without_writing() {
        let mut e = exec(Fake::new());
        let r = e.execute(&req(), &policy(true, 3, &[TARGET], &[TARGET]));
        assert_eq!(r.outcome, SwitchOutcome::BlockedValue);
        assert_eq!(e.driver_mut().write_calls, 0);
    }

    #[test]
    fn refuses_unconfirmed_value_without_writing() {
        let mut e = exec(Fake::new());
        let r = e.execute(&req(), &policy(true, 3, &[], &[0x11]));
        assert_eq!(r.outcome, SwitchOutcome::NeedsCalibration);
        assert_eq!(e.driver_mut().write_calls, 0);
    }

    #[test]
    fn reports_unsupported_and_does_not_retry() {
        let mut fake = Fake::new();
        fake.write_result = DdcWriteResult::Unsupported;
        let mut e = exec(fake);
        let r = e.execute(&req(), &default_policy());
        assert_eq!(r.outcome, SwitchOutcome::Unsupported);
        assert_eq!(e.driver_mut().write_calls, 1);
    }

    #[test]
    fn readback_failure_after_write_is_inconclusive_not_failure() {
        let mut fake = Fake::new();
        fake.read_succeeds = false;
        let mut e = exec(fake);
        let r = e.execute(&req(), &default_policy());
        assert_eq!(r.outcome, SwitchOutcome::AssumedSuccessReadbackInconclusive);
        assert_eq!(e.driver_mut().write_calls, 1);
    }

    #[test]
    fn skips_readback_when_panel_unreliable() {
        let mut e = exec(Fake::new());
        let r = e.execute(&req(), &policy(false, 3, &[], &[TARGET]));
        assert_eq!(r.outcome, SwitchOutcome::AssumedSuccessReadbackInconclusive);
        assert_eq!(e.driver_mut().write_calls, 1);
        assert_eq!(e.driver_mut().read_calls, 0);
    }

    #[test]
    fn retries_after_failed_write_then_succeeds() {
        let mut fake = Fake::new();
        fake.write_queue = VecDeque::from(vec![DdcWriteResult::Failed, DdcWriteResult::Ok]);
        let mut e = exec(fake);
        let r = e.execute(&req(), &default_policy());
        assert_eq!(r.outcome, SwitchOutcome::Success);
        assert_eq!(r.attempts, 2);
        assert_eq!(e.driver_mut().write_calls, 2);
    }

    #[test]
    fn fails_after_max_attempts_when_readback_keeps_mismatching() {
        let mut fake = Fake::new();
        fake.current = 0x11;
        fake.apply_write = false; // write "succeeds" but never changes the input
        let mut e = exec(fake);
        let r = e.execute(&req(), &default_policy());
        assert_eq!(r.outcome, SwitchOutcome::Failed);
        assert_eq!(r.attempts, 3);
        assert_eq!(e.driver_mut().write_calls, 3);
    }
}
