//! screen-hop's local single-monitor/two-source UI contract and pure view-model adapters.
//!
//! Slint renders one verified toggle plus setup/settings surfaces. [`controller`] translates the
//! local app state without performing I/O; [`bind`] converts that result to generated Slint types
//! and validates callback indices before the binary invokes the DDC switcher.

// AppWindow plus the exported SourceChoice/DetectedMonitor structs are generated from app.slint.
slint::include_modules!();

pub mod bind;
pub mod controller;

pub use controller::{Controller, LocalView, SourceView, StatusKind};
