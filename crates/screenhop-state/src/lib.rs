//! Replicated mesh state for screen-hop (milestone M3): the per-monitor lease lock that
//! serializes switches (decisions D1/D5) and the last-writer-wins ownership map whose ground
//! truth is the live `0x60` (§8.5/§8.6). Pure logic with an injected clock — no I/O.

pub mod lock;
pub mod ownership;

pub use lock::{
    Lease, LockManager, LockOutcome, DEFAULT_LEASE_MS, MIN_LEASE_MS, SWITCH_CEILING_MS,
};
pub use ownership::{OwnershipMap, OwnershipRecord, OwnershipState};

pub type PeerId = String;
pub type MonitorId = String;
