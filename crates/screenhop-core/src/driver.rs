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

/// Injectable monotonic clock so the actuation hard-ceiling (D5/§6.3) is enforceable AND
/// unit-testable. Only differences between readings are meaningful (arbitrary epoch).
pub trait Clock {
    /// Monotonic milliseconds since an arbitrary, fixed origin.
    fn now_ms(&self) -> u64;
}

/// Production [`Clock`] backed by a monotonic [`std::time::Instant`] captured at construction.
pub struct RealClock {
    origin: std::time::Instant,
}

impl RealClock {
    pub fn new() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}

impl Default for RealClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for RealClock {
    fn now_ms(&self) -> u64 {
        self.origin.elapsed().as_millis() as u64
    }
}
