//! Panel-global quirks DB (decisions D4/D7, plan §6.6).
//!
//! Quirks are **panel-global** behavior facts (working direction, settle timing, read-back
//! reliability, blocked input values, …) keyed by `monitor_id`/model token. They are merged from
//! three layers with precedence **user > local-learned > shipped**, and they replicate across the
//! mesh.
//!
//! SOFT-BRICK INVARIANT (D7): a quirk can only ever **restrict** behavior. `blocked_input_values`
//! is *additive* across layers (a higher layer can add to, but never remove from, the blocked set),
//! and **nothing here authorizes a `0x60` write** — only a peer's own self-calibrated value
//! (`CalibrationStore`, screenhop-identity) may be written. This is what makes accepting community
//! quirk PRs safe.

use std::collections::HashMap;
use std::path::Path;
use std::{fs, io};

use screenhop_core::ActuationPolicy;
use serde::{Deserialize, Serialize};

/// Which physical path is known to work for a panel (advisory hint; selection still happens
/// upstream). Mirrors `screenhop_core::SwitchDirection` but is (de)serializable on its own so the
/// core crate stays dependency-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingDirection {
    PullToSelf,
    PushRelease,
}

/// Panel-global facts for one monitor model/id. Every field is optional so a layer overrides only
/// what it actually specifies (field-level merge).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Quirk {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_direction: Option<WorkingDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readback_unreliable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settle_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sleep_multiplier: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ddc_off_by_default: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requires_active_input: Option<bool>,
    /// Values that must NEVER be written to this panel (safety). Additive across layers (D7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_input_values: Vec<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pbp_capable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

impl Quirk {
    /// Overlay `higher` (higher precedence) onto `self`: present scalar fields win; the blocked
    /// set is UNIONed (never shrinks — D7 "can only restrict").
    fn overlay(&mut self, higher: &Quirk) {
        if higher.working_direction.is_some() {
            self.working_direction = higher.working_direction;
        }
        if higher.readback_unreliable.is_some() {
            self.readback_unreliable = higher.readback_unreliable;
        }
        if higher.settle_ms.is_some() {
            self.settle_ms = higher.settle_ms;
        }
        if higher.sleep_multiplier.is_some() {
            self.sleep_multiplier = higher.sleep_multiplier;
        }
        if higher.ddc_off_by_default.is_some() {
            self.ddc_off_by_default = higher.ddc_off_by_default;
        }
        if higher.requires_active_input.is_some() {
            self.requires_active_input = higher.requires_active_input;
        }
        if higher.pbp_capable.is_some() {
            self.pbp_capable = higher.pbp_capable;
        }
        if higher.source.is_some() {
            self.source = higher.source.clone();
        }
        for v in &higher.blocked_input_values {
            if !self.blocked_input_values.contains(v) {
                self.blocked_input_values.push(*v);
            }
        }
    }

    /// Apply this (merged) quirk's **safety/behavior** facts to an actuation policy. Notably this
    /// is where `blocked_input_values` enters the soft-brick guard. It never adds a confirmed value
    /// (D7): the policy's `confirmed_values` allow-list is owned solely by self-calibration.
    pub fn apply_to_policy(&self, policy: &mut ActuationPolicy) {
        for v in &self.blocked_input_values {
            policy.blocked_values.insert(*v);
        }
        if let Some(ms) = self.settle_ms {
            policy.settle_ms = ms;
        }
        if let Some(unreliable) = self.readback_unreliable {
            policy.readback_reliable = !unreliable;
        }
    }
}

/// The shipped quirks DB embedded at build time from `quirks/quirks.json`.
const SHIPPED_JSON: &str = include_str!("../../../quirks/quirks.json");

/// Layered quirks database. Lookups merge the three layers with precedence
/// **user > local-learned > shipped** (plan §6.6).
#[derive(Debug, Clone, Default)]
pub struct QuirksDb {
    shipped: HashMap<String, Quirk>,
    local: HashMap<String, Quirk>,
    user: HashMap<String, Quirk>,
}

impl QuirksDb {
    /// A DB seeded with the shipped (embedded) quirks only.
    pub fn with_shipped() -> Self {
        Self {
            shipped: parse_layer(SHIPPED_JSON).unwrap_or_default(),
            ..Self::default()
        }
    }

    /// Load the local-learned override layer from a JSON file (missing file ⇒ no-op).
    pub fn load_local(&mut self, path: &Path) -> io::Result<()> {
        self.local = read_layer(path)?;
        Ok(())
    }

    /// Load the user override layer (highest precedence) from a JSON file (missing file ⇒ no-op).
    pub fn load_user(&mut self, path: &Path) -> io::Result<()> {
        self.user = read_layer(path)?;
        Ok(())
    }

    /// The merged quirk for `key` (a `monitor_id` or model token). Always returns a value (an
    /// all-`None` `Quirk::default()` when nothing is known), so callers don't special-case misses.
    pub fn merged(&self, key: &str) -> Quirk {
        let mut q = self.shipped.get(key).cloned().unwrap_or_default();
        if let Some(local) = self.local.get(key) {
            q.overlay(local);
        }
        if let Some(user) = self.user.get(key) {
            q.overlay(user);
        }
        q
    }

    /// Build an [`ActuationPolicy`] for `key` from `confirmed_values` (the per-(peer,monitor)
    /// self-calibrated allow-list — the ONLY source of writable values) plus the merged quirk's
    /// safety/behavior facts (blocked set, settle time, read-back reliability).
    pub fn policy_for(
        &self,
        key: &str,
        confirmed_values: impl IntoIterator<Item = u32>,
    ) -> ActuationPolicy {
        let mut policy = ActuationPolicy::new(confirmed_values, std::iter::empty());
        self.merged(key).apply_to_policy(&mut policy);
        policy
    }
}

fn parse_layer(json: &str) -> Option<HashMap<String, Quirk>> {
    // Tolerate `_comment`-style string fields by deserializing leniently then dropping non-objects.
    let raw: HashMap<String, serde_json::Value> = serde_json::from_str(json).ok()?;
    let mut out = HashMap::new();
    for (k, v) in raw {
        if v.is_object() {
            if let Ok(q) = serde_json::from_value::<Quirk>(v) {
                out.insert(k, q);
            }
        }
    }
    Some(out)
}

fn read_layer(path: &Path) -> io::Result<HashMap<String, Quirk>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(parse_layer(&s)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid quirks JSON"))?),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_db_parses_and_drops_the_comment_field() {
        let db = QuirksDb::with_shipped();
        // The "_comment" string is not a Quirk and must be ignored, not panic.
        assert!(db.merged("SAM-U32H750").settle_ms.is_some());
        assert_eq!(db.merged("nonexistent"), Quirk::default());
    }

    #[test]
    fn precedence_is_user_over_local_over_shipped() {
        let mut db = QuirksDb::with_shipped();
        db.shipped.insert(
            "M".into(),
            Quirk {
                settle_ms: Some(1000),
                readback_unreliable: Some(false),
                blocked_input_values: vec![0x01],
                source: Some("shipped".into()),
                ..Quirk::default()
            },
        );
        db.local.insert(
            "M".into(),
            Quirk {
                settle_ms: Some(2000),
                blocked_input_values: vec![0x02],
                source: Some("local".into()),
                ..Quirk::default()
            },
        );
        db.user.insert(
            "M".into(),
            Quirk {
                settle_ms: Some(3000),
                source: Some("user".into()),
                ..Quirk::default()
            },
        );
        let m = db.merged("M");
        assert_eq!(m.settle_ms, Some(3000)); // user wins
        assert_eq!(m.readback_unreliable, Some(false)); // from shipped (untouched above)
        assert_eq!(m.source.as_deref(), Some("user"));
        // Blocked values are UNIONed across all layers (D7: only ever restrict).
        let mut blocked = m.blocked_input_values.clone();
        blocked.sort();
        assert_eq!(blocked, vec![0x01, 0x02]);
    }

    #[test]
    fn blocked_values_flow_into_policy_but_never_confirm_a_write() {
        let mut db = QuirksDb::default();
        db.shipped.insert(
            "M".into(),
            Quirk {
                blocked_input_values: vec![0x0F],
                settle_ms: Some(2500),
                readback_unreliable: Some(true),
                ..Quirk::default()
            },
        );
        // Self-calibrated confirmed value is 0x11; the quirk blocks 0x0F.
        let policy = db.policy_for("M", [0x11]);
        assert!(policy.blocked_values.contains(&0x0F));
        assert!(policy.confirmed_values.contains(&0x11));
        assert!(!policy.confirmed_values.contains(&0x0F)); // a quirk never confirms a value (D7)
        assert_eq!(policy.settle_ms, 2500);
        assert!(!policy.readback_reliable);
    }
}
