//! screen-hop M0 spike (Rust / ddc-hi): enumerate DDC/CI monitors, read/write VCP 0x60,
//! and run a guided "pull-to-self" test. Cross-platform (Windows/Linux/macOS). Throwaway
//! by design — see docs/PLAN-screen-hop.md, milestone M0.

use screenhop_core::MonitorDriver;
use screenhop_ddc::DdcHiDriver;
use screenhop_identity::group_by_id;
use std::io::{self, Write};
use std::{thread, time::Duration};

fn main() {
    println!("============================================================");
    println!(" screen-hop  -  M0 DDC/CI feasibility spike (Rust / ddc-hi)");
    println!(" Reads/writes monitor input source (VCP 0x60) over DDC/CI.");
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
        _ => interactive(&mut driver),
    }
}

fn print_table(driver: &mut DdcHiDriver) {
    let monitors = driver.monitors();
    println!();
    println!(
        "{:<3} {:<26} {:<7} {:<14} Backend",
        "#", "Monitor", "Input", "MonitorId"
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
    println!("'#' = menu index; 'Input' = current source; 'MonitorId' = stable cross-PC id.");
}

fn interactive(driver: &mut DdcHiDriver) {
    loop {
        print_table(driver);
        println!(
            "[1] Read input   [2] Set input (DANGER)   [3] Guided pull-to-self test   [0] Exit"
        );
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
            "2" => cmd_set(driver),
            "3" => guided_pull_test(driver),
            "0" | "" => break,
            _ => println!("unknown choice"),
        }
        println!();
    }
}

fn cmd_set(driver: &mut DdcHiDriver) {
    let Some(i) = pick(driver) else { return };
    let id = driver.monitors()[i].id.clone();
    prompt("value to write (hex, e.g. 0F): ");
    let raw = read_line();
    let hex = raw.trim().trim_start_matches("0x").trim_start_matches("0X");
    let Ok(value) = u32::from_str_radix(hex, 16) else {
        println!("bad hex value");
        return;
    };
    println!(
        "   NOTE: this is the raw M0 spike — it writes the value you typed with NO soft-brick"
    );
    println!("         guard (no confirmed-set / blocked-value check). That is intentional for");
    println!("         hardware probing; the real agent only ever writes self-calibrated values.");
    if !confirm(&format!(
        "Set monitor #{i} input to 0x{value:02X}? This changes what it displays."
    )) {
        println!("aborted");
        return;
    }
    println!("write returned {:?}", driver.write_input(&id, value));
}

fn guided_pull_test(driver: &mut DdcHiDriver) {
    println!();
    println!("=== Guided pull-to-self test ===");
    println!("Verifies THIS machine can switch a monitor TO ITSELF over DDC/CI while it is NOT the shown input.");
    let Some(i) = pick(driver) else { return };
    let id = driver.monitors()[i].id.clone();

    // Capture panel identity up front so we can emit a formal verdict row at the end.
    let (mfr, model, mid) = {
        let m = &driver.monitors()[i];
        (
            m.manufacturer.clone().unwrap_or_default(),
            m.model.clone().unwrap_or_else(|| "Generic Monitor".into()),
            m.monitor_id().unwrap_or_else(|| "(no identity)".into()),
        )
    };

    println!();
    println!("STEP 1.  Make sure monitor #{i} is CURRENTLY SHOWING THIS machine.");
    pause("Press Enter when this machine is shown on it...");
    let Some(my_value) = driver.try_read_input(&id) else {
        println!("Could not read 0x60 - DDC/CI may be disabled in the monitor OSD. Aborting.");
        return;
    };
    println!("   -> This machine's input value on this monitor is 0x{my_value:02X}  (recorded)");

    println!();
    println!("STEP 2.  Use the monitor's PHYSICAL input button to switch it to ANOTHER machine,");
    println!("         so THIS machine is no longer shown on it.");
    pause("Press Enter once the monitor is showing the OTHER machine...");

    println!();
    println!("STEP 3.  Attempting pull-to-self: writing 0x60 = 0x{my_value:02X} from THIS machine (not currently shown)...");
    let result = driver.write_input(&id, my_value);
    println!("   -> write returned {result:?}");
    println!("   -> Waiting ~2.5s for the monitor to settle...");
    thread::sleep(Duration::from_millis(2500));

    println!();
    let switched = confirm("Did the monitor switch back to THIS machine?");
    println!();
    if switched {
        println!("RESULT: [PASS] pull-to-self WORKS on this monitor.");
        println!(
            "        screen-hop's primary path is viable for this panel. (M0 = go for this panel.)"
        );
    } else {
        println!("RESULT: [FAIL] pull-to-self did NOT work on this monitor.");
        println!("        This panel likely needs the push-release fallback, or only honors DDC over its active input.");
    }

    record_verdict(
        &mfr,
        &model,
        &mid,
        my_value,
        switched,
        &format!("{result:?}"),
    );
}

/// Offer to record a formal M0 verdict row, print it, and append it to the verdict log
/// (docs/hardware/pull-to-self-verdicts.md) when that file can be located from the CWD.
fn record_verdict(mfr: &str, model: &str, mid: &str, value: u32, pass: bool, write_result: &str) {
    println!();
    if !confirm("Record this as a formal M0 verdict row?") {
        return;
    }
    prompt("Rig label (e.g. 'PC-A \u{b7} Win11 \u{b7} RTX 3060'): ");
    let rig = read_line().trim().to_string();
    prompt("Cable / port (e.g. 'DP', 'HDMI', 'USB-C'): ");
    let cable = read_line().trim().to_string();
    prompt("Date (YYYY-MM-DD), Enter to leave blank: ");
    let date = read_line().trim().to_string();
    prompt("Extra notes (Enter for none): ");
    let notes = read_line().trim().to_string();

    let date = if date.is_empty() {
        "____-__-__".into()
    } else {
        date
    };
    let rig = if rig.is_empty() {
        "(unspecified)".into()
    } else {
        rig
    };
    let cable = if cable.is_empty() {
        "\u{2014}".into()
    } else {
        cable
    };
    let result = if pass { "PASS" } else { "FAIL" };
    let mfr_model = format!("{} \u{b7} {}", mfr.trim(), model.trim());
    let note_field = if notes.is_empty() {
        format!("write={write_result}")
    } else {
        format!("write={write_result}; {notes}")
    };
    let row = format!(
        "| {date} | {rig} | {mfr_model} | {mid} | {cable} | 0x{value:02X} | {result} | ~2.5s | {note_field} |"
    );

    println!();
    println!("Verdict row (for docs/hardware/pull-to-self-verdicts.md):");
    println!();
    println!("{row}");
    println!();
    match append_verdict_row(&row) {
        Some(path) => println!("   -> appended to {path}"),
        None => {
            println!("   -> could not locate the verdict log from here; paste the row in manually.")
        }
    }
}

/// Insert `row` directly below the `<!-- VERDICT-ROWS:` marker in the verdict log, searching a
/// few CWD-relative candidate paths. Returns the path written, or None if the file wasn't found.
fn append_verdict_row(row: &str) -> Option<String> {
    const MARKER: &str = "<!-- VERDICT-ROWS:";
    const CANDIDATES: [&str; 3] = [
        "docs/hardware/pull-to-self-verdicts.md",
        "../docs/hardware/pull-to-self-verdicts.md",
        "../../docs/hardware/pull-to-self-verdicts.md",
    ];
    for path in CANDIDATES {
        let p = std::path::Path::new(path);
        if !p.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(p) else {
            continue;
        };
        let Some(insert_at) = content
            .find(MARKER)
            .and_then(|start| content[start..].find('\n').map(|off| start + off + 1))
        else {
            continue;
        };
        let mut updated = String::with_capacity(content.len() + row.len() + 1);
        updated.push_str(&content[..insert_at]);
        updated.push_str(row);
        updated.push('\n');
        updated.push_str(&content[insert_at..]);
        if std::fs::write(p, updated).is_ok() {
            return Some(p.display().to_string());
        }
    }
    None
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
