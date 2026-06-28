use crate::driver::{Clock, Delayer, MonitorDriver};
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
///
/// The loop is bounded by both `policy.max_attempts` AND a wall-clock hard ceiling
/// (`policy.ceiling_ms`, D5/§6.3) measured against the injected [`Clock`], so a switch provably
/// terminates before the lease TTL. Note: the ceiling bounds time *between* driver calls; a driver
/// whose individual `write_input`/`try_read_input` can block (e.g. a DisplayPort push-release hang)
/// must additionally carry its own per-call timeout — the ceiling cannot interrupt a blocked syscall.
pub struct SwitchExecutor<D: MonitorDriver, L: Delayer, C: Clock> {
    driver: D,
    delayer: L,
    clock: C,
}

impl<D: MonitorDriver, L: Delayer, C: Clock> SwitchExecutor<D, L, C> {
    pub fn new(driver: D, delayer: L, clock: C) -> Self {
        Self {
            driver,
            delayer,
            clock,
        }
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

        // 3. Write / settle / verify with retries, bounded by both attempt count and the
        //    per-monitor hard ceiling (D5/§6.3).
        let started_ms = self.clock.now_ms();
        let deadline_ms = started_ms.saturating_add(u64::from(policy.ceiling_ms));
        let mut attempts = 0u32;
        for i in 0..policy.max_attempts {
            if self.clock.now_ms() >= deadline_ms {
                return SwitchResult {
                    outcome: SwitchOutcome::Failed,
                    attempts,
                    observed_value: None,
                    detail: Some(format!(
                        "Hard ceiling of {} ms reached after {attempts} attempt(s).",
                        policy.ceiling_ms
                    )),
                };
            }
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
                    self.backoff(policy, i, deadline_ms);
                    continue;
                }
                DdcWriteResult::Ok => {}
            }

            // Write accepted — let the panel settle before verifying (never sleeping past the ceiling).
            self.delay_clamped(policy.settle_ms, deadline_ms);

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
                    self.backoff(policy, i, deadline_ms);
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

    fn backoff(&self, policy: &ActuationPolicy, attempt_index: u32, deadline_ms: u64) {
        if policy.backoff_ms > 0 {
            // Linear backoff, overflow-safe (a large backoff_ms × attempt would otherwise wrap/panic).
            let wait = policy.backoff_ms.saturating_mul(attempt_index.saturating_add(1));
            self.delay_clamped(wait, deadline_ms);
        }
    }

    /// Delay `ms`, but never sleep past the hard ceiling `deadline_ms`.
    fn delay_clamped(&self, ms: u32, deadline_ms: u64) {
        let remaining = deadline_ms.saturating_sub(self.clock.now_ms());
        let capped = u64::from(ms).min(remaining);
        if capped > 0 {
            self.delayer.delay(capped.try_into().unwrap_or(u32::MAX));
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
    use crate::driver::{Clock, Delayer};
    use std::cell::Cell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    const MON: &str = "MON#0";
    const TARGET: u32 = 0x0F; // DisplayPort-1

    struct NoDelay;
    impl Delayer for NoDelay {
        fn delay(&self, _ms: u32) {}
    }

    /// Clock that never advances — fine for tests that don't exercise the ceiling.
    struct ZeroClock;
    impl Clock for ZeroClock {
        fn now_ms(&self) -> u64 {
            0
        }
    }

    /// A monotonic test clock plus a delayer that advances it, so the hard ceiling is exercised
    /// deterministically without real sleeps.
    #[derive(Clone, Default)]
    struct TestClock(Rc<Cell<u64>>);
    impl Clock for TestClock {
        fn now_ms(&self) -> u64 {
            self.0.get()
        }
    }
    impl Delayer for TestClock {
        fn delay(&self, ms: u32) {
            self.0.set(self.0.get() + u64::from(ms));
        }
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

    fn exec(fake: Fake) -> SwitchExecutor<Fake, NoDelay, ZeroClock> {
        SwitchExecutor::new(fake, NoDelay, ZeroClock)
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

    #[test]
    fn aborts_on_hard_ceiling_before_exhausting_attempts() {
        // A panel that keeps mis-reporting + a generous attempt budget, but a 2.5s ceiling and a
        // settle of 1s per attempt: the loop must abort on the ceiling, NOT run all 1000 attempts.
        let mut fake = Fake::new();
        fake.current = 0x11;
        fake.apply_write = false;
        let clock = TestClock::default();
        let mut e = SwitchExecutor::new(fake, clock.clone(), clock);
        let mut p = ActuationPolicy::new([TARGET], []);
        p.max_attempts = 1000;
        p.settle_ms = 1000;
        p.backoff_ms = 0;
        p.ceiling_ms = 2500;
        let r = e.execute(&req(), &p);
        assert_eq!(r.outcome, SwitchOutcome::Failed);
        assert!(
            r.detail.as_deref().unwrap_or("").contains("ceiling"),
            "expected ceiling detail, got {:?}",
            r.detail
        );
        // ~3 attempts fit before 2.5s elapses (settle 1s each), far short of max_attempts.
        assert!(r.attempts <= 4, "attempts should be ceiling-bounded, got {}", r.attempts);
    }
}
