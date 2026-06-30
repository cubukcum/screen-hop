//! screen-hop application layer: the per-peer mesh node that ties the secured transport,
//! peer identity, replicated state, and lease lock together (milestone M3). Orchestration
//! (presets, blind-point logic) builds on this in M4.

pub mod actuator;
pub mod discovery;
pub mod harness;
pub mod mesh;
pub mod orchestration;
pub mod peers;
pub mod persist;
pub mod reconcile;
pub mod runtime;

pub use actuator::LocalActuator;
pub use discovery::{merge, DiscoveredPeer, Discovery, ManualHosts, MdnsDiscovery, PeerSource};
pub use harness::{soak_panel, PanelStats, SoakReport, SoakSample};
pub use mesh::{ActuationReport, Actuator, ConnectError, MeshState, Node, Session};
pub use orchestration::{
    execute_plan, plan_preset, resolve_actuation, would_go_blind, ActuationError, PlannedSwitch,
    PresetOutcome, SwitchOp, SwitchOpResult, SwitchPlan,
};
pub use peers::{PeerPresence, PeerRegistry};
pub use persist::AgentConfig;
pub use reconcile::{
    read_to_live_read, reconcile_all, reconcile_one, reconcile_reads, LiveRead, ReconcileChange,
};
pub use runtime::{ActuatorRequest, ChannelActuator, LiveAgent, UiIntent};
