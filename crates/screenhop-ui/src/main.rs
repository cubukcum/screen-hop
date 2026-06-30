//! screen-hop UI binary (milestone M5).
//!
//! Three modes:
//! - default: design preview window with a dev switcher between surfaces.
//! - `--shot <png>`: render one surface to a PNG and exit (visual diffing against the design).
//! - `--live`: the real agent — enumerate this machine's monitors, join the LAN mesh, and route
//!   tray clicks into actual DDC switches. Verified on a 2-PC rig (see docs/REMAINING-CHECKLIST.md).

use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{SocketAddr, TcpListener};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use screenhop_app::discovery::{ManualHosts, MdnsDiscovery};
use screenhop_app::{
    persist, ActuatorRequest, ChannelActuator, LiveAgent, LocalActuator, Node, UiIntent,
};
use screenhop_core::{MonitorDriver, RealClock, RealDelayer, SwitchExecutor};
use screenhop_ddc::DdcHiDriver;
use screenhop_net::PeerIdentity;
use screenhop_quirks::QuirksDb;
use screenhop_ui::{bind, AppWindow, Controller, MonitorRow, Peer};
use slint::{ComponentHandle, Model, ModelRc, Timer, TimerMode, VecModel};

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn wall_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn main() -> Result<(), slint::PlatformError> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--live") {
        return run_live();
    }
    run_preview(&args)
}

/// Live mode: the real agent.
fn run_live() -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;
    app.set_screen(0); // tray flyout

    let config_dir = match persist::ensure_config_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("screen-hop --live: cannot open config dir: {e}");
            return app.run();
        }
    };
    let identity = persist::load_or_create_identity(&config_dir).unwrap_or_else(|e| {
        eprintln!("screen-hop --live: identity error: {e}; using an ephemeral identity");
        PeerIdentity::generate()
    });
    let cfg = persist::load_config(&config_dir).unwrap_or_default();
    let secret = persist::load_secret(&config_dir).ok().flatten();
    let calibration = persist::load_calibration(&config_dir).unwrap_or_default();
    let mut labels = persist::load_labels(&config_dir).unwrap_or_default();

    // --- actuator thread: owns the non-Send DdcHiDriver, services Switch/Read requests ----------
    let (req_tx, req_rx) = mpsc::channel::<ActuatorRequest>();
    let (mon_tx, mon_rx) = mpsc::channel::<Vec<(String, String)>>();
    {
        let peer_id = identity.peer_id();
        let calibration = calibration.clone();
        let mut quirks = QuirksDb::with_shipped();
        let _ = quirks.load_local(&config_dir.join("quirks-local.json"));
        std::thread::spawn(move || {
            let driver = DdcHiDriver::enumerate();
            let mons: Vec<(String, String)> = driver
                .monitors()
                .iter()
                .map(|m| {
                    let id = m.monitor_id().unwrap_or_else(|| m.id.clone());
                    let mfr = m.manufacturer.clone().unwrap_or_default();
                    let model = m.model.clone().unwrap_or_else(|| "Monitor".to_string());
                    let label = format!("{mfr} {model}").trim().to_string();
                    let label = if label.is_empty() { id.clone() } else { label };
                    (id, label)
                })
                .collect();
            let _ = mon_tx.send(mons);

            let exec = SwitchExecutor::new(driver, RealDelayer, RealClock::default());
            let mut actuator = LocalActuator::new(peer_id, exec, quirks, calibration);
            for req in req_rx {
                match req {
                    ActuatorRequest::Switch { monitor_id, reply } => {
                        let _ = reply.send(actuator.perform_switch(&monitor_id));
                    }
                    ActuatorRequest::Read { monitor_id, reply } => {
                        let _ = reply.send(actuator.driver_mut().try_read_input(&monitor_id));
                    }
                }
            }
        });
    }
    let monitors = mon_rx.recv().unwrap_or_default();
    let monitor_ids: Vec<String> = monitors.iter().map(|(id, _)| id.clone()).collect();
    for (id, label) in &monitors {
        labels.entry(id.clone()).or_insert_with(|| label.clone());
    }
    if monitor_ids.is_empty() {
        eprintln!("screen-hop --live: no DDC/CI monitors found (enable DDC/CI in the OSD).");
    }

    // A mesh secret is required to form the mesh. Without one, show monitors read-only.
    let Some(secret) = secret else {
        eprintln!(
            "screen-hop --live: no mesh secret. Showing monitors read-only. Write a shared secret \
             to {}\\mesh-secret on each PC to enable switching.",
            config_dir.display()
        );
        return run_readonly(app, identity.peer_id(), &monitor_ids, &labels);
    };

    // --- mesh node + agent ----------------------------------------------------------------------
    let node = Node::new(identity, &secret)
        .with_actuator(ChannelActuator::new(req_tx))
        .with_pin_store(persist::pins_path(&config_dir));
    let me = node.peer_id();

    let listener = match TcpListener::bind(("0.0.0.0", cfg.port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("screen-hop --live: cannot bind port {}: {e}", cfg.port);
            return run_readonly(app, me, &monitor_ids, &labels);
        }
    };
    let self_addr: SocketAddr = ([127, 0, 0, 1], cfg.port).into();

    let mut manual = ManualHosts::new();
    for h in &cfg.manual_hosts {
        manual.add(h);
    }
    let mdns = MdnsDiscovery::start().ok();

    let (intent_tx, intent_rx) = mpsc::channel::<UiIntent>();
    let agent = LiveAgent::new(node, self_addr, manual, mdns);
    let agent_state = agent.state();
    std::thread::spawn(move || agent.run(listener, intent_rx));

    // --- controller + UI binding ----------------------------------------------------------------
    let mut controller = Controller::new(me.clone(), agent_state, 20_000);
    for (id, label) in &labels {
        controller.set_label(id, label);
    }
    let controller = Rc::new(controller);
    let monitor_ids = Rc::new(monitor_ids);

    // Persistent models the Timer updates in place (so on_switch can flip a row instantly).
    let monitors_vm: Rc<VecModel<MonitorRow>> = Rc::new(VecModel::default());
    let peers_vm: Rc<VecModel<Peer>> = Rc::new(VecModel::default());
    app.set_monitors(ModelRc::from(monitors_vm.clone()));
    app.set_peers(ModelRc::from(peers_vm.clone()));

    // Shared binding (for on_switch index→id resolution) + pending switches (monitor_id → target).
    let binding = Rc::new(RefCell::new(bind::build_tray(
        &controller,
        &monitor_ids,
        std::slice::from_ref(&me),
        &["This PC".to_string()],
    )));
    let pending: Rc<RefCell<HashMap<String, (String, Instant)>>> =
        Rc::new(RefCell::new(HashMap::new()));

    // on_switch: enqueue the intent, mark the monitor pending, flip the row to in-flight now.
    {
        let binding = Rc::clone(&binding);
        let pending = Rc::clone(&pending);
        let monitors_vm = monitors_vm.clone();
        app.on_switch(move |mi, pi| {
            let Some((monitor_id, target)) = binding.borrow().resolve_switch(mi, pi) else {
                return;
            };
            let _ = intent_tx.send(UiIntent::Switch {
                monitor_id: monitor_id.clone(),
                target_peer_id: target.clone(),
            });
            pending
                .borrow_mut()
                .insert(monitor_id, (target, Instant::now()));
            if let Some(mut row) = monitors_vm.row_data(mi as usize) {
                row.switching = true;
                monitors_vm.set_row_data(mi as usize, row);
            }
        });
    }

    // Refresh timer: rebuild view models from live mesh state ~1.4×/s.
    let timer = Timer::default();
    {
        let controller = Rc::clone(&controller);
        let monitor_ids = Rc::clone(&monitor_ids);
        let binding = Rc::clone(&binding);
        let pending = Rc::clone(&pending);
        let monitors_vm = monitors_vm.clone();
        let peers_vm = peers_vm.clone();
        let app_weak = app.as_weak();
        let me = me.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(700), move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let now = wall_ms();

            // peers = this PC first, then every known peer.
            let mut peer_ids = vec![me.clone()];
            let mut peer_labels = vec!["This PC".to_string()];
            for pv in controller.peer_views(now) {
                if pv.id != me {
                    peer_labels.push(if pv.name.is_empty() {
                        pv.id.clone()
                    } else {
                        pv.name.clone()
                    });
                    peer_ids.push(pv.id);
                }
            }

            let b = bind::build_tray(&controller, &monitor_ids, &peer_ids, &peer_labels);

            // Expire stale pending entries (target reached, or timed out), then mark in-flight rows.
            {
                let mut p = pending.borrow_mut();
                p.retain(|mon, (target, since)| {
                    let arrived = b
                        .monitors
                        .iter()
                        .zip(b.monitor_ids.iter())
                        .find(|(_, id)| *id == mon)
                        .map(|(row, _)| {
                            row.active >= 0 && peer_ids.get(row.active as usize) == Some(target)
                        })
                        .unwrap_or(false);
                    !arrived && since.elapsed() < Duration::from_secs(15)
                });
            }
            let mut rows: Vec<_> = b.monitors.clone();
            {
                let p = pending.borrow();
                for (row, id) in rows.iter_mut().zip(b.monitor_ids.iter()) {
                    if p.contains_key(id) {
                        row.switching = true;
                    }
                }
            }

            monitors_vm.set_vec(rows);
            peers_vm.set_vec(b.peers.clone());
            app.set_online_count(peer_ids.len() as i32);
            app.set_degraded(controller.is_degraded(now));
            *binding.borrow_mut() = b;
        });
    }

    app.run()
}

/// Read-only fallback: show the enumerated monitors with no mesh (no secret / bind failure).
fn run_readonly(
    app: AppWindow,
    me: String,
    monitor_ids: &[String],
    labels: &HashMap<String, String>,
) -> Result<(), slint::PlatformError> {
    use std::sync::{Arc, Mutex};
    let state = Arc::new(Mutex::new(screenhop_app::MeshState::default()));
    let mut controller = Controller::new(me, state, 20_000);
    for (id, label) in labels {
        controller.set_label(id, label);
    }
    let b = bind::build_tray(
        &controller,
        monitor_ids,
        &["this-pc".to_string()],
        &["This PC".to_string()],
    );
    app.set_monitors(ModelRc::from(Rc::new(VecModel::from(b.monitors))));
    app.set_peers(ModelRc::from(Rc::new(VecModel::from(b.peers))));
    app.set_screen(0);
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
