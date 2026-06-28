//! Portable domain core for screen-hop: types, the `MonitorDriver`/`Delayer` traits,
//! and the DDC/CI actuation state machine. No OS-specific code lives here, so it
//! compiles and is fully unit-tested on every platform.

pub mod driver;
pub mod executor;
pub mod types;

pub use driver::{Clock, Delayer, MonitorDriver, RealClock, RealDelayer};
pub use executor::SwitchExecutor;
pub use types::{
    ActuationPolicy, DdcWriteResult, SwitchDirection, SwitchOutcome, SwitchRequest, SwitchResult,
};
