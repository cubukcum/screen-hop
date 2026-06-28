//! screen-hop application layer: the per-peer mesh node that ties the secured transport,
//! peer identity, replicated state, and lease lock together (milestone M3). Orchestration
//! (presets, blind-point logic) builds on this in M4.

pub mod mesh;
pub mod orchestration;

pub use mesh::{ActuationReport, Actuator, ConnectError, MeshState, Node, Session};
pub use orchestration::{
    plan_preset, resolve_actuation, would_go_blind, ActuationError, PlannedSwitch, SwitchOp,
    SwitchPlan,
};
