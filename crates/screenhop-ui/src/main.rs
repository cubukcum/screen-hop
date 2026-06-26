//! screen-hop UI preview (milestone M5).
//!
//! Normal run opens the preview window with a dev switcher. Snapshot mode renders a surface to a
//! PNG and exits, for visual diffing against the design:
//!   screenhop-ui --shot out.png [--screen flyout|wizard] [--dark]

slint::include_modules!();

use slint::ComponentHandle;

fn arg_value(args: &[String], key: &str) -> Option<String> {
    args.iter().position(|a| a == key).and_then(|i| args.get(i + 1)).cloned()
}

fn main() -> Result<(), slint::PlatformError> {
    let args: Vec<String> = std::env::args().collect();
    let app = AppWindow::new()?;

    if args.iter().any(|a| a == "--dark") {
        app.set_dark(true);
    }
    if let Some(s) = arg_value(&args, "--screen") {
        app.set_screen(match s.as_str() {
            "wizard" => 1,
            "dialog" => 2,
            _ => 0,
        });
    }
    if let Some(s) = arg_value(&args, "--step") {
        if let Ok(n) = s.parse::<i32>() {
            app.set_wizard_step(n);
        }
    }
    if let Some(s) = arg_value(&args, "--dialog") {
        if let Ok(n) = s.parse::<i32>() {
            app.set_dialog(n);
        }
    }

    if let Some(path) = arg_value(&args, "--shot") {
        app.set_dev_chrome(false);
        let weak = app.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(600), move || {
            if let Some(app) = weak.upgrade() {
                match app.window().take_snapshot() {
                    Ok(buf) => {
                        if let Err(e) = image::save_buffer(
                            &path,
                            buf.as_bytes(),
                            buf.width(),
                            buf.height(),
                            image::ExtendedColorType::Rgba8,
                        ) {
                            eprintln!("save error: {e}");
                        }
                    }
                    Err(e) => eprintln!("snapshot error: {e}"),
                }
            }
            let _ = slint::quit_event_loop();
        });
    }

    app.run()
}
