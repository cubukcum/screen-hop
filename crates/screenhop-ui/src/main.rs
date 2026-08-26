//! Local screen-hop desktop binary: one PC, one selected monitor, two confirmed inputs.

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use screenhop_app::{
    ensure_config_dir, load_config, save_config, LocalConfig, LocalSwitchReport, LocalSwitchStatus,
    LocalSwitcher, SourceSlot, SourceState,
};
use screenhop_core::{MonitorDriver, RealClock, RealDelayer, SwitchExecutor, SwitchOutcome};
use screenhop_ddc::{DdcHiDriver, MonitorInfo};
use screenhop_quirks::QuirksDb;
use screenhop_ui::{bind, AppWindow, Controller};
use slint::{ComponentHandle, ModelRc, Timer, TimerMode, VecModel};

const READ_REFRESH: Duration = Duration::from_secs(3);
const SETUP_POLL_INTERVAL: Duration = Duration::from_millis(250);
const SETUP_POLL_ATTEMPTS: usize = 120;
const MONITOR_ENUM_TIMEOUT: Duration = Duration::from_secs(10);

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|argument| argument == key)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn normalize_source_names(
    source_a: &str,
    source_b: &str,
) -> Result<(String, String), &'static str> {
    let source_a = source_a.trim().to_owned();
    let source_b = source_b.trim().to_owned();
    if source_a.is_empty() || source_b.is_empty() {
        return Err("Give both sources a name.");
    }
    Ok((source_a, source_b))
}

fn config_with_source_names(
    config: &LocalConfig,
    source_a: &str,
    source_b: &str,
) -> Result<LocalConfig, &'static str> {
    let (source_a, source_b) = normalize_source_names(source_a, source_b)?;
    let mut config = config.clone();
    config.source_mut(SourceSlot::A).label = source_a;
    config.source_mut(SourceSlot::B).label = source_b;
    Ok(config)
}

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

    if args.iter().any(|argument| argument == "--monitors") {
        run_monitors();
        return Ok(());
    }

    if args
        .iter()
        .any(|argument| argument == "--preview" || argument == "--shot")
    {
        return run_preview(&args);
    }

    // --live remains a harmless compatibility alias for older shortcuts. Local mode is now the
    // normal no-argument product. --calibrate opens the same guided setup as --setup.
    let force_setup = args
        .iter()
        .any(|argument| argument == "--setup" || argument == "--calibrate");
    run_local(force_setup)
}

#[derive(Debug, Clone)]
struct MonitorDescriptor {
    id: String,
    name: String,
    detail: String,
    recommended: bool,
    model_token: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct SetupDraft {
    monitor_id: Option<String>,
    monitor_name: Option<String>,
    monitor_model_token: Option<String>,
    source_a: Option<u16>,
    source_b: Option<u16>,
}

struct UiState {
    config: LocalConfig,
    observed: SourceState,
    pending: Option<SourceSlot>,
    read_in_flight: bool,
    worker_available: bool,
    safety_policy_error: Option<String>,
    last_read_started: Instant,
    monitors: Vec<MonitorDescriptor>,
    draft: SetupDraft,
    config_dir: PathBuf,
}

enum WorkerCommand {
    Read {
        config: LocalConfig,
    },
    SwitchTo {
        config: LocalConfig,
        source: SourceSlot,
    },
    CaptureA {
        monitor_id: String,
    },
    CaptureBAndReturn {
        monitor_id: String,
        monitor_model_token: Option<String>,
        source_a: u16,
    },
}

struct CaptureBSuccess {
    source_b: u16,
    return_report: LocalSwitchReport,
}

enum WorkerEvent {
    Read(SourceState),
    Switched {
        config: LocalConfig,
        state_after: SourceState,
        report: LocalSwitchReport,
    },
    CapturedA(Result<u16, String>),
    CapturedB(Result<CaptureBSuccess, String>),
}

struct WorkerHandle {
    command_tx: mpsc::Sender<WorkerCommand>,
    event_rx: mpsc::Receiver<WorkerEvent>,
    monitors: Vec<MonitorDescriptor>,
    startup_error: Option<String>,
    safety_policy_error: Option<String>,
}

fn run_local(force_setup: bool) -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;
    let config_dir = match ensure_config_dir() {
        Ok(directory) => directory,
        Err(error) => {
            let config = LocalConfig::default();
            apply_view(
                &app,
                &Controller::new().view(&config, SourceState::Unconfigured, None),
            );
            app.set_detected_monitors(ModelRc::from(Rc::new(VecModel::default())));
            app.set_selected_monitor(-1);
            app.set_screen(1);
            app.set_setup_message(
                format!("Could not open the configuration directory: {error}").into(),
            );
            return app.run();
        }
    };

    let (config, config_error) = match load_config(&config_dir) {
        Ok(config) => (config, None),
        Err(error) => (
            LocalConfig::default(),
            Some(format!(
                "The existing configuration is invalid and was not trusted: {error}. Complete setup to replace it."
            )),
        ),
    };

    let WorkerHandle {
        command_tx,
        event_rx,
        monitors,
        startup_error: worker_error,
        safety_policy_error,
    } = start_worker(config_dir.clone());
    let worker_available = worker_error.is_none();
    let selected_position = config
        .selected_monitor
        .as_deref()
        .and_then(|selected| monitors.iter().position(|monitor| monitor.id == selected));
    let selected_monitor_missing = config.is_ready() && selected_position.is_none();
    let selected_monitor_error = selected_monitor_missing.then(|| {
        "The configured monitor handle is not available. Complete setup again before switching."
            .to_owned()
    });
    let startup_message = [
        config_error.as_deref(),
        worker_error.as_deref(),
        safety_policy_error.as_deref(),
        selected_monitor_error.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    let startup_message = (!startup_message.is_empty()).then_some(startup_message);
    let detected_rows: Vec<(String, String, bool)> = monitors
        .iter()
        .map(|monitor| {
            (
                monitor.name.clone(),
                monitor.detail.clone(),
                monitor.recommended,
            )
        })
        .collect();
    app.set_detected_monitors(ModelRc::from(Rc::new(VecModel::from(
        bind::build_detected_monitors(&detected_rows),
    ))));

    let selected_index = selected_position.map_or_else(
        || default_setup_monitor_index(&monitors),
        |index| index as i32,
    );
    app.set_selected_monitor(selected_index);
    app.set_source_a_name(config.source(SourceSlot::A).label.as_str().into());
    app.set_source_b_name(config.source(SourceSlot::B).label.as_str().into());

    let observed = if config.is_ready() {
        SourceState::Unreadable
    } else {
        SourceState::Unconfigured
    };
    let state = Rc::new(RefCell::new(UiState {
        config,
        observed,
        pending: None,
        read_in_flight: false,
        worker_available,
        safety_policy_error,
        last_read_started: Instant::now() - READ_REFRESH,
        monitors,
        draft: SetupDraft::default(),
        config_dir,
    }));

    {
        let state_ref = state.borrow();
        apply_state_view(&app, &state_ref);
    }

    if force_setup
        || selected_monitor_missing
        || !state.borrow().config.is_ready()
        || !state.borrow().worker_available
    {
        open_setup(&app, &mut state.borrow_mut(), startup_message.as_deref());
    } else {
        app.set_screen(0);
        let mut ui = state.borrow_mut();
        request_read(&command_tx, &mut ui);
        if !ui.worker_available {
            apply_state_view(&app, &ui);
        }
    }

    wire_callbacks(&app, Rc::clone(&state), command_tx.clone());

    let timer = Timer::default();
    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        let command_tx = command_tx.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(100), move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };

            while let Ok(event) = event_rx.try_recv() {
                handle_worker_event(&app, &state, event);
            }

            let mut ui = state.borrow_mut();
            if app.get_screen() == 0
                && ui.worker_available
                && ui.config.is_ready()
                && ui.pending.is_none()
                && !ui.read_in_flight
                && ui.last_read_started.elapsed() >= READ_REFRESH
            {
                request_read(&command_tx, &mut ui);
                if !ui.worker_available {
                    apply_state_view(&app, &ui);
                }
            }
        });
    }

    app.run()
}

fn wire_callbacks(
    app: &AppWindow,
    state: Rc<RefCell<UiState>>,
    command_tx: mpsc::Sender<WorkerCommand>,
) {
    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        let command_tx = command_tx.clone();
        app.on_switch_source(move |index| {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let mut ui = state.borrow_mut();
            if !ui.worker_available {
                show_worker_stopped(&app, &mut ui);
                return;
            }
            if ui.pending.is_some() {
                return;
            }
            let Some(source) = bind::resolve_source(&ui.config, index) else {
                app.set_status_kind(4);
                app.set_status_text("Source is not configured".into());
                app.set_message("Rerun setup before switching.".into());
                return;
            };
            ui.pending = Some(source);
            apply_state_view(&app, &ui);
            let config = ui.config.clone();
            if command_tx
                .send(WorkerCommand::SwitchTo { config, source })
                .is_err()
            {
                show_worker_stopped(&app, &mut ui);
            }
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        let command_tx = command_tx.clone();
        app.on_toggle(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let mut ui = state.borrow_mut();
            if !ui.worker_available {
                show_worker_stopped(&app, &mut ui);
                return;
            }
            if ui.pending.is_some() {
                return;
            }
            let Some((target, request)) = toggle_request(&ui.config, ui.observed) else {
                app.set_status_kind(3);
                app.set_status_text("Choose a source explicitly".into());
                app.set_message(
                    "The current input is not known well enough for a blind toggle.".into(),
                );
                return;
            };
            ui.pending = Some(target);
            apply_state_view(&app, &ui);
            if command_tx.send(request).is_err() {
                show_worker_stopped(&app, &mut ui);
            }
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        app.on_open_setup(move || {
            if let Some(app) = app_weak.upgrade() {
                open_setup(&app, &mut state.borrow_mut(), None);
            }
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        app.on_open_settings(move || {
            if let Some(app) = app_weak.upgrade() {
                let ui = state.borrow();
                app.set_source_a_name(ui.config.source(SourceSlot::A).label.as_str().into());
                app.set_source_b_name(ui.config.source(SourceSlot::B).label.as_str().into());
                app.set_settings_name_message("".into());
                app.set_settings_name_message_kind(0);
                app.set_screen(2);
            }
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        app.on_close_settings(move || {
            if let Some(app) = app_weak.upgrade() {
                let ui = state.borrow();
                app.set_source_a_name(ui.config.source(SourceSlot::A).label.as_str().into());
                app.set_source_b_name(ui.config.source(SourceSlot::B).label.as_str().into());
                app.set_settings_name_message("".into());
                app.set_settings_name_message_kind(0);
                app.set_screen(0);
            }
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        app.on_save_source_names(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let source_a_name = app.get_source_a_name();
            let source_b_name = app.get_source_b_name();
            let mut ui = state.borrow_mut();
            if ui.pending.is_some() {
                app.set_settings_name_message(
                    "Wait for the current switch to finish before renaming.".into(),
                );
                app.set_settings_name_message_kind(2);
                return;
            }
            let config = match config_with_source_names(
                &ui.config,
                source_a_name.as_str(),
                source_b_name.as_str(),
            ) {
                Ok(config) => config,
                Err(message) => {
                    app.set_settings_name_message(message.into());
                    app.set_settings_name_message_kind(2);
                    return;
                }
            };
            if let Err(error) = save_config(&ui.config_dir, &config) {
                app.set_settings_name_message(format!("Could not save names: {error}").into());
                app.set_settings_name_message_kind(2);
                return;
            }

            let source_a_name = config.source(SourceSlot::A).label.clone();
            let source_b_name = config.source(SourceSlot::B).label.clone();
            ui.config = config;
            app.set_source_a_name(source_a_name.into());
            app.set_source_b_name(source_b_name.into());
            app.set_settings_name_message("Names saved. Captured inputs were not changed.".into());
            app.set_settings_name_message_kind(1);
            apply_state_view(&app, &ui);
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        app.on_setup_continue(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let selected = app.get_selected_monitor();
            let Ok(index) = usize::try_from(selected) else {
                app.set_setup_message("Choose a monitor first.".into());
                return;
            };
            let mut ui = state.borrow_mut();
            let Some(monitor) = ui.monitors.get(index).cloned() else {
                app.set_setup_message("That monitor is no longer available. Reopen setup.".into());
                return;
            };
            ui.draft = SetupDraft {
                monitor_id: Some(monitor.id),
                monitor_name: Some(monitor.name),
                monitor_model_token: monitor.model_token,
                source_a: None,
                source_b: None,
            };
            app.set_setup_message("".into());
            app.set_setup_step(2);
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        let command_tx = command_tx.clone();
        app.on_setup_capture_a(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let ui = state.borrow();
            let Some(monitor_id) = ui.draft.monitor_id.clone() else {
                app.set_setup_message("Return to monitor selection and choose a display.".into());
                return;
            };
            drop(ui);
            app.set_setup_busy(true);
            app.set_setup_message("Reading the currently visible input…".into());
            if command_tx
                .send(WorkerCommand::CaptureA { monitor_id })
                .is_err()
            {
                show_worker_stopped(&app, &mut state.borrow_mut());
                app.set_setup_busy(false);
                app.set_setup_message("The DDC worker stopped. Reopen screen-hop.".into());
            }
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        let command_tx = command_tx.clone();
        app.on_setup_listen_b(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let ui = state.borrow();
            let (Some(monitor_id), Some(source_a)) =
                (ui.draft.monitor_id.clone(), ui.draft.source_a)
            else {
                app.set_setup_message("Capture source A before listening for source B.".into());
                return;
            };
            let monitor_model_token = ui.draft.monitor_model_token.clone();
            drop(ui);
            app.set_setup_busy(true);
            app.set_setup_message(
                "Listening for up to 30 seconds. Physically switch the monitor to source B now; screen-hop will then attempt to return to A.".into(),
            );
            if command_tx
                .send(WorkerCommand::CaptureBAndReturn {
                    monitor_id,
                    monitor_model_token,
                    source_a,
                })
                .is_err()
            {
                show_worker_stopped(&app, &mut state.borrow_mut());
                app.set_setup_busy(false);
                app.set_setup_message("The DDC worker stopped. Reopen screen-hop.".into());
            }
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        let command_tx = command_tx.clone();
        app.on_setup_finish(move || {
            let Some(app) = app_weak.upgrade() else {
                return;
            };
            let mut ui = state.borrow_mut();
            let (Some(monitor_id), Some(monitor_name), Some(source_a), Some(source_b)) = (
                ui.draft.monitor_id.clone(),
                ui.draft.monitor_name.clone(),
                ui.draft.source_a,
                ui.draft.source_b,
            ) else {
                app.set_setup_message(
                    "Both source captures must pass before setup can finish.".into(),
                );
                return;
            };

            let (source_a_name, source_b_name) = match normalize_source_names(
                app.get_source_a_name().as_str(),
                app.get_source_b_name().as_str(),
            ) {
                Ok(names) => names,
                Err(message) => {
                    app.set_setup_message(message.into());
                    return;
                }
            };

            let mut config = LocalConfig {
                selected_monitor: Some(monitor_id.clone()),
                selected_monitor_model_token: ui.draft.monitor_model_token.clone(),
                ..LocalConfig::default()
            };
            config.monitor_aliases.insert(monitor_id, monitor_name);
            config.source_mut(SourceSlot::A).label = source_a_name;
            config.source_mut(SourceSlot::A).confirmed_value = Some(source_a);
            config.source_mut(SourceSlot::B).label = source_b_name;
            config.source_mut(SourceSlot::B).confirmed_value = Some(source_b);
            config.last_requested = Some(SourceSlot::A);

            if let Err(error) = save_config(&ui.config_dir, &config) {
                app.set_setup_message(format!("Could not save setup: {error}").into());
                return;
            }

            ui.config = config;
            ui.observed = SourceState::A;
            ui.pending = None;
            ui.draft = SetupDraft::default();
            app.set_setup_message("".into());
            app.set_setup_busy(false);
            app.set_screen(0);
            apply_state_view(&app, &ui);
            request_read(&command_tx, &mut ui);
            if !ui.worker_available {
                apply_state_view(&app, &ui);
                app.set_status_kind(4);
                app.set_status_text("DDC worker stopped".into());
                app.set_message("Close and reopen screen-hop, then try again.".into());
            }
        });
    }

    {
        let app_weak = app.as_weak();
        let state = Rc::clone(&state);
        app.on_setup_cancel(move || {
            if let Some(app) = app_weak.upgrade() {
                let mut ui = state.borrow_mut();
                ui.draft = SetupDraft::default();
                app.set_setup_busy(false);
                app.set_setup_message("".into());
                app.set_screen(0);
                apply_state_view(&app, &ui);
            }
        });
    }
}

fn handle_worker_event(app: &AppWindow, state: &Rc<RefCell<UiState>>, event: WorkerEvent) {
    match event {
        WorkerEvent::Read(observed) => {
            let mut ui = state.borrow_mut();
            ui.observed = observed;
            ui.read_in_flight = false;
            if ui.pending.is_none() && app.get_screen() == 0 {
                apply_state_view(app, &ui);
            }
        }
        WorkerEvent::Switched {
            config,
            state_after,
            report,
        } => {
            let mut ui = state.borrow_mut();
            ui.config = config;
            ui.observed = state_after;
            ui.pending = None;
            ui.read_in_flight = false;
            if ui.safety_policy_error.is_some() {
                apply_state_view(app, &ui);
            } else {
                let view = Controller::new().view_after_report(&ui.config, ui.observed, &report);
                apply_view(app, &view);
            }

            if report.status.is_effective_success() {
                if let Err(error) = save_config(&ui.config_dir, &ui.config) {
                    app.set_status_kind(3);
                    app.set_message(
                        format!(
                            "The monitor switched, but the last source could not be saved: {error}"
                        )
                        .into(),
                    );
                }
            }
        }
        WorkerEvent::CapturedA(result) => {
            app.set_setup_busy(false);
            match result {
                Ok(value) => {
                    state.borrow_mut().draft.source_a = Some(value);
                    app.set_setup_message(
                        format!("Captured source A as input code 0x{value:02X}.").into(),
                    );
                    app.set_setup_step(3);
                }
                Err(error) => app.set_setup_message(error.into()),
            }
        }
        WorkerEvent::CapturedB(result) => {
            app.set_setup_busy(false);
            match result {
                Ok(success) => {
                    debug_assert!(matches!(
                        success.return_report.status,
                        LocalSwitchStatus::Executed(SwitchOutcome::Success)
                    ));
                    state.borrow_mut().draft.source_b = Some(success.source_b);
                    app.set_setup_message(
                        format!(
                            "Captured source B as 0x{:02X} and confirmed the return to source A.",
                            success.source_b
                        )
                        .into(),
                    );
                    app.set_setup_step(4);
                }
                Err(error) => app.set_setup_message(error.into()),
            }
        }
    }
}

fn request_read(command_tx: &mpsc::Sender<WorkerCommand>, ui: &mut UiState) {
    if ui.read_in_flight || !ui.worker_available || !ui.config.is_ready() {
        return;
    }
    ui.read_in_flight = true;
    ui.last_read_started = Instant::now();
    if command_tx
        .send(WorkerCommand::Read {
            config: ui.config.clone(),
        })
        .is_err()
    {
        ui.read_in_flight = false;
        ui.worker_available = false;
    }
}

fn open_setup(app: &AppWindow, ui: &mut UiState, message: Option<&str>) {
    ui.draft = SetupDraft::default();
    let selected = ui
        .config
        .selected_monitor
        .as_deref()
        .and_then(|id| ui.monitors.iter().position(|monitor| monitor.id == id))
        .map_or_else(
            || default_setup_monitor_index(&ui.monitors),
            |index| index as i32,
        );
    app.set_selected_monitor(selected);
    app.set_setup_step(1);
    app.set_setup_busy(false);
    app.set_setup_message(message.unwrap_or_default().into());
    app.set_source_a_name(ui.config.source(SourceSlot::A).label.as_str().into());
    app.set_source_b_name(ui.config.source(SourceSlot::B).label.as_str().into());
    app.set_screen(1);
}

fn apply_view(app: &AppWindow, view: &screenhop_ui::LocalView) {
    let binding = bind::build_binding(view);
    app.set_monitor_name(binding.monitor_name);
    app.set_monitor_detail(binding.monitor_detail);
    app.set_sources(ModelRc::from(Rc::new(VecModel::from(binding.sources))));
    app.set_active_source(binding.active_source);
    app.set_pending_source(binding.pending_source);
    app.set_ready(binding.ready);
    app.set_status_kind(binding.status_kind);
    app.set_status_text(binding.status_text);
    app.set_message(binding.message);
}

fn apply_state_view(app: &AppWindow, ui: &UiState) {
    let mut view = Controller::new().view(&ui.config, ui.observed, ui.pending);
    if !ui.worker_available {
        view.ready = false;
        view.pending_source = -1;
        view.status_kind = screenhop_ui::StatusKind::Error;
        view.status_text = "DDC worker unavailable".to_owned();
        view.message =
            "No switch command can be sent. Close and reopen screen-hop, then try again."
                .to_owned();
    } else if let Some(error) = &ui.safety_policy_error {
        view.ready = false;
        view.pending_source = -1;
        view.status_kind = screenhop_ui::StatusKind::Error;
        view.status_text = "Safety policy could not be loaded".to_owned();
        view.message = error.clone();
    }
    apply_view(app, &view);
}

fn show_worker_stopped(app: &AppWindow, ui: &mut UiState) {
    ui.pending = None;
    ui.read_in_flight = false;
    ui.worker_available = false;
    apply_state_view(app, ui);
}

fn start_worker(config_dir: PathBuf) -> WorkerHandle {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let (monitor_tx, monitor_rx) = mpsc::channel();

    std::thread::spawn(move || {
        let driver = DdcHiDriver::enumerate();
        let monitors = monitor_descriptors(&driver);

        let mut quirks = QuirksDb::with_shipped();
        let mut quirk_errors = Vec::new();
        if let Err(error) = quirks.load_local(&config_dir.join("quirks-local.json")) {
            eprintln!("screen-hop: cannot load local quirks: {error}");
            quirk_errors.push(format!("local quirks: {error}"));
        }
        if let Err(error) = quirks.load_user(&config_dir.join("quirks-user.json")) {
            eprintln!("screen-hop: cannot load user quirks: {error}");
            quirk_errors.push(format!("user quirks: {error}"));
        }
        let safety_policy_error = (!quirk_errors.is_empty()).then(|| {
            format!(
                "Monitor writes are disabled because a quirks safety file is invalid ({}). Fix or remove the invalid file, then reopen screen-hop.",
                quirk_errors.join("; ")
            )
        });

        let executor = SwitchExecutor::new(driver, RealDelayer, RealClock::default());
        let mut switcher = LocalSwitcher::new(executor, quirks);
        if let Some(error) = &safety_policy_error {
            switcher.disable_writes(error.clone());
        }
        let _ = monitor_tx.send((monitors, safety_policy_error));

        for command in command_rx {
            match command {
                WorkerCommand::Read { config } => {
                    let state = switcher.read_state(&config);
                    if event_tx.send(WorkerEvent::Read(state)).is_err() {
                        break;
                    }
                }
                WorkerCommand::SwitchTo { mut config, source } => {
                    let report = switcher.switch_to(&mut config, source);
                    let state_after = switcher.read_state(&config);
                    if event_tx
                        .send(WorkerEvent::Switched {
                            config,
                            state_after,
                            report,
                        })
                        .is_err()
                    {
                        break;
                    }
                }
                WorkerCommand::CaptureA { monitor_id } => {
                    let result = match read_input_retry(switcher.driver_mut(), &monitor_id) {
                        Some(value) => u16::try_from(value).map_err(|_| {
                            format!("Monitor returned out-of-range input value {value}.")
                        }),
                        None => Err(
                            "Could not read source A. Enable DDC/CI and make sure this PC is visible."
                                .to_owned(),
                        ),
                    };
                    if event_tx.send(WorkerEvent::CapturedA(result)).is_err() {
                        break;
                    }
                }
                WorkerCommand::CaptureBAndReturn {
                    monitor_id,
                    monitor_model_token,
                    source_a,
                } => {
                    let result = capture_b_and_return(
                        &mut switcher,
                        &monitor_id,
                        monitor_model_token,
                        source_a,
                    );
                    if event_tx.send(WorkerEvent::CapturedB(result)).is_err() {
                        break;
                    }
                }
            }
        }
    });

    match monitor_rx.recv_timeout(MONITOR_ENUM_TIMEOUT) {
        Ok((monitors, safety_policy_error)) => WorkerHandle {
            command_tx,
            event_rx,
            monitors,
            startup_error: None,
            safety_policy_error,
        },
        Err(error) => {
            // Do not let commands queue behind an enumeration call that may still be blocked. Once
            // the OS call returns, dropping the only real sender makes that worker exit without
            // performing any DDC writes. The replacement sender is deliberately disconnected so
            // every callback fails closed and can show an honest error immediately.
            drop(command_tx);
            let (stopped_tx, stopped_rx) = mpsc::channel();
            drop(stopped_rx);
            let message = match error {
                mpsc::RecvTimeoutError::Timeout => format!(
                    "Monitor detection did not finish within {} seconds. No switch command was sent. Close and reopen screen-hop; if this repeats, check the display driver and DDC/CI setting.",
                    MONITOR_ENUM_TIMEOUT.as_secs()
                ),
                mpsc::RecvTimeoutError::Disconnected => {
                    "The DDC worker stopped before monitor detection completed. No switch command was sent. Close and reopen screen-hop."
                        .to_owned()
                }
            };
            WorkerHandle {
                command_tx: stopped_tx,
                event_rx,
                monitors: Vec::new(),
                startup_error: Some(message),
                safety_policy_error: None,
            }
        }
    }
}

fn capture_b_and_return<D, L, C>(
    switcher: &mut LocalSwitcher<D, L, C>,
    monitor_id: &str,
    monitor_model_token: Option<String>,
    source_a: u16,
) -> Result<CaptureBSuccess, String>
where
    D: MonitorDriver,
    L: screenhop_core::Delayer,
    C: screenhop_core::Clock,
{
    let mut candidate: Option<(u16, u8)> = None;
    let mut source_b = None;

    for attempt in 0..SETUP_POLL_ATTEMPTS {
        let reading = switcher.driver_mut().try_read_input(monitor_id);
        if let Some(found) = update_distinct_candidate(source_a, &mut candidate, reading) {
            source_b = Some(found);
            break;
        }
        if attempt + 1 < SETUP_POLL_ATTEMPTS {
            std::thread::sleep(SETUP_POLL_INTERVAL);
        }
    }

    let Some(source_b) = source_b else {
        return Err(
            "Source B was not observed reliably within 30 seconds. Use the monitor's physical input control to return to this PC; no configuration was saved."
                .to_owned(),
        );
    };

    let mut proof = LocalConfig {
        selected_monitor: Some(monitor_id.to_owned()),
        selected_monitor_model_token: monitor_model_token,
        ..LocalConfig::default()
    };
    proof.source_mut(SourceSlot::A).confirmed_value = Some(source_a);
    proof.source_mut(SourceSlot::B).confirmed_value = Some(source_b);
    proof.last_requested = Some(SourceSlot::B);

    let return_report = switcher.switch_to(&mut proof, SourceSlot::A);
    if !matches!(
        return_report.status,
        LocalSwitchStatus::Executed(SwitchOutcome::Success)
    ) {
        let detail = return_report
            .detail
            .as_deref()
            .unwrap_or("the return command was not confirmed");
        return Err(format!(
            "Source B was captured as 0x{source_b:02X}, but screen-hop could not confirm the return to A: {detail}. Use the monitor's physical input control; setup was not saved."
        ));
    }

    Ok(CaptureBSuccess {
        source_b,
        return_report,
    })
}

fn read_input_retry<D: MonitorDriver>(driver: &mut D, monitor_id: &str) -> Option<u32> {
    for attempt in 0..8 {
        if let Some(value) = driver.try_read_input(monitor_id) {
            return Some(value);
        }
        if attempt < 7 {
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    None
}

fn update_distinct_candidate(
    source_a: u16,
    candidate: &mut Option<(u16, u8)>,
    reading: Option<u32>,
) -> Option<u16> {
    let value = reading.and_then(|value| u16::try_from(value).ok())?;
    if value == source_a {
        *candidate = None;
        return None;
    }

    match candidate {
        Some((previous, count)) if *previous == value => {
            *count = count.saturating_add(1);
            if *count >= 2 {
                return Some(value);
            }
        }
        _ => *candidate = Some((value, 1)),
    }
    None
}

fn toggle_target(state: SourceState, last_requested: Option<SourceSlot>) -> Option<SourceSlot> {
    match state {
        SourceState::A => Some(SourceSlot::B),
        SourceState::B => Some(SourceSlot::A),
        SourceState::Unreadable => last_requested.map(SourceSlot::opposite),
        SourceState::Unknown(_) | SourceState::Unconfigured => None,
    }
}

fn toggle_request(config: &LocalConfig, state: SourceState) -> Option<(SourceSlot, WorkerCommand)> {
    let target = toggle_target(state, config.last_requested)?;
    Some((
        target,
        WorkerCommand::SwitchTo {
            config: config.clone(),
            source: target,
        },
    ))
}

fn monitor_descriptors(driver: &DdcHiDriver) -> Vec<MonitorDescriptor> {
    build_monitor_descriptors(driver.monitors())
}

fn build_monitor_descriptors(monitors: Vec<MonitorInfo>) -> Vec<MonitorDescriptor> {
    let mut seen_ids = HashSet::new();
    let monitors = monitors
        .into_iter()
        // `DdcHiDriver::index_of` addresses a monitor by the first exact id match. Showing a
        // later duplicate would therefore offer a row that cannot actually be selected.
        .filter(|monitor| seen_ids.insert(monitor.id.clone()))
        .collect::<Vec<_>>();
    let recognized = monitors
        .iter()
        .enumerate()
        .filter_map(|(index, monitor)| recognized_monitor(monitor).then_some(index))
        .collect::<Vec<_>>();
    let recommended_index = if recognized.len() == 1 {
        recognized.first().copied()
    } else {
        None
    };

    monitors
        .into_iter()
        .enumerate()
        .map(|(index, monitor)| {
            let recommended = recommended_index == Some(index);
            let name = friendly_monitor_name(&monitor);
            let identity = if recommended {
                "Best identified match"
            } else if recognized_monitor(&monitor) {
                "Model identified"
            } else {
                "Name unavailable"
            };
            let model_token = monitor.model_token();
            MonitorDescriptor {
                id: monitor.id.clone(),
                name,
                detail: format!(
                    "{identity} · {} · {}",
                    friendly_backend_name(&monitor.backend),
                    monitor.id
                ),
                recommended,
                model_token,
            }
        })
        .collect()
}

fn default_setup_monitor_index(monitors: &[MonitorDescriptor]) -> i32 {
    match monitors {
        [] => -1,
        [_] => 0,
        _ => monitors
            .iter()
            .position(|monitor| monitor.recommended)
            .map_or(-1, |index| index as i32),
    }
}

fn friendly_monitor_name(monitor: &MonitorInfo) -> String {
    let manufacturer = monitor
        .manufacturer
        .as_deref()
        .map(str::trim)
        .filter(|value| !generic_monitor_label(value));
    let model = monitor
        .model
        .as_deref()
        .map(str::trim)
        .filter(|value| !generic_monitor_label(value));

    match (manufacturer, model) {
        (Some(manufacturer), Some(model))
            if model
                .to_ascii_lowercase()
                .starts_with(&manufacturer.to_ascii_lowercase()) =>
        {
            model.to_owned()
        }
        (Some(manufacturer), Some(model)) => format!("{manufacturer} {model}"),
        (Some(manufacturer), None) => format!("{manufacturer} monitor"),
        (None, Some(model)) => model.to_owned(),
        (None, None) => "Unknown monitor".to_owned(),
    }
}

fn recognized_monitor(monitor: &MonitorInfo) -> bool {
    monitor
        .model
        .as_deref()
        .is_some_and(|value| !generic_monitor_label(value))
}

fn generic_monitor_label(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.is_empty()
        || value == "monitor"
        || value == "display"
        || value.contains("generic pnp")
        || value.contains("generic monitor")
        || value.contains("default monitor")
        || value.contains("unknown monitor")
}

fn friendly_backend_name(backend: &str) -> &'static str {
    let backend = backend.to_ascii_lowercase();
    if backend.contains("nvapi") {
        "NVIDIA DDC path"
    } else if backend.contains("winapi") {
        "Windows DDC path"
    } else if backend.contains("i2c") {
        "Linux DDC path"
    } else if backend.contains("mac") {
        "macOS DDC path"
    } else {
        "DDC path"
    }
}

fn run_monitors() {
    let mut driver = DdcHiDriver::enumerate();
    let monitors = driver.monitors();
    println!("{} display handle(s) detected:", monitors.len());
    for (index, monitor) in monitors.iter().enumerate() {
        let input = driver.try_read_input(&monitor.id);
        println!();
        println!(
            "#{index}  {}",
            monitor.model.as_deref().unwrap_or("Monitor")
        );
        println!("    local id   : {}", monitor.id);
        println!("    selected id: {}", monitor.id);
        println!("    backend    : {}", monitor.backend);
        match input {
            Some(value) => println!("    input 0x60 : 0x{value:02X}"),
            None => println!("    input 0x60 : unreadable"),
        }
    }
}

fn run_preview(args: &[String]) -> Result<(), slint::PlatformError> {
    let app = AppWindow::new()?;
    if args.iter().any(|argument| argument == "--dark") {
        app.set_dark(true);
    }
    if let Some(screen) = arg_value(args, "--screen") {
        app.set_screen(match screen.as_str() {
            "setup" | "wizard" => 1,
            "settings" => 2,
            _ => 0,
        });
    }
    if let Some(step) = arg_value(args, "--step").and_then(|value| value.parse::<i32>().ok()) {
        app.set_setup_step(step.clamp(1, 4));
    }
    if args
        .iter()
        .any(|argument| argument == "--setup-layout-stress")
    {
        let rows = vec![
            (
                "Unknown monitor".to_owned(),
                "Name unavailable · Windows DDC path · WinApi:Generic PnP Monitor".to_owned(),
                false,
            ),
            (
                "AOC 27P2DG5".to_owned(),
                "Best identified match · NVIDIA DDC path · Nvapi:AD107-A/2147881091:$Video"
                    .to_owned(),
                true,
            ),
            (
                "Unknown monitor".to_owned(),
                "Name unavailable · Windows DDC path · WinApi:Secondary PnP Monitor".to_owned(),
                false,
            ),
        ];
        app.set_screen(1);
        app.set_setup_step(1);
        app.set_detected_monitors(ModelRc::from(Rc::new(VecModel::from(
            bind::build_detected_monitors(&rows),
        ))));
        app.set_selected_monitor(1);
        app.set_setup_message(
            "The saved configuration could not be read safely. Complete setup to replace it; no monitor input will be changed on this screen."
                .into(),
        );
    }

    if let Some(path) = arg_value(args, "--shot") {
        let delay = arg_value(args, "--delay")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(1_000);
        let weak = app.as_weak();
        Timer::single_shot(Duration::from_millis(delay), move || {
            let Some(app) = weak.upgrade() else {
                return;
            };
            let saved = app
                .window()
                .take_snapshot()
                .map_err(|error| error.to_string())
                .and_then(|buffer| {
                    image::save_buffer(
                        Path::new(&path),
                        buffer.as_bytes(),
                        buffer.width(),
                        buffer.height(),
                        image::ExtendedColorType::Rgba8,
                    )
                    .map_err(|error| error.to_string())
                });
            if let Err(error) = saved {
                eprintln!("screen-hop snapshot failed: {error}");
                std::process::exit(1);
            }
            let _ = slint::quit_event_loop();
        });
    }

    app.run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor_info(
        id: &str,
        manufacturer: Option<&str>,
        model: Option<&str>,
        backend: &str,
    ) -> MonitorInfo {
        MonitorInfo {
            id: id.to_owned(),
            manufacturer: manufacturer.map(str::to_owned),
            model: model.map(str::to_owned),
            serial: None,
            backend: backend.to_owned(),
            fingerprint: None,
        }
    }

    #[test]
    fn source_names_are_trimmed_and_must_both_be_present() {
        assert_eq!(
            normalize_source_names("  Work laptop  ", "Game PC\t"),
            Ok(("Work laptop".to_owned(), "Game PC".to_owned()))
        );
        assert_eq!(
            normalize_source_names("   ", "Game PC"),
            Err("Give both sources a name.")
        );
        assert_eq!(
            normalize_source_names("Work laptop", "\n\t"),
            Err("Give both sources a name.")
        );
    }

    #[test]
    fn renaming_sources_preserves_monitor_inputs_aliases_and_switch_history() {
        let mut original = LocalConfig {
            selected_monitor: Some("display-7".to_owned()),
            selected_monitor_model_token: Some("AOC:27P2DG5".to_owned()),
            ..LocalConfig::default()
        };
        original.source_mut(SourceSlot::A).confirmed_value = Some(0x0f);
        original.source_mut(SourceSlot::B).confirmed_value = Some(0x11);
        original
            .monitor_aliases
            .insert("display-7".to_owned(), "Desk monitor".to_owned());
        original.last_requested = Some(SourceSlot::B);

        let renamed = config_with_source_names(&original, "  Desktop  ", "Laptop ").unwrap();
        let mut expected = original;
        expected.source_mut(SourceSlot::A).label = "Desktop".to_owned();
        expected.source_mut(SourceSlot::B).label = "Laptop".to_owned();

        assert_eq!(renamed, expected);
    }

    #[test]
    fn setup_monitor_rows_dedupe_unaddressable_ids_and_prefer_unique_model() {
        let monitors = build_monitor_descriptors(vec![
            monitor_info("WinApi:Generic PnP Monitor", None, None, "WinApi"),
            monitor_info("WinApi:Generic PnP Monitor", None, None, "WinApi"),
            monitor_info(
                "Nvapi:AD107-A/2147881091:$Video",
                Some("AOC"),
                Some("27P2DG5"),
                "Nvapi",
            ),
        ]);

        assert_eq!(monitors.len(), 2);
        assert_eq!(monitors[0].name, "Unknown monitor");
        assert!(!monitors[0].recommended);
        assert_eq!(monitors[1].name, "AOC 27P2DG5");
        assert!(monitors[1].recommended);
        assert!(monitors[1].detail.starts_with("Best identified match"));
        assert_eq!(default_setup_monitor_index(&monitors), 1);
    }

    #[test]
    fn setup_does_not_guess_between_multiple_recognized_models() {
        let monitors = build_monitor_descriptors(vec![
            monitor_info("display-1", Some("AOC"), Some("27P2DG5"), "WinApi"),
            monitor_info("display-2", Some("DEL"), Some("U2720Q"), "WinApi"),
        ]);

        assert!(monitors.iter().all(|monitor| !monitor.recommended));
        assert_eq!(default_setup_monitor_index(&monitors), -1);
        assert_eq!(default_setup_monitor_index(&[]), -1);
    }

    #[test]
    fn setup_handles_zero_recognized_models_without_a_recommendation() {
        let monitors = build_monitor_descriptors(vec![monitor_info(
            "WinApi:Generic PnP Monitor",
            None,
            Some("Generic PnP Monitor"),
            "WinApi",
        )]);

        assert_eq!(monitors.len(), 1);
        assert_eq!(monitors[0].name, "Unknown monitor");
        assert!(!monitors[0].recommended);
        assert_eq!(default_setup_monitor_index(&monitors), 0);
    }

    #[test]
    fn toggle_hint_is_safe_for_known_and_unreadable_states() {
        assert_eq!(toggle_target(SourceState::A, None), Some(SourceSlot::B));
        assert_eq!(toggle_target(SourceState::B, None), Some(SourceSlot::A));
        assert_eq!(
            toggle_target(SourceState::Unreadable, Some(SourceSlot::B)),
            Some(SourceSlot::A)
        );
        assert_eq!(toggle_target(SourceState::Unreadable, None), None);
        assert_eq!(toggle_target(SourceState::Unknown(0x44), None), None);
    }

    #[test]
    fn toggle_request_writes_exactly_the_target_shown_as_pending() {
        let config = LocalConfig::default();
        let (displayed_target, command) = toggle_request(&config, SourceState::A).unwrap();
        match command {
            WorkerCommand::SwitchTo { source, .. } => assert_eq!(source, displayed_target),
            WorkerCommand::Read { .. }
            | WorkerCommand::CaptureA { .. }
            | WorkerCommand::CaptureBAndReturn { .. } => {
                panic!("toggle must enqueue an explicit SwitchTo command")
            }
        }
    }

    #[test]
    fn second_source_requires_two_matching_distinct_reads() {
        let mut candidate = None;
        assert_eq!(update_distinct_candidate(0x0f, &mut candidate, None), None);
        assert_eq!(
            update_distinct_candidate(0x0f, &mut candidate, Some(0x11)),
            None
        );
        assert_eq!(
            update_distinct_candidate(0x0f, &mut candidate, Some(0x12)),
            None
        );
        assert_eq!(
            update_distinct_candidate(0x0f, &mut candidate, Some(0x12)),
            Some(0x12)
        );
    }

    #[test]
    fn returning_to_source_a_resets_a_partial_b_candidate() {
        let mut candidate = None;
        assert_eq!(
            update_distinct_candidate(0x0f, &mut candidate, Some(0x11)),
            None
        );
        assert_eq!(
            update_distinct_candidate(0x0f, &mut candidate, Some(0x0f)),
            None
        );
        assert!(candidate.is_none());
    }
}
