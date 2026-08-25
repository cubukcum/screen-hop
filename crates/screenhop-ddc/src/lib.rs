//! Cross-platform DDC/CI [`MonitorDriver`] backed by the `ddc-hi` crate
//! (`ddc-winapi` on Windows, `ddc-i2c` on Linux, `ddc-macos` on macOS — incl. Apple Silicon).
//!
//! Monitor addressing falls back to the backend's unique display id when a serial-backed EDID
//! fingerprint is unavailable. The actuation logic is tested through `MonitorDriver` fakes in
//! screenhop-core; the pure identity/token helpers are tested here.

use ddc_hi::{Ddc, Display, DisplayInfo};
use screenhop_core::{DdcWriteResult, MonitorDriver};
use screenhop_identity::MonitorFingerprint;

/// VCP feature code for Input Select.
const VCP_INPUT_SELECT: u8 = 0x60;

/// Identity + backend for a discovered monitor.
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    /// Unique local backend address for reads and writes. This is deliberately separate from the
    /// optional fingerprint and is the id the one-PC app persists for handle selection.
    pub id: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial: Option<u32>,
    pub backend: String,
    /// Composite EDID fingerprint, when enough identity is available.
    pub fingerprint: Option<MonitorFingerprint>,
}

impl MonitorInfo {
    /// Stable monitor id when the fingerprint includes a real serial discriminator.
    ///
    /// Manufacturer/product alone is shared by every unit of a model and is therefore unsafe for
    /// local addressing: two serial-less displays would collapse to one id and writes could reach
    /// the wrong handle. Callers fall back to [`MonitorInfo::id`] in that case.
    pub fn monitor_id(&self) -> Option<String> {
        self.fingerprint
            .as_ref()
            .filter(|fingerprint| {
                fingerprint.numeric_serial != 0
                    || fingerprint
                        .ascii_serial
                        .as_deref()
                        .is_some_and(|serial| !serial.trim().is_empty())
            })
            .map(MonitorFingerprint::monitor_id)
    }

    /// Normalized manufacturer/model key for applying model-wide quirks.
    ///
    /// This token is deliberately separate from the unique addressing id. It may be shared by
    /// many physical units and must never be used to select a DDC handle.
    pub fn model_token(&self) -> Option<String> {
        let manufacturer = self
            .manufacturer
            .as_deref()
            .or_else(|| {
                self.fingerprint
                    .as_ref()
                    .map(|fingerprint| fingerprint.pnp_manufacturer.as_str())
            })
            .and_then(normalize_token_part)?;
        let model = self.model.as_deref().and_then(normalize_token_part)?;
        Some(format!("{manufacturer}-{model}"))
    }
}

fn normalize_token_part(value: &str) -> Option<String> {
    let mut normalized = String::with_capacity(value.len());
    let mut separator_pending = false;

    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() {
            if separator_pending && !normalized.is_empty() {
                normalized.push('-');
            }
            normalized.push(character.to_ascii_uppercase());
            separator_pending = false;
        } else if !normalized.is_empty() {
            separator_pending = true;
        }
    }

    (!normalized.is_empty()).then_some(normalized)
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

    fn index_of(&self, monitor_id: &str) -> Option<usize> {
        self.ids.iter().position(|x| x == monitor_id)
    }
}

impl MonitorDriver for DdcHiDriver {
    fn is_ddc_available(&mut self, monitor_id: &str) -> bool {
        // A locally controlled monitor may stop answering reads as soon as it displays the other
        // input while still accepting a write that brings it back. Treat an enumerated handle as
        // available and let `write_input` report the real capability result. Using a read as this
        // gate would make the exact B -> A recovery path local mode needs impossible on otherwise
        // cooperative panels.
        self.index_of(monitor_id).is_some()
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
    backend_local_id(&format!("{:?}", d.info.backend), &d.info.id, index)
}

fn backend_local_id(backend: &str, backend_id: &str, index: usize) -> String {
    let backend_id = backend_id.trim();
    if backend_id.is_empty() {
        format!("{backend}:index:{index}")
    } else {
        format!("{backend}:{backend_id}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor_info(
        manufacturer: Option<&str>,
        model: Option<&str>,
        fingerprint: MonitorFingerprint,
    ) -> MonitorInfo {
        MonitorInfo {
            id: "provisional#1".to_owned(),
            manufacturer: manufacturer.map(str::to_owned),
            model: model.map(str::to_owned),
            serial: None,
            backend: "test".to_owned(),
            fingerprint: Some(fingerprint),
        }
    }

    #[test]
    fn ambiguous_serialless_fingerprint_falls_back_to_provisional_id() {
        let monitor = monitor_info(
            Some("DEL"),
            Some("U2720Q"),
            MonitorFingerprint::from_parts("DEL", 0x1234, 0, None),
        );

        assert_eq!(monitor.monitor_id(), None);
        assert_eq!(monitor.id, "provisional#1");
    }

    #[test]
    fn fingerprint_with_a_real_serial_remains_stable() {
        let numeric = monitor_info(
            Some("DEL"),
            Some("U2720Q"),
            MonitorFingerprint::from_parts("DEL", 0x1234, 42, None),
        );
        let ascii = monitor_info(
            Some("DEL"),
            Some("U2720Q"),
            MonitorFingerprint::from_parts("DEL", 0x1234, 0, Some("ABC123".to_owned())),
        );

        assert!(numeric.monitor_id().is_some());
        assert!(ascii.monitor_id().is_some());
    }

    #[test]
    fn model_token_is_normalized_and_requires_both_parts() {
        let normalized = monitor_info(
            Some(" sam "),
            Some("U32H750 / R"),
            MonitorFingerprint::from_parts("SAM", 1, 99, None),
        );
        let missing_model = monitor_info(
            Some("SAM"),
            None,
            MonitorFingerprint::from_parts("SAM", 1, 99, None),
        );

        assert_eq!(normalized.model_token().as_deref(), Some("SAM-U32H750-R"));
        assert_eq!(missing_model.model_token(), None);
    }

    #[test]
    fn local_address_prefers_backend_unique_id_and_only_then_index() {
        assert_eq!(
            backend_local_id("WinApi", r#"\\.\DISPLAY1\Monitor0"#, 7),
            r#"WinApi:\\.\DISPLAY1\Monitor0"#
        );
        assert_eq!(backend_local_id("WinApi", "   ", 7), "WinApi:index:7");
    }
}
