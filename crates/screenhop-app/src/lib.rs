//! Local application layer for toggling one monitor between exactly two configured sources.
//!
//! Networking and peer/mesh orchestration are intentionally not part of the compiled application
//! surface. The core executor still owns DDC retries, verification, timing, and safety guards.

#[path = "local_persist.rs"]
pub mod persist;
#[path = "local_switcher.rs"]
pub mod switcher;

pub use persist::{
    atomic_write, default_config_dir, ensure_config_dir, load_config, save_config, ConfigError,
    LocalConfig, SourceConfig, SourceSlot, CONFIG_FILE, LOCAL_CONFIG_VERSION,
};
pub use switcher::{
    LocalNoWriteReason, LocalSwitchReport, LocalSwitchStatus, LocalSwitcher, SourceState,
};
