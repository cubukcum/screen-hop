//! screen-hop UI library: the backend-facing **controller** that the Slint tray binary renders.
//!
//! The Slint surfaces (in `ui/*.slint`) are the approved design layer; `main.rs` runs them. This
//! library is the live data path between [`screenhop_app`] and those surfaces — it produces the
//! view models the UI shows and turns user intents into real backend calls, replacing the
//! design-preview's hardcoded mock data. It is pure Rust (no Slint), so it is unit-tested here; the
//! Slint property binding and the live mesh event loop are wired in `main.rs` and verified on the
//! 2-PC rig (M5 checklist).

pub mod controller;

pub use controller::{Controller, MonitorUiState, MonitorView, PeerView};
