use crate::types::DdcWriteResult;
use std::time::Duration;

/// Abstraction over the platform's DDC/CI access. Keeps the actuation state machine
/// fully unit-testable without real hardware (the production implementation is
/// `DdcHiDriver` in the screenhop-ddc crate). Methods take `&mut self` because the
/// underlying ddc-hi handles require mutable access.
pub trait MonitorDriver {
    /// True if the monitor responds to DDC/CI at all (a read of 0x60 succeeds).
    fn is_ddc_available(&mut self, monitor_id: &str) -> bool;

    /// Reads the monitor's current input value (VCP 0x60). `None` if the read fails (inconclusive).
    fn try_read_input(&mut self, monitor_id: &str) -> Option<u32>;

    /// Writes the monitor's input value (VCP 0x60).
    fn write_input(&mut self, monitor_id: &str, value: u32) -> DdcWriteResult;
}

/// Injectable delay so the actuation state machine's timing is testable (no real sleeps in tests).
pub trait Delayer {
    fn delay(&self, milliseconds: u32);
}

/// Production [`Delayer`] backed by `std::thread::sleep`.
pub struct RealDelayer;

impl Delayer for RealDelayer {
    fn delay(&self, milliseconds: u32) {
        if milliseconds > 0 {
            std::thread::sleep(Duration::from_millis(milliseconds as u64));
        }
    }
}
