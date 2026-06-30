//! Production [`Actuator`]: the real local DDC actuation path (M2 wiring).
//!
//! [`LocalActuator`] is the impl the [`crate::mesh::Node`] expects via [`crate::Actuator`]. It ties
//! the M1 [`SwitchExecutor`] to the two pieces that, per the audit, were never actually consulted
//! from the app layer:
//!
//! - the per-`(peer, monitor)` **calibration allow-list** (`screenhop-identity`) — the ONLY source
//!   of a writable value (D4); and
//! - the panel-global **quirks DB** (`screenhop-quirks`) — blocked set, settle timing, read-back
//!   reliability — folded into the policy via [`QuirksDb::policy_for`].
//!
//! Generic over the injected driver/delayer/clock so the same logic runs against the real ddc-hi
//! driver in production and a fake in tests.

use screenhop_core::{
    Clock, Delayer, MonitorDriver, SwitchDirection, SwitchExecutor, SwitchOutcome, SwitchRequest,
};
use screenhop_identity::CalibrationStore;
use screenhop_quirks::QuirksDb;

use crate::mesh::{ActuationReport, Actuator};

/// The local actuator for one peer.
pub struct LocalActuator<D: MonitorDriver, L: Delayer, C: Clock> {
    peer_id: String,
    executor: SwitchExecutor<D, L, C>,
    quirks: QuirksDb,
    calibration: CalibrationStore,
}

impl<D: MonitorDriver, L: Delayer, C: Clock> LocalActuator<D, L, C> {
    pub fn new(
        peer_id: impl Into<String>,
        executor: SwitchExecutor<D, L, C>,
        quirks: QuirksDb,
        calibration: CalibrationStore,
    ) -> Self {
        Self {
            peer_id: peer_id.into(),
            executor,
            quirks,
            calibration,
        }
    }

    /// Mutable access to the calibration store (e.g. to record a value learned during onboarding).
    pub fn calibration_mut(&mut self) -> &mut CalibrationStore {
        &mut self.calibration
    }

    /// Mutable access to the quirks DB (e.g. to load the local/user layers at startup, or record a
    /// learned panel-global fact via [`QuirksDb::set_local`]).
    pub fn quirks_mut(&mut self) -> &mut QuirksDb {
        &mut self.quirks
    }
}

impl<D, L, C> Actuator for LocalActuator<D, L, C>
where
    D: MonitorDriver + Send,
    L: Delayer + Send,
    C: Clock + Send,
{
    fn switch_to_self(&mut self, monitor_id: &str) -> ActuationReport {
        // Pull-to-self writes THIS peer's own self-calibrated value. With no confirmed value the
        // panel is "unknown until first active" (D4) — refuse without inventing a value to write.
        let Some(value) = self.calibration.confirmed_value(&self.peer_id, monitor_id) else {
            return ActuationReport::new(SwitchOutcome::NeedsCalibration, None);
        };

        // Build the policy from the per-(peer,monitor) allow-list PLUS the panel-global quirk
        // (blocked set, settle time, read-back reliability). A quirk can only ever restrict (D7).
        let confirmed = self.calibration.confirmed_set(&self.peer_id, monitor_id);
        let policy = self.quirks.policy_for(monitor_id, confirmed);

        let request = SwitchRequest {
            monitor_id: monitor_id.to_owned(),
            input_value: value,
            direction: SwitchDirection::PullToSelf,
        };
        let result = self.executor.execute(&request, &policy);
        ActuationReport::new(result.outcome, result.observed_value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screenhop_core::DdcWriteResult;
    use screenhop_quirks::Quirk;

    const PEER: &str = "peerA";
    const MON: &str = "mon-1";

    struct NoDelay;
    impl Delayer for NoDelay {
        fn delay(&self, _ms: u32) {}
    }
    struct ZeroClock;
    impl Clock for ZeroClock {
        fn now_ms(&self) -> u64 {
            0
        }
    }

    struct FakeDriver {
        available: bool,
        current: u32,
        write_result: DdcWriteResult,
        apply: bool,
        writes: Vec<u32>,
    }
    impl FakeDriver {
        fn new() -> Self {
            Self {
                available: true,
                current: 0x11,
                write_result: DdcWriteResult::Ok,
                apply: true,
                writes: Vec::new(),
            }
        }
    }
    impl MonitorDriver for FakeDriver {
        fn is_ddc_available(&mut self, _id: &str) -> bool {
            self.available
        }
        fn try_read_input(&mut self, _id: &str) -> Option<u32> {
            Some(self.current)
        }
        fn write_input(&mut self, _id: &str, value: u32) -> DdcWriteResult {
            self.writes.push(value);
            if self.write_result == DdcWriteResult::Ok && self.apply {
                self.current = value;
            }
            self.write_result
        }
    }

    fn actuator(
        driver: FakeDriver,
        quirks: QuirksDb,
        calibration: CalibrationStore,
    ) -> LocalActuator<FakeDriver, NoDelay, ZeroClock> {
        let exec = SwitchExecutor::new(driver, NoDelay, ZeroClock);
        LocalActuator::new(PEER, exec, quirks, calibration)
    }

    #[test]
    fn uncalibrated_monitor_refuses_without_writing() {
        // No confirmed value for (PEER, MON) -> "unknown until first active" -> no write.
        let mut a = actuator(
            FakeDriver::new(),
            QuirksDb::default(),
            CalibrationStore::new(),
        );
        let report = a.switch_to_self(MON);
        assert_eq!(report.outcome, SwitchOutcome::NeedsCalibration);
        // The driver is moved into the executor; we can't peek writes here, but the executor was
        // never reached (we returned before building a request). The next test proves a calibrated
        // value DOES reach the driver, so this branch genuinely short-circuits.
        assert_eq!(report.observed, None);
    }

    #[test]
    fn calibrated_pull_to_self_succeeds() {
        let mut cal = CalibrationStore::new();
        cal.record(PEER, MON, 0x0F);
        let mut a = actuator(FakeDriver::new(), QuirksDb::default(), cal);
        let report = a.switch_to_self(MON);
        assert_eq!(report.outcome, SwitchOutcome::Success);
        assert_eq!(report.observed, Some(0x0F));
    }

    #[test]
    fn quirk_blocked_value_is_consulted_and_prevents_the_write() {
        // The panel-global quirk blocks 0x0F. Even though 0x0F is this peer's calibrated value, the
        // quirk's blocked set must flow through policy_for and stop the write (proves wiring).
        let mut cal = CalibrationStore::new();
        cal.record(PEER, MON, 0x0F);
        let mut quirks = QuirksDb::default();
        quirks.set_local(
            MON,
            Quirk {
                blocked_input_values: vec![0x0F],
                ..Quirk::default()
            },
        );
        let mut a = actuator(FakeDriver::new(), quirks, cal);
        let report = a.switch_to_self(MON);
        assert_eq!(report.outcome, SwitchOutcome::BlockedValue);
    }

    #[test]
    fn quirk_readback_unreliable_flows_into_the_policy() {
        // A read-back-unreliable quirk must make a good write report assumed-success (no verify).
        let mut cal = CalibrationStore::new();
        cal.record(PEER, MON, 0x0F);
        let mut quirks = QuirksDb::default();
        quirks.set_local(
            MON,
            Quirk {
                readback_unreliable: Some(true),
                ..Quirk::default()
            },
        );
        let mut a = actuator(FakeDriver::new(), quirks, cal);
        let report = a.switch_to_self(MON);
        assert_eq!(
            report.outcome,
            SwitchOutcome::AssumedSuccessReadbackInconclusive
        );
    }
}
