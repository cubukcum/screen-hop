//! Stable local monitor identity helpers for screen-hop.
//!
//! - [`fingerprint`]: parse EDID into a composite [`MonitorFingerprint`] + stable local id.
//! - [`collision`]: group fingerprints (de-dup same panel vs flag identical-model collisions).
//!
//! All pure and unit-tested; OS enumeration wiring lives in screenhop-ddc.

pub mod collision;
pub mod fingerprint;

pub use collision::{collisions_needing_labels, group_by_id, EnumeratedPanel};
pub use fingerprint::{EdidError, MonitorFingerprint};
