//! screen-hop UI binary (milestone M5).
//!
//! Three modes:
//! - default: design preview window with a dev switcher between surfaces.
//! - `--shot <png>`: render one surface to a PNG and exit (visual diffing against the design).
//! - `--live`: the real agent — enumerate this machine's monitors, join the LAN mesh, and route
//!   tray clicks into actual DDC switches. Verified on a 2-PC rig (see docs/REMAINING-CHECKLIST.md).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, TcpListener};
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use screenhop_app::discovery::{ManualHosts, MdnsDiscovery};
use screenhop_app::{
    persist, reconcile_reads, ActuatorRequest, ChannelActuator, LiveAgent, LocalActuator,
    MeshState, Node, UiIntent,
};
use screenhop_core::{MonitorDriver, RealClock, RealDelayer, SwitchExecutor};
use screenhop_ddc::{DdcHiDriver, MonitorInfo};
use screenhop_identity::CalibrationStore;
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
    if args.iter().any(|a| a == "--calibrate") {
        if let Err(e) = run_calibrate() {
            eprintln!("screen-hop --calibrate: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if args.iter().any(|a| a == "--monitors") {
        run_monitors();
        return Ok(());
    }
    if args.iter().any(|a| a == "--live") {
        return run_live();
    }
    run_preview(&args)
}

/// Diagnostic: dump every display handle this machine enumerates, with its full identity fields and
/// whether DDC reads work — so cross-PC identity mismatches (different GPU backends exposing
/// different EDID) can be diagnosed by comparing two machines' output.
fn run_monitors() {
    let mut driver = DdcHiDriver::enumerate();
    let infos = driver.monitors();
    println!("{} display handle(s) on this PC:", infos.len());
    for (i, m) in infos.iter().enumerate() {
        let input = read_input_retry(&mut driver, &m.id);
        println!();
        println!("#{i}  backend = {}", m.backend);
        println!("    local id     : {}", m.id);
        println!("    model        : {:?}", m.model);
        println!("    manufacturer : {:?}", m.manufacturer);
        println!("    monitor_id   : {:?}", m.monitor_id());
        match &m.fingerprint {
            Some(fp) => {
                let sha = fp.raw_sha256.as_deref().map(|s| &s[..8.min(s.len())]);
                println!(
                    "    edid         : pnp={} product=0x{:04X} numeric_serial={} ascii_serial={:?} raw_sha(8)={:?}",
                    fp.pnp_manufacturer, fp.product_code, fp.numeric_serial, fp.ascii_serial, sha
                );
            }
            None => println!("    edid         : <none exposed by this backend>"),
        }
        match input {
            Some(v) => println!("    reads 0x60   : yes (0x{v:02X})"),
            None => println!("    reads 0x60   : NO"),
        }
    }
    println!();
    println!(
        "If a monitor here has no identity (e.g. it's behind a USB-C hub/dock) but another PC"
    );
    println!("sees its real id, force them to match by editing config.json in the config dir:");
    println!("  {{ \"monitor_aliases\": {{ \"<local id on THIS pc>\": \"<shared id>\" }} }}");
}

/// Calibration (one-shot CLI). With THIS PC currently displayed on the monitors you want to use,
/// read each panel's live `0x60` and record it as this peer's pull-to-self value, then persist.
/// Re-run any time the wiring changes. This is the headless equivalent of the wizard's calibrate
/// step (the GUI wizard wiring is still pending — see docs/REMAINING-CHECKLIST.md).
fn run_calibrate() -> std::io::Result<()> {
    let config_dir = persist::ensure_config_dir()?;
    let identity = persist::load_or_create_identity(&config_dir)?;
    let me = identity.peer_id();
    let cfg = persist::load_config(&config_dir)?;
    let mut cal = persist::load_calibration(&config_dir)?;

    let mut driver = DdcHiDriver::enumerate();
    let monitors = identified_monitors(&driver, &cfg.monitor_aliases);
    driver.remap_ids(|m| effective_id(m, &cfg.monitor_aliases));
    if monitors.is_empty() {
        println!(
            "No identifiable DDC/CI monitors found. Enable DDC/CI in the OSD; for a monitor behind \
             a hub/dock that hides its identity, add a monitor_aliases entry (see --monitors)."
        );
        return Ok(());
    }
    println!("Calibrating as peer {me} (make sure THIS PC is the shown input on each panel):");
    for (id, label) in &monitors {
        match read_input_retry(&mut driver, id) {
            Some(v) => {
                // Guard against the classic trap: calibrating while the monitor is showing ANOTHER
                // PC records that PC's input as "ours". If a saved value changes, flag it loudly —
                // a legit re-cable changes it too, but usually it means the wrong PC was shown.
                if let Some(prev) = cal.confirmed_value(&me, id) {
                    if prev != v {
                        println!(
                            "  [warn] {label}: value changed 0x{prev:02X} -> 0x{v:02X}. If you did \
                             NOT re-cable, make sure THIS PC is the one shown on it — you may be \
                             saving another PC's input by mistake."
                        );
                    }
                }
                cal.record(&me, id, v);
                println!("  [ok]   {label} ({id}) = 0x{v:02X}");
            }
            None => println!(
                "  [skip] {label} ({id}) — DDC/CI unreadable after retries (is this PC the shown \
                 input, and is DDC/CI enabled in the OSD?)"
            ),
        }
    }
    persist::save_calibration(&config_dir, &cal)?;
    println!(
        "Saved calibration to {}",
        config_dir.join("calibration.json").display()
    );
    Ok(())
}

/// The id used for a monitor everywhere except the raw OS handle: a user **alias** wins (for a panel
/// whose EDID identity is hidden on this PC, e.g. behind a USB-C hub), else the stable EDID id, else
/// the provisional handle id. Re-keying the driver to this id makes the mesh, calibration, and the
/// DDC handle all agree on one id.
fn effective_id(m: &MonitorInfo, aliases: &HashMap<String, String>) -> String {
    if let Some(target) = aliases.get(&m.id) {
        return target.clone();
    }
    m.monitor_id().unwrap_or_else(|| m.id.clone())
}

/// The de-duplicated list of `(effective_id, label)` to show + drive: only monitors with a real
/// cross-PC identity (an EDID `monitor_id` or a user alias), collapsed by effective id so the same
/// physical panel seen via multiple GPU backends (WinApi + Nvapi) is one row. Anonymous handles
/// (no EDID, no alias) are omitted — they can't be correlated across PCs; alias one (see
/// `--monitors`) to include it. Call this BEFORE `remap_ids` (it reads the original handle ids).
fn identified_monitors(
    driver: &DdcHiDriver,
    aliases: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for m in driver.monitors() {
        if !(aliases.contains_key(&m.id) || m.monitor_id().is_some()) {
            continue; // anonymous handle — no stable cross-PC identity
        }
        let eid = effective_id(&m, aliases);
        if !seen.insert(eid.clone()) {
            continue; // same physical monitor via another backend
        }
        let mfr = m.manufacturer.clone().unwrap_or_default();
        let model = m.model.clone().unwrap_or_else(|| "Monitor".to_string());
        let label = format!("{mfr} {model}").trim().to_string();
        let label = if label.is_empty() { eid.clone() } else { label };
        out.push((eid, label));
    }
    out
}

/// Read a panel's input, retrying a few times — DDC reads are flaky on some GPU backends (the
/// first attempt often fails even when the panel is fine), so a one-shot read drops good panels.
fn read_input_retry(driver: &mut DdcHiDriver, monitor_id: &str) -> Option<u32> {
    for attempt in 0..8 {
        if let Some(v) = driver.try_read_input(monitor_id) {
            return Some(v);
        }
        if attempt < 7 {
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    None
}

/// Periodic reconcile sweep (the cross-platform half of the OS trigger; the Windows
/// `WM_DISPLAYCHANGE` hook is a documented follow-up). Reads each panel's live `0x60` THROUGH the
/// actuator thread (so the driver stays on its own thread and no lock is held during the slow read),
/// then folds the results into ownership under a brief lock.
fn reconcile_loop(
    reads_tx: mpsc::Sender<ActuatorRequest>,
    state: Arc<Mutex<MeshState>>,
    calibration: CalibrationStore,
    me: String,
    monitor_ids: Vec<String>,
) {
    if monitor_ids.is_empty() {
        return;
    }
    loop {
        std::thread::sleep(Duration::from_secs(4));
        let mut reads: Vec<(String, Option<u32>)> = Vec::with_capacity(monitor_ids.len());
        for id in &monitor_ids {
            let (reply, rx) = mpsc::channel();
            if reads_tx
                .send(ActuatorRequest::Read {
                    monitor_id: id.clone(),
                    reply,
                })
                .is_err()
            {
                return; // actuator thread gone
            }
            let val = rx.recv_timeout(Duration::from_secs(20)).ok().flatten();
            reads.push((id.clone(), val));
        }
        let now = wall_ms();
        let mut online: HashSet<String> = {
            let st = state.lock().unwrap_or_else(|e| e.into_inner());
            st.peers.online(now, 20_000).into_iter().collect()
        };
        online.insert(me.clone());
        let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
        reconcile_reads(&mut st.ownership, &calibration, &online, &reads, now);
    }
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
        let aliases = cfg.monitor_aliases.clone();
        let mut quirks = QuirksDb::with_shipped();
        let _ = quirks.load_local(&config_dir.join("quirks-local.json"));
        std::thread::spawn(move || {
            let mut driver = DdcHiDriver::enumerate();
            let mons = identified_monitors(&driver, &aliases);
            driver.remap_ids(|m| effective_id(m, &aliases));
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
    let recon_tx = req_tx.clone(); // a second handle to the actuator thread, for reconcile reads
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
    // Announce a friendly name (the machine's hostname, or the configured name) instead of the
    // 64-char peer id, so peers show up readably in each other's tray.
    let agent_name = {
        let configured = cfg.name.trim();
        if !configured.is_empty() && configured != "screen-hop" {
            configured.to_string()
        } else {
            std::env::var("COMPUTERNAME")
                .ok()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "screen-hop".to_string())
        }
    };
    let agent = LiveAgent::new(node, agent_name, self_addr, manual, mdns);
    let agent_state = agent.state();
    std::thread::spawn(move || agent.run(listener, intent_rx));

    // Periodic reconcile sweep (the cross-platform half of the OS trigger).
    {
        let state = Arc::clone(&agent_state);
        let cal = calibration.clone();
        let me = me.clone();
        let mons = monitor_ids.clone();
        std::thread::spawn(move || reconcile_loop(recon_tx, state, cal, me, mons));
    }

    // --- controller + UI binding ----------------------------------------------------------------
    let mut controller = Controller::new(me.clone(), Arc::clone(&agent_state), 20_000);
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
                eprintln!("screen-hop: click row={mi} seg={pi} -> no monitor/peer at those indices");
                return;
            };
            eprintln!("screen-hop: click row={mi} seg={pi} -> switch monitor {monitor_id} to peer {target}");
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
                    // The tray segment is narrow; show a short label (friendly name, else a short
                    // id prefix) so it fits and the right segment is clickable.
                    let raw = if pv.name.is_empty() {
                        pv.id.clone()
                    } else {
                        pv.name.clone()
                    };
                    peer_labels.push(raw.chars().take(8).collect::<String>());
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
    let state = Arc::new(Mutex::new(MeshState::default()));
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
