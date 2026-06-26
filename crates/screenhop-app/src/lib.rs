//! screen-hop application layer: the per-peer mesh node that ties the secured transport,
//! peer identity, replicated state, and lease lock together (milestone M3). Orchestration
//! (presets, blind-point logic) builds on this in M4.

pub mod mesh;

pub use mesh::{ConnectError, MeshState, Node, Session};
