//! screen-hop UI binary (milestone M5).
//!
//! Three modes:
//! - default: design preview window with a dev switcher between surfaces.
//! - `--shot <png>`: render one surface to a PNG and exit (visual diffing against the design).
//! - `--live`: enumerate this machine's real monitors and drive the tray through the production
//!   Controller -> bind path (read-only for now; the mesh Node/discovery/actuation loop is the next
//!   step, see docs/REMAINING-CHECKLIST.md).

use std::rc::Rc;
use std::sync::{Arc, Mutex};

use screenhop_app::MeshState;
use screenhop_ui::{bind, AppWindow, Controller};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() -> Result<(), slint::PlatformError> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--live") {
        return run_live(&args);
    }
    run_preview(&args)
}

/// Live mode: a real, read-only view of this machine's DDC/CI monitors through the production
/// Controller -> bind path. Switching shows the honest in-flight state; the mesh routing +
/// per-panel calibration that make a switch actually actuate are the documented next step.
fn run_live(_args: &[String]) -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;
    app.set_screen(0); // tray flyout

    // 1. Enumerate this machine's DDC/CI monitors (read-only).
    let driver = screenhop_ddc::DdcHiDriver::enumerate();
    let mut monitor_ids: Vec<String> = Vec::new();
    let mut labels: Vec<(String, String)> = Vec::new();
    for m in driver.monitors() {
        let id = m.monitor_id().unwrap_or_else(|| m.id.clone());
        let mfr = m.manufacturer.clone().unwrap_or_default();
        let model = m.model.clone().unwrap_or_else(|| "Monitor".to_string());
        let label = format!("{mfr} {model}").trim().to_string();
        labels.push((
            id.clone(),
            if label.is_empty() { id.clone() } else { label },
        ));
        monitor_ids.push(id);
    }
    if monitor_ids.is_empty() {
        eprintln!(
            "screen-hop --live: no DDC/CI monitors found. Enable DDC/CI in the monitor OSD; on \
             Linux load i2c-dev and grant /dev/i2c-* access."
        );
    }

    // 2. Backend state + controller. The full mesh Node/discovery/actuation loop is the next step
    //    (docs/REMAINING-CHECKLIST.md); for now this is the real Controller over a local state.
    let state = Arc::new(Mutex::new(MeshState::default()));
    let me = "this-pc".to_string();
    let mut controller = Controller::new(me.clone(), Arc::clone(&state), 10_000);
    for (id, label) in &labels {
        controller.set_label(id, label);
    }

    let peer_ids = vec![me.clone()];
    let peer_labels = vec!["This PC".to_string()];

    // 3. Build the tray view models and bind them to the window.
    let binding = bind::build_tray(&controller, &monitor_ids, &peer_ids, &peer_labels);
    let monitors_vm = Rc::new(VecModel::from(binding.monitors));
    app.set_monitors(ModelRc::from(monitors_vm.clone()));
    app.set_peers(ModelRc::from(Rc::new(VecModel::from(binding.peers))));
    app.set_online_count(peer_ids.len() as i32);

    // 4. Wire callbacks. Switch shows the honest in-flight state; real mesh routing + actuation
    //    (which need a paired peer and per-panel calibration) are the documented next step.
    let monitors_for_switch = monitors_vm.clone();
    app.on_switch(move |mi, _pi| {
        let i = mi as usize;
        if let Some(mut row) = monitors_for_switch.row_data(i) {
            row.switching = true;
            monitors_for_switch.set_row_data(i, row);
        }
        eprintln!(
            "screen-hop --live: switch requested for monitor #{mi}; mesh routing + calibration are \
             not wired yet (see docs/REMAINING-CHECKLIST.md)."
        );
    });
    app.on_apply_preset(|pi| {
        eprintln!("screen-hop --live: preset #{pi} requested; preset routing not wired yet.");
    });

    app.run()
}

/// Design-preview / snapshot mode (the original behaviour).
fn run_preview(args: &[String]) -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;

    if args.iter().any(|a| a == "--dark") {
        app.set_dark(true);
    }
    if let Some(s) = arg_value(args, "--screen") {
        app.set_screen(match s.as_str() {
            "wizard" => 1,
            "dialog" => 2,
            "deskmap" => 3,
            "settings" => 4,
            _ => 0,
        });
    }
    if let Some(s) = arg_value(args, "--step") {
        if let Ok(n) = s.parse::<i32>() {
            app.set_wizard_step(n);
        }
    }
    if let Some(s) = arg_value(args, "--dialog") {
        if let Ok(n) = s.parse::<i32>() {
            app.set_dialog(n);
        }
    }

    if let Some(path) = arg_value(args, "--shot") {
        app.set_dev_chrome(false);
        // Settle delay before snapshotting (lets fonts/layout/animations land). Override with
        // `--delay <ms>` for slower machines / heavier surfaces.
        let delay_ms = arg_value(args, "--delay")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(600);
        let weak = app.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(delay_ms), move || {
            if let Some(app) = weak.upgrade() {
                let ok = match app.window().take_snapshot() {
                    Ok(buf) => match image::save_buffer(
                        &path,
                        buf.as_bytes(),
                        buf.width(),
                        buf.height(),
                        image::ExtendedColorType::Rgba8,
                    ) {
                        Ok(()) => true,
                        Err(e) => {
                            eprintln!("save error: {e}");
                            false
                        }
                    },
                    Err(e) => {
                        eprintln!("snapshot error: {e}");
                        false
                    }
                };
                if !ok {
                    let _ = slint::quit_event_loop();
                    // Non-zero exit so CI / design-diff scripts notice a failed render.
                    std::process::exit(1);
                }
            }
            let _ = slint::quit_event_loop();
        });
    }

    app.run()
}
