//! Monitor identity & calibration for screen-hop (milestone M2).
//!
//! - [`fingerprint`]: parse EDID into a composite cross-PC [`MonitorFingerprint`] + stable id.
//! - [`collision`]: group fingerprints (de-dup same panel vs flag identical-model collisions).
//! - [`calibration`]: per-`(peer, monitor)` confirmed `0x60` values (never cross-used — D4).
//!
//! All pure and unit-tested; OS enumeration wiring lives in screenhop-ddc.

pub mod calibration;
pub mod collision;
pub mod fingerprint;

pub use calibration::CalibrationStore;
pub use collision::{collisions_needing_labels, group_by_id, EnumeratedPanel};
pub use fingerprint::{EdidError, MonitorFingerprint};
