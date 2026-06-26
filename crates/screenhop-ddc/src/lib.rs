//! Cross-platform DDC/CI [`MonitorDriver`] backed by the `ddc-hi` crate
//! (`ddc-winapi` on Windows, `ddc-i2c` on Linux, `ddc-macos` on macOS — incl. Apple Silicon).
//!
//! Monitor identity here is provisional (manufacturer/model/serial + ordinal); M2 replaces
//! it with the composite EDID fingerprint. Not unit-tested — it needs real hardware; the
//! actuation logic that depends on it is tested through `MonitorDriver` fakes in screenhop-core.

use ddc_hi::{Ddc, Display};
use screenhop_core::{DdcWriteResult, MonitorDriver};

/// VCP feature code for Input Select.
const VCP_INPUT_SELECT: u8 = 0x60;

/// Identity + backend for a discovered monitor (provisional pre-M2 identity).
#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub id: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial: Option<u32>,
    pub backend: String,
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
        match self.displays[idx]
            .handle
            .set_vcp_feature(VCP_INPUT_SELECT, value as u16)
        {
            Ok(()) => DdcWriteResult::Ok,
            Err(_) => DdcWriteResult::Failed,
        }
    }
}

fn provisional_id(index: usize, d: &Display) -> String {
    let mfr = d.info.manufacturer_id.clone().unwrap_or_default();
    let model = d.info.model_name.clone().unwrap_or_else(|| "Monitor".into());
    let serial = d.info.serial.map(|s| s.to_string()).unwrap_or_default();
    format!("{mfr}-{model}-{serial}#{index}")
}
