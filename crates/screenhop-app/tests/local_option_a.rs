use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use screenhop_app::{
    load_config, save_config, LocalConfig, LocalNoWriteReason, LocalSwitchStatus, LocalSwitcher,
    SourceSlot, SourceState, CONFIG_FILE,
};
use screenhop_core::{
    Clock, DdcWriteResult, Delayer, MonitorDriver, SwitchExecutor, SwitchOutcome,
};
use screenhop_quirks::{Quirk, QuirksDb};

const MONITOR: &str = "MONITOR#1";
const INPUT_A: u16 = 0x0f;
const INPUT_B: u16 = 0x11;

struct NoDelay;

impl Delayer for NoDelay {
    fn delay(&self, _milliseconds: u32) {}
}

struct ZeroClock;

impl Clock for ZeroClock {
    fn now_ms(&self) -> u64 {
        0
    }
}

struct FakeDriver {
    available: bool,
    current: Option<u32>,
    reads: VecDeque<Option<u32>>,
    writes: Vec<u32>,
    write_results: VecDeque<DdcWriteResult>,
    apply_write: bool,
}

impl FakeDriver {
    fn at(value: u16) -> Self {
        Self {
            available: true,
            current: Some(u32::from(value)),
            reads: VecDeque::new(),
            writes: Vec::new(),
            write_results: VecDeque::new(),
            apply_write: true,
        }
    }

    fn unreadable() -> Self {
        Self {
            current: None,
            ..Self::at(INPUT_A)
        }
    }
}

impl MonitorDriver for FakeDriver {
    fn is_ddc_available(&mut self, _monitor_id: &str) -> bool {
        self.available
    }

    fn try_read_input(&mut self, _monitor_id: &str) -> Option<u32> {
        self.reads.pop_front().unwrap_or(self.current)
    }

    fn write_input(&mut self, _monitor_id: &str, value: u32) -> DdcWriteResult {
        self.writes.push(value);
        let result = self.write_results.pop_front().unwrap_or(DdcWriteResult::Ok);
        if result == DdcWriteResult::Ok && self.apply_write {
            self.current = Some(value);
        }
        result
    }
}

fn ready_config() -> LocalConfig {
    let mut config = LocalConfig {
        selected_monitor: Some(MONITOR.to_owned()),
        selected_monitor_model_token: Some("SAM-U32H750".to_owned()),
        ..LocalConfig::default()
    };
    config.source_mut(SourceSlot::A).confirmed_value = Some(INPUT_A);
    config.source_mut(SourceSlot::B).confirmed_value = Some(INPUT_B);
    config
}

fn switcher(driver: FakeDriver, quirks: QuirksDb) -> LocalSwitcher<FakeDriver, NoDelay, ZeroClock> {
    LocalSwitcher::new(SwitchExecutor::new(driver, NoDelay, ZeroClock), quirks)
}

#[test]
fn toggle_switches_a_to_b_and_b_to_a() {
    let mut config = ready_config();
    let mut switcher = switcher(FakeDriver::at(INPUT_A), QuirksDb::default());

    let to_b = switcher.toggle(&mut config);
    assert_eq!(to_b.requested_source, Some(SourceSlot::B));
    assert_eq!(to_b.state_before, Some(SourceState::A));
    assert_eq!(
        to_b.status,
        LocalSwitchStatus::Executed(SwitchOutcome::Success)
    );
    assert_eq!(config.last_requested, Some(SourceSlot::B));

    let to_a = switcher.toggle(&mut config);
    assert_eq!(to_a.requested_source, Some(SourceSlot::A));
    assert_eq!(to_a.state_before, Some(SourceState::B));
    assert_eq!(
        to_a.status,
        LocalSwitchStatus::Executed(SwitchOutcome::Success)
    );
    assert_eq!(
        switcher.driver_mut().writes,
        vec![u32::from(INPUT_B), u32::from(INPUT_A)]
    );
}

#[test]
fn unknown_readable_input_never_uses_last_requested_or_writes() {
    let mut config = ready_config();
    config.last_requested = Some(SourceSlot::A);
    let mut driver = FakeDriver::at(INPUT_A);
    driver.current = Some(0x44);
    let mut switcher = switcher(driver, QuirksDb::default());

    let report = switcher.toggle(&mut config);

    assert_eq!(report.state_before, Some(SourceState::Unknown(0x44)));
    assert_eq!(
        report.status,
        LocalSwitchStatus::NoWrite(LocalNoWriteReason::UnknownCurrentInput)
    );
    assert!(switcher.driver_mut().writes.is_empty());
}

#[test]
fn unreadable_input_without_last_requested_makes_no_write() {
    let mut config = ready_config();
    let mut switcher = switcher(FakeDriver::unreadable(), QuirksDb::default());

    let report = switcher.toggle(&mut config);

    assert_eq!(report.state_before, Some(SourceState::Unreadable));
    assert_eq!(
        report.status,
        LocalSwitchStatus::NoWrite(LocalNoWriteReason::UnreadableCurrentInput)
    );
    assert!(switcher.driver_mut().writes.is_empty());
}

#[test]
fn unreadable_input_uses_last_requested_only_as_inactive_input_fallback() {
    let mut config = ready_config();
    config.last_requested = Some(SourceSlot::A);
    let mut switcher = switcher(FakeDriver::unreadable(), QuirksDb::default());

    let report = switcher.toggle(&mut config);

    assert_eq!(report.state_before, Some(SourceState::Unreadable));
    assert_eq!(report.requested_source, Some(SourceSlot::B));
    assert_eq!(
        report.status,
        LocalSwitchStatus::Executed(SwitchOutcome::Success)
    );
    assert_eq!(switcher.driver_mut().writes, vec![u32::from(INPUT_B)]);
}

#[test]
fn blocked_quirk_wins_over_both_value_allow_list() {
    let mut config = ready_config();
    let mut quirks = QuirksDb::default();
    quirks.set_local(
        MONITOR,
        Quirk {
            blocked_input_values: vec![u32::from(INPUT_B)],
            ..Quirk::default()
        },
    );
    let mut switcher = switcher(FakeDriver::at(INPUT_A), quirks);

    let report = switcher.switch_to(&mut config, SourceSlot::B);

    assert_eq!(
        report.status,
        LocalSwitchStatus::Executed(SwitchOutcome::BlockedValue)
    );
    assert_eq!(report.attempts, 0);
    assert!(switcher.driver_mut().writes.is_empty());
}

#[test]
fn model_token_blocked_quirk_is_applied_to_the_selected_monitor() {
    let mut config = ready_config();
    let mut quirks = QuirksDb::default();
    quirks.set_local(
        "SAM-U32H750",
        Quirk {
            blocked_input_values: vec![u32::from(INPUT_B)],
            ..Quirk::default()
        },
    );
    let mut switcher = switcher(FakeDriver::at(INPUT_A), quirks);

    let report = switcher.switch_to(&mut config, SourceSlot::B);

    assert_eq!(
        report.status,
        LocalSwitchStatus::Executed(SwitchOutcome::BlockedValue)
    );
    assert!(switcher.driver_mut().writes.is_empty());
}

#[test]
fn unavailable_safety_policy_disables_switches_but_keeps_reads_available() {
    let mut config = ready_config();
    let mut switcher = switcher(FakeDriver::at(INPUT_A), QuirksDb::default());
    switcher.disable_writes("invalid quirks-user.json");

    assert_eq!(switcher.read_state(&config), SourceState::A);
    let explicit = switcher.switch_to(&mut config, SourceSlot::B);
    let toggle = switcher.toggle(&mut config);

    assert_eq!(
        explicit.status,
        LocalSwitchStatus::NoWrite(LocalNoWriteReason::SafetyPolicyUnavailable)
    );
    assert_eq!(
        toggle.status,
        LocalSwitchStatus::NoWrite(LocalNoWriteReason::SafetyPolicyUnavailable)
    );
    assert!(switcher.driver_mut().writes.is_empty());
}

#[test]
fn executor_retry_and_inconclusive_readback_are_preserved_in_local_report() {
    let mut config = ready_config();
    let mut driver = FakeDriver::at(INPUT_A);
    driver.write_results = VecDeque::from([DdcWriteResult::Failed, DdcWriteResult::Ok]);
    driver.reads = VecDeque::from([None]);
    let mut switcher = switcher(driver, QuirksDb::default());

    let report = switcher.switch_to(&mut config, SourceSlot::B);

    assert_eq!(
        report.status,
        LocalSwitchStatus::Executed(SwitchOutcome::AssumedSuccessReadbackInconclusive)
    );
    assert_eq!(report.attempts, 2);
    assert_eq!(switcher.driver_mut().writes, vec![u32::from(INPUT_B); 2]);
    assert_eq!(config.last_requested, Some(SourceSlot::B));
}

static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

fn temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "screenhop-local-option-a-{}-{}",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn local_config_round_trips_and_missing_file_is_safe_default() {
    let dir = temp_dir();
    assert_eq!(load_config(&dir).unwrap(), LocalConfig::default());

    let mut config = ready_config();
    config
        .monitor_aliases
        .insert(MONITOR.into(), "Center".into());
    config.last_requested = Some(SourceSlot::B);
    save_config(&dir, &config).unwrap();

    assert_eq!(load_config(&dir).unwrap(), config);
    assert!(!dir.join("config.tmp").exists());
}

#[test]
fn corrupt_incomplete_duplicate_and_out_of_range_config_fail_closed() {
    let cases = [
        "not json",
        r#"{"version":2}"#,
        r#"{
            "version": 2,
            "selected_monitor": "MONITOR#1",
            "selected_monitor_model_token": "SAM-U32H750",
            "sources": [
                {"label":"A","confirmed_value":15},
                {"label":"B","confirmed_value":15}
            ],
            "monitor_aliases": {},
            "last_requested": null
        }"#,
        r#"{
            "version": 2,
            "selected_monitor": "MONITOR#1",
            "selected_monitor_model_token": "SAM-U32H750",
            "sources": [
                {"label":"A","confirmed_value":65536},
                {"label":"B","confirmed_value":17}
            ],
            "monitor_aliases": {},
            "last_requested": null
        }"#,
    ];

    for (index, json) in cases.into_iter().enumerate() {
        let dir = temp_dir();
        fs::write(dir.join(CONFIG_FILE), json).unwrap();
        let error = load_config(&dir).expect_err(&format!("case {index} must fail closed"));
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }
}
