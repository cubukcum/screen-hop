//! screen-hop UI preview (milestone M5). Renders the tray flyout from the Claude Design
//! handoff with sample data. The real app will host these components from a system-tray
//! popover and feed them live mesh state.

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    AppWindow::new()?.run()
}
