//! screen-hop UI binary (milestone M5).
//!
//! Three modes:
//! - default: design preview window with a dev switcher between surfaces.
//! - `--shot <png>`: render one surface to a PNG and exit (visual diffing against the design).
//! - `--live`: the real agent — enumerate this machine's monitors, join the LAN mesh, and route
//!   tray clicks into actual DDC switches. Verified on a 2-PC rig (see docs/REMAINING-CHECKLIST.md).

// GUI subsystem: without this, Windows allocates a console window for the app (Rust's default is
// the console subsystem), and closing that stray window kills the whole process. CLI modes stay
// usable because main() re-attaches to the parent console before printing anything.
#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::net::{SocketAddr, TcpListener};
use std::rc::Rc;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use screenhop_app::discovery::{ManualHosts, MdnsDiscovery};
use screenhop_app::{
    persist, reconcile_reads, ActuationReport, ActuatorRequest, ChannelActuator, LiveAgent,
    LocalActuator, MeshState, Node, UiIntent,
};
use screenhop_core::{MonitorDriver, RealClock, RealDelayer, SwitchExecutor, SwitchOutcome};
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

/// A `windows_subsystem = "windows"` exe gets no console at all, which would make the CLI modes
/// (`--monitors`, `--calibrate`, `--shot`) silent when run from a terminal. Attach to the parent
/// process's console so their output shows up again. Launched from the Start menu or autostart
/// there is no parent console and the call fails as a harmless no-op; handles that the shell
/// redirected explicitly (pipes, files) are inherited as usual and unaffected by this.
fn attach_parent_console() {
    #[cfg(windows)]
    unsafe {
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(process_id: u32) -> i32;
        }
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        AttachConsole(ATTACH_PARENT_PROCESS);
    }
}

fn main() -> Result<(), slint::PlatformError> {
    attach_parent_console();
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
        // The first-run wizard saves a mesh secret then asks us to relaunch, so the normal live
        // path below picks the secret up and brings the mesh online (no deferred-startup gymnastics).
        loop {
            match run_live()? {
                LiveExit::Quit => return Ok(()),
                LiveExit::Relaunch => continue,
            }
        }
    }
    run_preview(&args)
}

/// How a `--live` session ended: the user quit, or they just paired via the first-run wizard and we
/// should relaunch so the mesh comes up with the freshly-saved secret.
enum LiveExit {
    Quit,
    Relaunch,
}

/// A malformed/unreadable config must never silently fall back to write-enabled defaults. Keep the
/// node useful for discovery and remote control, but make local DDC actuation read-only until the
/// operator fixes `config.json`.
fn load_live_config(config_dir: &std::path::Path) -> persist::AgentConfig {
    match persist::load_config(config_dir) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!(
                "screen-hop --live: cannot read config.json: {e}; local actuation is disabled"
            );
            persist::AgentConfig {
                can_actuate: false,
                ..Default::default()
            }
        }
    }
}

fn target_can_actuate(capable_peer_ids: &HashSet<String>, target_peer_id: &str) -> bool {
    capable_peer_ids.contains(target_peer_id)
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

/// A compact, tray-friendly label for a peer segment. The segment is narrow, so prefer the peer's
/// announced friendly name (hostname or configured `name`), truncating with an ellipsis if it's
/// long; fall back to a short id prefix when no name was announced yet (better than a 64-char hex
/// blob). Kept pure so it can be unit-tested without a running mesh.
fn short_peer_label(name: &str, id: &str) -> String {
    const MAX: usize = 12;
    let name = name.trim();
    if name.is_empty() {
        return id.chars().take(6).collect();
    }
    if name.chars().count() > MAX {
        let head: String = name.chars().take(MAX - 1).collect();
        format!("{head}…")
    } else {
        name.to_string()
    }
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
fn run_live() -> Result<LiveExit, slint::PlatformError> {
    let app = AppWindow::new()?;
    app.set_dev_chrome(false);
    app.set_presets(ModelRc::from(Rc::new(VecModel::default())));
    app.set_presets_enabled(false);
    app.set_read_only_mode(false);
    app.set_online_count(1);
    app.set_screen(0); // tray flyout

    let config_dir = match persist::ensure_config_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("screen-hop --live: cannot open config dir: {e}");
            return run_readonly(app, "read-only".into(), &[], &HashMap::new())
                .map(|()| LiveExit::Quit);
        }
    };
    let identity = persist::load_or_create_identity(&config_dir).unwrap_or_else(|e| {
        eprintln!("screen-hop --live: identity error: {e}; using an ephemeral identity");
        PeerIdentity::generate()
    });
    let cfg = load_live_config(&config_dir);
    let can_actuate = cfg.can_actuate;
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
        if let Err(e) = quirks.load_local(&config_dir.join("quirks-local.json")) {
            eprintln!("screen-hop --live: cannot load learned monitor quirks: {e}");
        }
        if let Err(e) = quirks.load_user(&config_dir.join("quirks-user.json")) {
            eprintln!("screen-hop --live: cannot load user monitor quirks: {e}");
        }
        std::thread::spawn(move || {
            let mut driver = DdcHiDriver::enumerate();
            let mons = identified_monitors(&driver, &aliases);
            driver.remap_ids(|m| effective_id(m, &aliases));
            let _ = mon_tx.send(mons);

            if !can_actuate {
                for req in req_rx {
                    match req {
                        ActuatorRequest::Switch { reply, .. } => {
                            // Defensive fallback: a read-only node is not attached to the mesh as
                            // an actuator, but if an internal caller sends a switch anyway, report
                            // an honest failure and never touch DDC.
                            let _ = reply.send(ActuationReport::new(SwitchOutcome::Failed, None));
                        }
                        ActuatorRequest::Read { monitor_id, reply } => {
                            let _ = reply.send(driver.try_read_input(&monitor_id));
                        }
                    }
                }
                return;
            }

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

    // A mesh secret is required to form the mesh. First run (no secret) → onboarding wizard, so a
    // new user can pair from the window instead of hand-creating a file.
    let Some(secret) = secret else {
        return run_first_run_wizard(app, &config_dir);
    };

    // --- mesh node + agent ----------------------------------------------------------------------
    let recon_tx = req_tx.clone(); // a second handle to the actuator thread, for reconcile reads
    let node = Node::new(identity, &secret).with_pin_store(persist::pins_path(&config_dir));
    let node = if can_actuate {
        node.with_actuator(ChannelActuator::new(req_tx))
    } else {
        node
    };
    let me = node.peer_id();

    let listener = match TcpListener::bind(("0.0.0.0", cfg.port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("screen-hop --live: cannot bind port {}: {e}", cfg.port);
            return run_readonly(app, me, &monitor_ids, &labels).map(|()| LiveExit::Quit);
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
    let agent = LiveAgent::new(node, agent_name, can_actuate, self_addr, manual, mdns);
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
    let mut initial_binding = bind::build_tray(
        &controller,
        &monitor_ids,
        std::slice::from_ref(&me),
        &["This PC".to_string()],
    );
    if let Some(peer) = initial_binding.peers.first_mut() {
        peer.enabled = can_actuate;
    }
    let binding = Rc::new(RefCell::new(initial_binding));
    let pending: Rc<RefCell<HashMap<String, (String, Instant)>>> =
        Rc::new(RefCell::new(HashMap::new()));
    let capable_peer_ids: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(HashSet::new()));
    if can_actuate {
        capable_peer_ids.borrow_mut().insert(me.clone());
    }

    // on_switch: enqueue the intent, mark the monitor pending, flip the row to in-flight now.
    {
        let binding = Rc::clone(&binding);
        let pending = Rc::clone(&pending);
        let capable_peer_ids = Rc::clone(&capable_peer_ids);
        let monitors_vm = monitors_vm.clone();
        app.on_switch(move |mi, pi| {
            let Some((monitor_id, target)) = binding.borrow().resolve_switch(mi, pi) else {
                eprintln!("screen-hop: click row={mi} seg={pi} -> no monitor/peer at those indices");
                return;
            };
            if !target_can_actuate(&capable_peer_ids.borrow(), &target) {
                eprintln!(
                    "screen-hop: switch rejected: target peer {target} is offline or read-only"
                );
                return;
            }
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
        let capable_peer_ids = Rc::clone(&capable_peer_ids);
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
            let mut capable = HashSet::new();
            let mut online_count = 1_i32; // this process is running even when configured read-only
            if can_actuate {
                capable.insert(me.clone());
            }
            for pv in controller.peer_views(now) {
                if pv.id != me && pv.online {
                    online_count += 1;
                }
                if pv.online && pv.can_actuate {
                    capable.insert(pv.id.clone());
                }
                if pv.id != me {
                    peer_labels.push(short_peer_label(&pv.name, &pv.id));
                    peer_ids.push(pv.id);
                }
            }
            let mut b = bind::build_tray(&controller, &monitor_ids, &peer_ids, &peer_labels);
            for (peer, id) in b.peers.iter_mut().zip(peer_ids.iter()) {
                peer.enabled = target_can_actuate(&capable, id);
            }
            *capable_peer_ids.borrow_mut() = capable;

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
                    // The executor has a 15 s ceiling and the authenticated transport allows a
                    // small response margin; keep progress visible for that same full window.
                    !arrived && since.elapsed() < Duration::from_secs(25)
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
            app.set_online_count(online_count);
            app.set_degraded(controller.is_degraded(now));
            *binding.borrow_mut() = b;
        });
    }

    app.run().map(|()| LiveExit::Quit)
}

/// First-run onboarding: no mesh secret yet. Show the wizard's Pair step; when the user commits a
/// passphrase, save it (same `mesh-secret` format as the CLI) and ask `main()` to relaunch so the
/// normal live path brings the mesh up. Closing the wizard just exits — the user can also drop a
/// `mesh-secret` file in by hand (the classic path).
fn run_first_run_wizard(
    app: AppWindow,
    config_dir: &std::path::Path,
) -> Result<LiveExit, slint::PlatformError> {
    app.set_screen(1); // onboarding wizard
    app.set_wizard_step(1); // Pair

    let relaunch = Rc::new(std::cell::Cell::new(false));
    {
        let app_weak = app.as_weak();
        let config_dir = config_dir.to_path_buf();
        let relaunch = Rc::clone(&relaunch);
        app.on_wizard_pair(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let secret = app.get_wizard_secret().trim().to_string();
            if secret.is_empty() {
                app.set_wizard_secret("".into());
                app.set_wizard_error("Enter at least one non-space character.".into());
                return;
            }
            app.set_wizard_error("".into());
            if let Err(e) = persist::save_secret(&config_dir, &secret) {
                eprintln!("screen-hop --live: could not save mesh secret: {e}");
                app.set_wizard_error(format!("Could not save the passphrase: {e}").into());
                return;
            }
            relaunch.set(true);
            let _ = slint::quit_event_loop();
        });
    }
    app.on_wizard_close(|| {
        let _ = slint::quit_event_loop();
    });

    println!(
        "screen-hop --live: first run — enter the SAME shared passphrase on each PC to pair, or drop \
         a `mesh-secret` file into {} by hand.",
        config_dir.display()
    );
    app.run()?;
    Ok(if relaunch.get() {
        LiveExit::Relaunch
    } else {
        LiveExit::Quit
    })
}

/// Read-only fallback: show the enumerated monitors with no mesh (no secret / bind failure).
fn run_readonly(
    app: AppWindow,
    me: String,
    monitor_ids: &[String],
    labels: &HashMap<String, String>,
) -> Result<(), slint::PlatformError> {
    // This path has no functioning mesh actuator (config-dir or listen failure). Replace every
    // design/default model with honest live data and mark the switch controls unavailable.
    app.set_dev_chrome(false);
    app.set_degraded(false);
    app.set_read_only_mode(true);
    app.set_online_count(0);
    app.set_presets(ModelRc::from(Rc::new(VecModel::default())));
    app.set_presets_enabled(false);
    app.on_switch(|_, _| {
        eprintln!("screen-hop: switch ignored because the agent is read-only");
    });

    let state = Arc::new(Mutex::new(MeshState::default()));
    let mut controller = Controller::new(me, state, 20_000);
    for (id, label) in labels {
        controller.set_label(id, label);
    }
    let mut b = bind::build_tray(
        &controller,
        monitor_ids,
        &["this-pc".to_string()],
        &["This PC".to_string()],
    );
    for peer in &mut b.peers {
        peer.enabled = false;
    }
    app.set_monitors(ModelRc::from(Rc::new(VecModel::from(b.monitors))));
    app.set_peers(ModelRc::from(Rc::new(VecModel::from(b.peers))));
    app.set_screen(0);
    app.run()
}

/// Design-preview / snapshot mode (the original behaviour).
fn run_preview(args: &[String]) -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;

    // Keep the intentionally interactive design preview available, while live/read-only modes
    // remain fail-closed. These opt-ins are never set by `run_live` or `run_readonly`.
    app.set_dev_chrome(true);
    app.set_simulate_switches(true);
    let preview_preset_names = vec![
        "Trading".to_string(),
        "Work".to_string(),
        "Couch".to_string(),
    ];
    app.set_presets(ModelRc::from(Rc::new(VecModel::from(bind::build_presets(
        &preview_preset_names,
        Some(0),
    )))));
    app.set_presets_enabled(true);

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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::{load_live_config, short_peer_label, target_can_actuate};

    static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_config_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "screenhop-ui-config-{}-{}",
            std::process::id(),
            TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn short_label_prefers_a_short_name_verbatim() {
        assert_eq!(short_peer_label("Couch", "abc123def456"), "Couch");
    }

    #[test]
    fn short_label_truncates_long_names_with_an_ellipsis() {
        let out = short_peer_label("DESKTOP-LONGHOSTNAME", "id");
        assert_eq!(out, "DESKTOP-LON…");
        assert_eq!(out.chars().count(), 12);
    }

    #[test]
    fn short_label_falls_back_to_a_short_id_prefix_when_unnamed() {
        // Better a 6-char prefix than a 64-char hex blob in a narrow tray segment.
        assert_eq!(short_peer_label("   ", "0123456789abcdef"), "012345");
    }

    #[test]
    fn malformed_config_fails_closed_for_local_actuation() {
        let dir = temp_config_dir();
        fs::write(dir.join(screenhop_app::persist::CONFIG_FILE), b"not json").unwrap();

        let cfg = load_live_config(&dir);

        assert!(!cfg.can_actuate);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn switch_target_must_be_in_the_live_actuator_set() {
        let capable = HashSet::from(["writer".to_string()]);
        assert!(target_can_actuate(&capable, "writer"));
        assert!(!target_can_actuate(&capable, "read-only"));
        assert!(!target_can_actuate(&capable, "offline"));
    }
}
