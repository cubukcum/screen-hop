//! Cross-platform DDC/CI [`MonitorDriver`] backed by the `ddc-hi` crate
//! (`ddc-winapi` on Windows, `ddc-i2c` on Linux, `ddc-macos` on macOS — incl. Apple Silicon).
//!
//! Monitor identity here is provisional (manufacturer/model/serial + ordinal); M2 replaces
//! it with the composite EDID fingerprint. Not unit-tested — it needs real hardware; the
//! actuation logic that depends on it is tested through `MonitorDriver` fakes in screenhop-core.

use ddc_hi::{Ddc, Display, DisplayInfo};
use screenhop_core::{DdcWriteResult, MonitorDriver};
use screenhop_identity::MonitorFingerprint;

/// VCP feature code for Input Select.
const VCP_INPUT_SELECT: u8 = 0x60;

/// Identity + backend for a discovered monitor.
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Provisional per-handle id (backend-specific). Use [`MonitorInfo::monitor_id`] for the
    /// stable cross-PC id.
    pub id: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial: Option<u32>,
    pub backend: String,
    /// Composite EDID fingerprint, when enough identity is available (M2).
    pub fingerprint: Option<MonitorFingerprint>,
}

impl MonitorInfo {
    /// Stable cross-PC monitor id, when a fingerprint could be built.
    pub fn monitor_id(&self) -> Option<String> {
        self.fingerprint.as_ref().map(MonitorFingerprint::monitor_id)
    }
}

/// Production [`MonitorDriver`] over ddc-hi.
pub struct DdcHiDriver {
    displays: Vec<Display>,
    ids: Vec<String>,
}

impl DdcHiDriver {
    /// Enumerate all DDC/CI-capable displays on this machine.
    pub fn enumerate() -> Self {
        let displays = Display::enumerate();
        let ids = displays
            .iter()
            .enumerate()
            .map(|(i, d)| provisional_id(i, d))
            .collect();
        Self { displays, ids }
    }

    pub fn is_empty(&self) -> bool {
        self.displays.is_empty()
    }

    pub fn len(&self) -> usize {
        self.displays.len()
    }

    /// Identity/info for each discovered monitor, in id order.
    pub fn monitors(&self) -> Vec<MonitorInfo> {
        self.displays
            .iter()
            .zip(&self.ids)
            .map(|(d, id)| MonitorInfo {
                id: id.clone(),
                manufacturer: d.info.manufacturer_id.clone(),
                model: d.info.model_name.clone(),
                serial: d.info.serial,
                backend: format!("{:?}", d.info.backend),
                fingerprint: fingerprint_from_info(&d.info),
            })
            .collect()
    }

    pub fn ids(&self) -> &[String] {
        &self.ids
    }

    fn index_of(&self, monitor_id: &str) -> Option<usize> {
        self.ids.iter().position(|x| x == monitor_id)
    }
}

impl MonitorDriver for DdcHiDriver {
    fn is_ddc_available(&mut self, monitor_id: &str) -> bool {
        self.try_read_input(monitor_id).is_some()
    }

    fn try_read_input(&mut self, monitor_id: &str) -> Option<u32> {
        let idx = self.index_of(monitor_id)?;
        match self.displays[idx].handle.get_vcp_feature(VCP_INPUT_SELECT) {
            Ok(v) => Some(v.value() as u32),
            Err(_) => None,
        }
    }

    fn write_input(&mut self, monitor_id: &str, value: u32) -> DdcWriteResult {
        let Some(idx) = self.index_of(monitor_id) else {
            return DdcWriteResult::Failed;
        };
        // VCP values are 16-bit on the wire; a value that doesn't fit is not a valid input code, so
        // refuse it rather than silently truncating (which could write a *different* input).
        let Ok(value16) = u16::try_from(value) else {
            return DdcWriteResult::Unsupported;
        };
        match self.displays[idx]
            .handle
            .set_vcp_feature(VCP_INPUT_SELECT, value16)
        {
            Ok(()) => DdcWriteResult::Ok,
            Err(e) => classify_write_error(&e),
        }
    }
}

/// ddc-hi doesn't type-distinguish "feature/value unsupported" from a transient failure, so we
/// best-effort sniff the error text: an "unsupported" error is permanent (the executor must NOT
/// retry and should try a fallback path), anything else is treated as a retryable failure. Generic
/// over the error type (ddc-hi's error type is not publicly nameable) — its `Debug` form suffices.
fn classify_write_error<E: std::fmt::Debug>(err: &E) -> DdcWriteResult {
    let msg = format!("{err:?}").to_ascii_lowercase();
    if msg.contains("unsupported") || msg.contains("not supported") {
        DdcWriteResult::Unsupported
    } else {
        DdcWriteResult::Failed
    }
}

/// Build a composite fingerprint from a ddc-hi `DisplayInfo`. Prefers the raw EDID block
/// (Linux/macOS); falls back to parsed identity parts (Windows exposes no raw EDID). Returns
/// `None` when the backend reports no usable identity at all (e.g. a generic Windows handle).
fn fingerprint_from_info(info: &DisplayInfo) -> Option<MonitorFingerprint> {
    if let Some(edid) = &info.edid_data {
        if let Ok(fp) = MonitorFingerprint::from_edid(edid) {
            return Some(fp);
        }
    }

    let has_identity = info.manufacturer_id.is_some()
        || info.serial.is_some()
        || info.serial_number.is_some()
        || info.model_id.is_some();
    if !has_identity {
        return None;
    }

    Some(MonitorFingerprint::from_parts(
        info.manufacturer_id.clone().unwrap_or_default(),
        info.model_id.unwrap_or(0),
        info.serial.unwrap_or(0),
        info.serial_number.clone(),
    ))
}

fn provisional_id(index: usize, d: &Display) -> String {
    let mfr = d.info.manufacturer_id.clone().unwrap_or_default();
    let model = d.info.model_name.clone().unwrap_or_else(|| "Monitor".into());
    let serial = d.info.serial.map(|s| s.to_string()).unwrap_or_default();
    format!("{mfr}-{model}-{serial}#{index}")
}
