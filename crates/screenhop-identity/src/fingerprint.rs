use std::fmt::Write as _;

use sha2::{Digest, Sha256};

/// Composite, cross-PC monitor identity derived from EDID (docs/PLAN-screen-hop.md §7.2).
///
/// No single field is unique enough on its own — serials are frequently blank/zero or
/// model-constant — so the stable [`MonitorFingerprint::monitor_id`] hashes the identity tuple
/// (manufacturer, product, serials). Manufacture week/year and the raw-EDID hash are kept as
/// advisory fields only and deliberately excluded from the id (see [`MonitorFingerprint::monitor_id`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorFingerprint {
    /// 3-letter PNP manufacturer id, e.g. "AOC".
    pub pnp_manufacturer: String,
    pub product_code: u16,
    /// 32-bit numeric serial (0 if absent).
    pub numeric_serial: u32,
    /// ASCII serial descriptor (0xFF), if present.
    pub ascii_serial: Option<String>,
    pub week: u8,
    /// Full manufacture year (e.g. 2021), or 0 if unknown.
    pub year: u16,
    /// Hex SHA-256 of the full raw EDID — advisory only (drift detection), NOT part of the id.
    pub raw_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EdidError {
    TooShort,
    BadHeader,
}

const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

impl MonitorFingerprint {
    /// Parse the EDID base block (must be at least 128 bytes).
    pub fn from_edid(edid: &[u8]) -> Result<Self, EdidError> {
        if edid.len() < 128 {
            return Err(EdidError::TooShort);
        }
        if edid[0..8] != EDID_HEADER {
            return Err(EdidError::BadHeader);
        }

        let pnp_manufacturer = decode_pnp(edid[8], edid[9]);
        let product_code = u16::from_le_bytes([edid[10], edid[11]]);
        let numeric_serial = u32::from_le_bytes([edid[12], edid[13], edid[14], edid[15]]);
        // EDID byte 16 is the manufacture week, EXCEPT the sentinel 0xFF which flags that byte 17
        // carries a "model year" rather than a manufacture year. Normalize 0xFF to 0 (no week).
        let week = if edid[16] == 0xFF { 0 } else { edid[16] };
        let year = if edid[17] == 0 {
            0
        } else {
            1990 + edid[17] as u16
        };

        // Scan the four 18-byte descriptors for an ASCII serial (0xFF).
        let mut ascii_serial = None;
        for off in [54usize, 72, 90, 108] {
            let d = &edid[off..off + 18];
            if d[0] == 0 && d[1] == 0 && d[2] == 0 && d[4] == 0 && d[3] == 0xFF {
                let text = decode_descriptor_text(&d[5..18]);
                if !text.is_empty() {
                    ascii_serial = Some(text);
                }
            }
        }

        Ok(Self {
            pnp_manufacturer,
            product_code,
            numeric_serial,
            ascii_serial,
            week,
            year,
            raw_sha256: Some(sha256_hex(edid)),
        })
    }

    /// Build a fingerprint from already-parsed identity parts — for backends (e.g. ddc-hi on
    /// Windows) that expose parsed fields but not the raw EDID block.
    pub fn from_parts(
        pnp_manufacturer: impl Into<String>,
        product_code: u16,
        numeric_serial: u32,
        ascii_serial: Option<String>,
    ) -> Self {
        Self {
            pnp_manufacturer: pnp_manufacturer.into(),
            product_code,
            numeric_serial,
            ascii_serial,
            week: 0,
            year: 0,
            raw_sha256: None,
        }
    }

    /// Stable cross-PC id: first 12 hex chars of SHA-256 over the composite identity fields.
    ///
    /// `week`/`year` and the raw-EDID hash are intentionally **excluded**: a backend that exposes
    /// only parsed identity (e.g. ddc-hi on Windows, via [`from_parts`]) cannot supply manufacture
    /// week/year, so including them would give the same physical panel a different id depending on
    /// which OS/backend enumerated it — fragmenting calibration and ownership across a mixed-OS
    /// mesh (the normal case). The id therefore covers only fields every backend can produce:
    /// manufacturer, product code, numeric serial, and ASCII serial.
    ///
    /// [`from_parts`]: MonitorFingerprint::from_parts
    pub fn monitor_id(&self) -> String {
        let composite = format!(
            "{}|{}|{}|{}",
            self.pnp_manufacturer,
            self.product_code,
            self.numeric_serial,
            self.ascii_serial.as_deref().unwrap_or(""),
        );
        sha256_hex(composite.as_bytes())[..12].to_string()
    }

    /// True when this fingerprint carries no per-unit serial (numeric 0 AND no/empty ASCII
    /// serial), so two identical-model panels collide and require user labeling.
    pub fn is_ambiguous(&self) -> bool {
        self.numeric_serial == 0 && self.ascii_serial.as_deref().is_none_or(str::is_empty)
    }
}

/// Decode the 2-byte packed EDID manufacturer id into 3 letters.
fn decode_pnp(b0: u8, b1: u8) -> String {
    let v = ((b0 as u16) << 8) | b1 as u16;
    [(v >> 10) & 0x1F, (v >> 5) & 0x1F, v & 0x1F]
        .into_iter()
        .map(|n| {
            let n = n as u8;
            if (1..=26).contains(&n) {
                (b'A' + n - 1) as char
            } else {
                '?'
            }
        })
        .collect()
}

/// EDID descriptor text is space-padded and terminated by 0x0A. Per the EDID spec it is 7-bit
/// ASCII; anything outside printable ASCII is dropped rather than mapped to arbitrary `char`s, so a
/// garbled/virtualized descriptor can't inject control characters into a serial/label.
fn decode_descriptor_text(bytes: &[u8]) -> String {
    let mut s = String::new();
    for &b in bytes {
        if b == 0x0A {
            break;
        }
        if b.is_ascii_graphic() || b == b' ' {
            s.push(b as char);
        }
    }
    s.trim_end().to_string()
}

fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut out = String::with_capacity(digest.len() * 2);
    for b in digest {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid 128-byte EDID for "AOC", product 0x1234, given numeric serial,
    /// week 10 / year 2021, and an optional ASCII serial descriptor.
    fn sample_edid(numeric_serial: u32, ascii_serial: Option<&str>) -> Vec<u8> {
        let mut e = vec![0u8; 128];
        e[0..8].copy_from_slice(&EDID_HEADER);
        // "AOC" -> A=1,O=15,C=3 -> 0x05E3
        e[8] = 0x05;
        e[9] = 0xE3;
        e[10..12].copy_from_slice(&0x1234u16.to_le_bytes());
        e[12..16].copy_from_slice(&numeric_serial.to_le_bytes());
        e[16] = 10; // week
        e[17] = (2021 - 1990) as u8; // year byte
        if let Some(s) = ascii_serial {
            let off = 54; // first descriptor
            e[off + 3] = 0xFF; // serial-number descriptor tag
            let bytes = s.as_bytes();
            let n = bytes.len().min(12);
            e[off + 5..off + 5 + n].copy_from_slice(&bytes[..n]);
            if off + 5 + n < off + 18 {
                e[off + 5 + n] = 0x0A; // terminator
            }
        }
        e
    }

    #[test]
    fn parses_core_fields() {
        let fp = MonitorFingerprint::from_edid(&sample_edid(1598, None)).unwrap();
        assert_eq!(fp.pnp_manufacturer, "AOC");
        assert_eq!(fp.product_code, 0x1234);
        assert_eq!(fp.numeric_serial, 1598);
        assert_eq!(fp.week, 10);
        assert_eq!(fp.year, 2021);
        assert_eq!(fp.ascii_serial, None);
        assert!(fp.raw_sha256.is_some());
    }

    #[test]
    fn parses_ascii_serial_descriptor() {
        let fp = MonitorFingerprint::from_edid(&sample_edid(0, Some("ABC123"))).unwrap();
        assert_eq!(fp.ascii_serial.as_deref(), Some("ABC123"));
    }

    #[test]
    fn rejects_bad_header_and_short_input() {
        let mut bad = sample_edid(1, None);
        bad[0] = 0x12;
        assert_eq!(
            MonitorFingerprint::from_edid(&bad),
            Err(EdidError::BadHeader)
        );
        assert_eq!(
            MonitorFingerprint::from_edid(&[0u8; 10]),
            Err(EdidError::TooShort)
        );
    }

    #[test]
    fn monitor_id_is_stable_and_serial_sensitive() {
        let a = MonitorFingerprint::from_edid(&sample_edid(1598, None)).unwrap();
        let b = MonitorFingerprint::from_edid(&sample_edid(1598, None)).unwrap();
        let c = MonitorFingerprint::from_edid(&sample_edid(9999, None)).unwrap();
        assert_eq!(a.monitor_id(), b.monitor_id());
        assert_ne!(a.monitor_id(), c.monitor_id());
        assert_eq!(a.monitor_id().len(), 12);
    }

    #[test]
    fn ambiguity_tracks_serial_presence() {
        assert!(MonitorFingerprint::from_edid(&sample_edid(0, None))
            .unwrap()
            .is_ambiguous());
        assert!(!MonitorFingerprint::from_edid(&sample_edid(1598, None))
            .unwrap()
            .is_ambiguous());
        assert!(!MonitorFingerprint::from_edid(&sample_edid(0, Some("S1")))
            .unwrap()
            .is_ambiguous());
    }

    #[test]
    fn from_parts_matches_edid_identity() {
        // Same identity tuple via parts vs EDID -> SAME monitor_id, even though from_parts cannot
        // supply week/year/raw-EDID. This is the cross-backend stability the join key requires:
        // a Windows (parts) peer and a Linux (raw EDID) peer must agree on the id for one panel.
        let via_edid = MonitorFingerprint::from_edid(&sample_edid(1598, None)).unwrap();
        let via_parts = MonitorFingerprint::from_parts("AOC", 0x1234, 1598, None);
        assert_eq!(via_parts.pnp_manufacturer, via_edid.pnp_manufacturer);
        assert_eq!(via_parts.numeric_serial, via_edid.numeric_serial);
        assert!(via_parts.raw_sha256.is_none());
        assert_eq!(
            via_parts.monitor_id(),
            via_edid.monitor_id(),
            "the same panel must get the same id regardless of backend"
        );
    }

    #[test]
    fn monitor_id_ignores_week_year_for_cross_backend_stability() {
        let with_date = MonitorFingerprint::from_edid(&sample_edid(1598, None)).unwrap();
        let mut no_date = with_date.clone();
        no_date.week = 0;
        no_date.year = 0;
        assert_eq!(with_date.monitor_id(), no_date.monitor_id());
    }
}
