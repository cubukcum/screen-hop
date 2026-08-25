//! screen-hop local DDC/CI spike (Rust / ddc-hi): enumerate monitors, read VCP 0x60,
//! and run the guided one-PC A -> B -> A compatibility test. Cross-platform
//! (Windows/Linux/macOS) and developer-only by design.

use screenhop_core::{DdcWriteResult, MonitorDriver};
use screenhop_ddc::DdcHiDriver;
use screenhop_identity::group_by_id;
use std::io::{self, Write};
use std::{thread, time::Duration};

fn main() {
    println!("============================================================");
    println!(" screen-hop  -  local DDC/CI compatibility spike");
    println!(" Reads VCP 0x60; guided writes use only live-captured values.");
    println!("============================================================");

    let mut driver = DdcHiDriver::enumerate();
    if driver.is_empty() {
        println!("No DDC/CI-capable monitors found on this machine.");
        println!("If you DO have external monitors: enable DDC/CI in their OSD;");
        println!(
            "on Linux, ensure the i2c-dev module is loaded and you have /dev/i2c-* permissions."
        );
        return;
    }

    let cmd = std::env::args().nth(1).unwrap_or_else(|| "menu".into());
    match cmd.trim_start_matches('-') {
        "list" | "l" => print_table(&mut driver),
        "local-round-trip" | "round-trip" | "roundtrip" => {
            guided_local_round_trip_test(&mut driver)
        }
        _ => interactive(&mut driver),
    }
}

fn print_table(driver: &mut DdcHiDriver) {
    let monitors = driver.monitors();
    println!();
    println!(
        "{:<3} {:<26} {:<7} {:<14} Backend",
        "#", "Monitor", "Input", "Fingerprint"
    );
    println!("{}", "-".repeat(78));
    let mut fingerprints = Vec::new();
    for (i, m) in monitors.iter().enumerate() {
        let input = match driver.try_read_input(&m.id) {
            Some(v) => format!("0x{v:02X}"),
            None => "n/a".into(),
        };
        let label = format!(
            "{} {}",
            m.manufacturer.clone().unwrap_or_default(),
            m.model.clone().unwrap_or_else(|| "Generic Monitor".into())
        );
        let mid = m.monitor_id().unwrap_or_else(|| "(no identity)".into());
        if let Some(fp) = &m.fingerprint {
            fingerprints.push(fp.clone());
        }
        println!(
            "{:<3} {:<26} {:<7} {:<14} {}",
            i,
            truncate(label.trim(), 26),
            input,
            mid,
            m.backend
        );
    }
    let distinct = group_by_id(&fingerprints).len();
    println!();
    println!(
        "{} display handle(s) -> {} distinct monitor(s) by EDID fingerprint.",
        monitors.len(),
        distinct
    );
    println!("'#' = local handle index; 'Input' = current source; fingerprint is diagnostic only.");
}

fn interactive(driver: &mut DdcHiDriver) {
    loop {
        print_table(driver);
        println!("[1] Read input");
        println!("[2] Guided local A -> B -> A test (safe captured values only)");
        println!("[0] Exit");
        prompt("> ");
        match read_line().trim() {
            "1" => {
                if let Some(i) = pick(driver) {
                    let id = driver.monitors()[i].id.clone();
                    match driver.try_read_input(&id) {
                        Some(v) => println!("monitor #{i}: input = 0x{v:02X}"),
                        None => println!("read failed (DDC/CI disabled or unresponsive?)"),
                    }
                }
            }
            "2" => guided_local_round_trip_test(driver),
            "0" | "" => break,
            _ => println!("unknown choice"),
        }
        println!();
    }
}

const READ_POLL_ATTEMPTS: usize = 10;
const READ_POLL_INTERVAL: Duration = Duration::from_millis(500);
const SWITCH_SETTLE: Duration = Duration::from_millis(2500);
const ROUND_TRIP_DWELL: Duration = Duration::from_millis(3500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DistinctInputObservation {
    Distinct(u32),
    Unchanged,
    Inconsistent,
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpectedInputObservation {
    Expected,
    Other(u32),
    Unreadable,
}

/// Decide whether polling after the physical switch produced a stable second input. A value is
/// eligible for a later write only after two consecutive successful 0x60 reads return the same
/// value and that value differs from A.
fn classify_distinct_input(input_a: u32, readings: &[Option<u32>]) -> DistinctInputObservation {
    let mut previous_distinct = None;
    let mut saw_successful_read = false;
    let mut saw_distinct_read = false;

    for reading in readings {
        match reading {
            Some(value) => {
                saw_successful_read = true;
                if *value == input_a {
                    previous_distinct = None;
                    continue;
                }
                saw_distinct_read = true;
                if previous_distinct == Some(*value) {
                    return DistinctInputObservation::Distinct(*value);
                }
                previous_distinct = Some(*value);
            }
            None => previous_distinct = None,
        }
    }

    if saw_distinct_read {
        DistinctInputObservation::Inconsistent
    } else if saw_successful_read {
        DistinctInputObservation::Unchanged
    } else {
        DistinctInputObservation::Unreadable
    }
}

fn classify_expected_input(expected: u32, readings: &[Option<u32>]) -> ExpectedInputObservation {
    if readings.iter().flatten().any(|&value| value == expected) {
        ExpectedInputObservation::Expected
    } else if let Some(value) = readings.iter().rev().flatten().next() {
        ExpectedInputObservation::Other(*value)
    } else {
        ExpectedInputObservation::Unreadable
    }
}

fn poll_for_distinct_input(
    driver: &mut DdcHiDriver,
    monitor_id: &str,
    input_a: u32,
) -> DistinctInputObservation {
    let mut readings = Vec::with_capacity(READ_POLL_ATTEMPTS);
    for attempt in 1..=READ_POLL_ATTEMPTS {
        let observed = driver.try_read_input(monitor_id);
        match observed {
            Some(value) => println!(
                "   read {attempt}/{READ_POLL_ATTEMPTS}: 0x{value:02X}{}",
                if value == input_a {
                    " (still A)"
                } else {
                    " (distinct candidate; needs a matching repeat)"
                }
            ),
            None => println!("   read {attempt}/{READ_POLL_ATTEMPTS}: unavailable"),
        }
        readings.push(observed);
        if let DistinctInputObservation::Distinct(value) =
            classify_distinct_input(input_a, &readings)
        {
            return DistinctInputObservation::Distinct(value);
        }
        if attempt < READ_POLL_ATTEMPTS {
            thread::sleep(READ_POLL_INTERVAL);
        }
    }
    classify_distinct_input(input_a, &readings)
}

fn poll_for_expected_input(
    driver: &mut DdcHiDriver,
    monitor_id: &str,
    expected: u32,
) -> ExpectedInputObservation {
    let mut readings = Vec::with_capacity(READ_POLL_ATTEMPTS);
    for attempt in 1..=READ_POLL_ATTEMPTS {
        let observed = driver.try_read_input(monitor_id);
        match observed {
            Some(value) => println!(
                "   verification read {attempt}/{READ_POLL_ATTEMPTS}: 0x{value:02X}{}",
                if value == expected {
                    " (expected A)"
                } else {
                    " (not A)"
                }
            ),
            None => println!("   verification read {attempt}/{READ_POLL_ATTEMPTS}: unavailable"),
        }
        readings.push(observed);
        if matches!(
            classify_expected_input(expected, &readings),
            ExpectedInputObservation::Expected
        ) {
            return ExpectedInputObservation::Expected;
        }
        if attempt < READ_POLL_ATTEMPTS {
            thread::sleep(READ_POLL_INTERVAL);
        }
    }
    classify_expected_input(expected, &readings)
}

fn print_physical_recovery() {
    println!("RECOVERY: If the picture does not return, use the monitor's PHYSICAL OSD/input");
    println!("          button to select the original port connected to this PC (input A).");
    println!("          This test never guesses or probes another input value.");
}

/// Validate Option A's one-PC switching path without ever accepting a typed or guessed VCP value.
/// A and B are both learned from successful live reads before either value can be written.
fn guided_local_round_trip_test(driver: &mut DdcHiDriver) {
    println!();
    println!("=== Guided local A -> B -> A test (one PC) ===");
    println!("This validates that one PC can switch one monitor away and bring it back locally.");
    println!("It never scans, guesses, or probes input codes: A and B must both be observed live.");
    println!("Keep this terminal visible on another display/remote session if switching hides it.");
    print_physical_recovery();

    let Some(i) = pick(driver) else { return };
    let id = driver.monitors()[i].id.clone();

    println!();
    println!("STEP 1. Capture A from the input currently showing THIS PC.");
    if !confirm(&format!(
        "Is monitor #{i} currently showing this PC on the original input (A)?"
    )) {
        println!("Aborted before any write. Put the monitor on this PC and start again.");
        return;
    }
    let Some(input_a) = driver.try_read_input(&id) else {
        println!("Could not read VCP 0x60 while A was active. Aborted before any write.");
        println!("Enable DDC/CI in the monitor OSD, then retry.");
        return;
    };
    println!("   -> captured A from a live read: 0x{input_a:02X}");

    println!();
    println!("STEP 2. Use the monitor's PHYSICAL OSD/input button to switch from A to B.");
    println!("        Do not type an input code; the test will only observe VCP 0x60.");
    pause("Press Enter after the monitor is visibly showing the intended B source...");
    println!("   Polling for two matching live values distinct from A...");
    let input_b = match poll_for_distinct_input(driver, &id, input_a) {
        DistinctInputObservation::Distinct(value) => value,
        DistinctInputObservation::Unchanged => {
            println!(
                "VCP 0x60 never changed from A. B was not safely identified; no write issued."
            );
            println!("The panel may report a stale value or may have fallen back to A.");
            print_physical_recovery();
            return;
        }
        DistinctInputObservation::Inconsistent => {
            println!(
                "Distinct values were seen, but none appeared in two consecutive matching reads."
            );
            println!("B was not safely identified; no write issued.");
            print_physical_recovery();
            return;
        }
        DistinctInputObservation::Unreadable => {
            println!("VCP 0x60 was unreadable on B. B was not safely identified; no write issued.");
            print_physical_recovery();
            return;
        }
    };
    println!("   -> captured B from two matching distinct live reads: 0x{input_b:02X}");
    if !confirm("Is the monitor visibly showing the intended B source now?") {
        println!("Aborted: the observed value was not confirmed as B. No write issued.");
        print_physical_recovery();
        return;
    }

    println!();
    println!("STEP 3. Prove that this PC can recover B -> A while B is active.");
    println!("        The only value that will be written is captured A = 0x{input_a:02X}.");
    print_physical_recovery();
    if !confirm("Write captured A now to bring the monitor back to this PC?") {
        println!(
            "Aborted before any write. The monitor remains on B; recover with its physical OSD."
        );
        return;
    }
    let recovery_write = driver.write_input(&id, input_a);
    println!("   -> B -> A write returned {recovery_write:?}");
    println!("   -> Waiting ~2.5s for the monitor to settle...");
    thread::sleep(SWITCH_SETTLE);
    if !confirm("Did the monitor return to this PC on A?") {
        println!("RESULT: [FAIL] local B -> A recovery was not visually confirmed.");
        print_physical_recovery();
        println!("No result was persisted.");
        return;
    }
    if !matches!(recovery_write, DdcWriteResult::Ok) {
        println!(
            "RESULT: [INCONCLUSIVE] the picture returned, but the DDC write reported an error."
        );
        println!("No automatic round trip will run and no result was persisted.");
        return;
    }
    match poll_for_expected_input(driver, &id, input_a) {
        ExpectedInputObservation::Expected => {
            println!("   -> live read verified that the monitor is back on captured A");
        }
        ExpectedInputObservation::Other(value) => {
            println!(
                "Read-back remained at 0x{value:02X}, not captured A. Aborting before the full test."
            );
            print_physical_recovery();
            println!("No result was persisted.");
            return;
        }
        ExpectedInputObservation::Unreadable => {
            println!("A was unreadable after recovery. Aborting before the full test.");
            println!("No result was persisted.");
            return;
        }
    }

    println!();
    println!("STEP 4 (optional). Run the complete automatic A -> B -> A round trip.");
    println!("        The test will write captured B = 0x{input_b:02X}, wait ~3.5s, then");
    println!("        write captured A = 0x{input_a:02X} automatically. No other value is used.");
    print_physical_recovery();
    if !confirm("Run this complete automatic round trip now?") {
        println!("RESULT: [PARTIAL] B -> A recovery passed; automatic A -> B -> A was skipped.");
        println!("No passing result was persisted because the full sequence was not confirmed.");
        return;
    }

    println!("   -> writing captured B (0x{input_b:02X})...");
    let to_b_write = driver.write_input(&id, input_b);
    println!("   -> A -> B write returned {to_b_write:?}");
    println!("   -> waiting ~3.5s on B before automatic recovery...");
    thread::sleep(ROUND_TRIP_DWELL);
    println!("   -> writing captured A (0x{input_a:02X}) for recovery...");
    let to_a_write = driver.write_input(&id, input_a);
    println!("   -> B -> A write returned {to_a_write:?}");
    println!("   -> waiting ~2.5s for the monitor to settle...");
    thread::sleep(SWITCH_SETTLE);

    let saw_b = confirm("During that interval, did you see the monitor switch from A to B?");
    let returned_to_a = confirm("Did it then return automatically from B to this PC on A?");
    let final_observation = poll_for_expected_input(driver, &id, input_a);
    let writes_succeeded =
        matches!(to_b_write, DdcWriteResult::Ok) && matches!(to_a_write, DdcWriteResult::Ok);
    let readback_confirmed = matches!(final_observation, ExpectedInputObservation::Expected);

    println!();
    if writes_succeeded && saw_b && returned_to_a && readback_confirmed {
        println!("RESULT: [PASS] one-PC automatic A -> B -> A works on this monitor.");
        println!(
            "        Both values came from live reads and the full sequence was user-confirmed."
        );
    } else {
        println!("RESULT: [FAIL/INCONCLUSIVE] the full local round trip was not fully confirmed.");
        println!("        writes: A -> B = {to_b_write:?}, B -> A = {to_a_write:?}");
        println!("        saw B = {saw_b}, returned to A = {returned_to_a}");
        println!("        final read-back = {final_observation:?}");
        print_physical_recovery();
    }
    println!("No result was persisted; this developer diagnostic does not modify app setup.");
}

// ---- small console helpers --------------------------------------------------

fn pick(driver: &DdcHiDriver) -> Option<usize> {
    prompt(&format!(
        "monitor index (0-{}): ",
        driver.len().saturating_sub(1)
    ));
    match read_line().trim().parse::<usize>() {
        Ok(i) if i < driver.len() => Some(i),
        _ => {
            println!("bad index");
            None
        }
    }
}

fn read_line() -> String {
    let mut s = String::new();
    io::stdin().read_line(&mut s).ok();
    s
}

fn prompt(p: &str) {
    print!("{p}");
    io::stdout().flush().ok();
}

fn pause(p: &str) {
    prompt(p);
    read_line();
}

fn confirm(p: &str) -> bool {
    prompt(&format!("{p} (y/n): "));
    read_line().trim().to_lowercase().starts_with('y')
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let head: String = s.chars().take(n.saturating_sub(1)).collect();
        format!("{head}~")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_distinct_input, classify_expected_input, DistinctInputObservation,
        ExpectedInputObservation,
    };

    #[test]
    fn distinct_input_requires_matching_successful_values_different_from_a() {
        assert_eq!(
            classify_distinct_input(0x0f, &[None, Some(0x0f), None]),
            DistinctInputObservation::Unchanged
        );
        assert_eq!(
            classify_distinct_input(0x0f, &[None, Some(0x11), Some(0x11)]),
            DistinctInputObservation::Distinct(0x11)
        );
    }

    #[test]
    fn distinct_input_requires_two_consecutive_matching_reads() {
        assert_eq!(
            classify_distinct_input(0x0f, &[Some(0x11), None, Some(0x11)]),
            DistinctInputObservation::Inconsistent
        );
        assert_eq!(
            classify_distinct_input(0x0f, &[Some(0x11), Some(0x12)]),
            DistinctInputObservation::Inconsistent
        );
        assert_eq!(
            classify_distinct_input(0x0f, &[Some(0x11), Some(0x12), Some(0x12)]),
            DistinctInputObservation::Distinct(0x12)
        );
    }

    #[test]
    fn distinct_input_reports_fully_unreadable_poll() {
        assert_eq!(
            classify_distinct_input(0x0f, &[None, None]),
            DistinctInputObservation::Unreadable
        );
    }

    #[test]
    fn expected_input_requires_a_matching_live_read() {
        assert_eq!(
            classify_expected_input(0x0f, &[None, Some(0x11), Some(0x12)]),
            ExpectedInputObservation::Other(0x12)
        );
        assert_eq!(
            classify_expected_input(0x0f, &[Some(0x11), Some(0x0f)]),
            ExpectedInputObservation::Expected
        );
        assert_eq!(
            classify_expected_input(0x0f, &[None]),
            ExpectedInputObservation::Unreadable
        );
    }
}
