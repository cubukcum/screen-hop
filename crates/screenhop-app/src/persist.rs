//! On-disk persistence for the agent's state (the "finish the agent" work).
//!
//! Everything the agent must remember across restarts lives in one per-user config directory:
//! the peer identity, the mesh secret, TOFU pins, per-`(peer,monitor)` calibration, monitor labels,
//! and a small config file. Writes are **atomic** (temp file + rename) so a crash mid-write never
//! corrupts a file.
//!
//! Security note: the mesh secret and identity key are stored in plaintext under the user's profile
//! today. Wrapping them with the OS keystore (Windows DPAPI) is a follow-up — see SECURITY.md.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use screenhop_identity::CalibrationStore;
use screenhop_net::PeerIdentity;
use serde::{de::DeserializeOwned, Deserialize, Serialize};

pub const CALIBRATION_FILE: &str = "calibration.json";
pub const LABELS_FILE: &str = "labels.json";
pub const IDENTITY_FILE: &str = "identity.key";
pub const SECRET_FILE: &str = "mesh-secret";
pub const CONFIG_FILE: &str = "config.json";
pub const PINS_FILE: &str = "pins.json";

/// Agent settings that aren't secret (safe to read/log/share).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub port: u16,
    pub name: String,
    pub can_actuate: bool,
    #[serde(default)]
    pub manual_hosts: Vec<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            port: 7777,
            name: "screen-hop".into(),
            can_actuate: true,
            manual_hosts: Vec::new(),
        }
    }
}

/// The per-user config directory, honoring `$SCREENHOP_CONFIG_DIR` if set (used by tests and power
/// users), otherwise the OS-standard location (`%APPDATA%\screen-hop` on Windows, etc.).
pub fn default_config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SCREENHOP_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("", "", "screen-hop").map(|d| d.config_dir().to_path_buf())
}

/// Resolve and create the config directory, returning its path.
pub fn ensure_config_dir() -> io::Result<PathBuf> {
    let dir = default_config_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config directory available"))?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Atomic write: write to `<path>.tmp`, then rename over `path`. `std::fs::rename` replaces the
/// destination on every supported platform, so `path` is never left half-written after a crash.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

fn read_opt(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(b) => Ok(Some(b)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Serialize `value` as pretty JSON and write it atomically.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    atomic_write(path, &bytes)
}

/// Load JSON from `path`, or `None` if the file doesn't exist (a corrupt file is an error).
pub fn load_json<T: DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    match read_opt(path)? {
        Some(b) => {
            Ok(Some(serde_json::from_slice(&b).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, e)
            })?))
        }
        None => Ok(None),
    }
}

// ---- typed helpers (all take the config directory) --------------------------

pub fn save_calibration(dir: &Path, store: &CalibrationStore) -> io::Result<()> {
    save_json(&dir.join(CALIBRATION_FILE), store)
}
pub fn load_calibration(dir: &Path) -> io::Result<CalibrationStore> {
    Ok(load_json(&dir.join(CALIBRATION_FILE))?.unwrap_or_default())
}

pub fn save_labels(dir: &Path, labels: &HashMap<String, String>) -> io::Result<()> {
    save_json(&dir.join(LABELS_FILE), labels)
}
pub fn load_labels(dir: &Path) -> io::Result<HashMap<String, String>> {
    Ok(load_json(&dir.join(LABELS_FILE))?.unwrap_or_default())
}

pub fn save_config(dir: &Path, cfg: &AgentConfig) -> io::Result<()> {
    save_json(&dir.join(CONFIG_FILE), cfg)
}
pub fn load_config(dir: &Path) -> io::Result<AgentConfig> {
    Ok(load_json(&dir.join(CONFIG_FILE))?.unwrap_or_default())
}

/// Path to the persisted TOFU pin store (the agent passes this to `Node::with_pin_store`).
pub fn pins_path(dir: &Path) -> PathBuf {
    dir.join(PINS_FILE)
}

/// The mesh secret (group passphrase), if one has been saved.
pub fn load_secret(dir: &Path) -> io::Result<Option<String>> {
    Ok(read_opt(&dir.join(SECRET_FILE))?
        .map(|b| String::from_utf8_lossy(&b).trim().to_string())
        .filter(|s| !s.is_empty()))
}
pub fn save_secret(dir: &Path, secret: &str) -> io::Result<()> {
    atomic_write(&dir.join(SECRET_FILE), secret.trim().as_bytes())
}

/// Load this install's persisted peer identity, or `None` if not created yet / malformed.
pub fn load_identity(dir: &Path) -> io::Result<Option<PeerIdentity>> {
    match read_opt(&dir.join(IDENTITY_FILE))? {
        Some(b) if b.len() == 32 => {
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&b);
            Ok(Some(PeerIdentity::from_secret_bytes(&arr)))
        }
        _ => Ok(None),
    }
}
pub fn save_identity(dir: &Path, id: &PeerIdentity) -> io::Result<()> {
    atomic_write(&dir.join(IDENTITY_FILE), &*id.secret_bytes())
}
/// Load the existing identity or generate + persist a fresh one (stable across restarts — D2).
pub fn load_or_create_identity(dir: &Path) -> io::Result<PeerIdentity> {
    if let Some(id) = load_identity(dir)? {
        return Ok(id);
    }
    let id = PeerIdentity::generate();
    save_identity(dir, &id)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A unique, freshly-created temp directory for one test.
    fn tmp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "screenhop-persist-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_overwrites_cleanly() {
        let dir = tmp_dir();
        let p = dir.join("f.bin");
        atomic_write(&p, b"first").unwrap();
        atomic_write(&p, b"second").unwrap();
        assert_eq!(fs::read(&p).unwrap(), b"second");
        // The temp sidecar must not linger.
        assert!(!p.with_extension("tmp").exists());
    }

    #[test]
    fn calibration_round_trips() {
        let dir = tmp_dir();
        let mut store = CalibrationStore::new();
        store.record("peerA", "mon1", 0x0F);
        save_calibration(&dir, &store).unwrap();
        let loaded = load_calibration(&dir).unwrap();
        assert_eq!(loaded.confirmed_value("peerA", "mon1"), Some(0x0F));
        assert_eq!(loaded.owner_for("mon1", 0x0F).as_deref(), Some("peerA"));
    }

    #[test]
    fn labels_and_config_round_trip() {
        let dir = tmp_dir();
        let mut labels = HashMap::new();
        labels.insert("mon1".to_string(), "Center 32\"".to_string());
        save_labels(&dir, &labels).unwrap();
        assert_eq!(
            load_labels(&dir).unwrap().get("mon1").map(String::as_str),
            Some("Center 32\"")
        );

        let cfg = AgentConfig {
            port: 9000,
            name: "Desk".into(),
            can_actuate: false,
            manual_hosts: vec!["10.0.0.5:7777".into()],
        };
        save_config(&dir, &cfg).unwrap();
        let loaded = load_config(&dir).unwrap();
        assert_eq!(loaded.port, 9000);
        assert_eq!(loaded.manual_hosts, vec!["10.0.0.5:7777".to_string()]);
    }

    #[test]
    fn missing_files_load_as_defaults() {
        let dir = tmp_dir();
        assert!(load_calibration(&dir)
            .unwrap()
            .confirmed_value("x", "y")
            .is_none());
        assert!(load_labels(&dir).unwrap().is_empty());
        assert_eq!(load_config(&dir).unwrap().port, 7777); // default
        assert!(load_secret(&dir).unwrap().is_none());
        assert!(load_identity(&dir).unwrap().is_none());
    }

    #[test]
    fn identity_is_stable_across_load_or_create() {
        let dir = tmp_dir();
        let first = load_or_create_identity(&dir).unwrap().peer_id();
        let second = load_or_create_identity(&dir).unwrap().peer_id(); // loads the persisted one
        assert_eq!(first, second);
    }

    #[test]
    fn secret_round_trips_and_trims() {
        let dir = tmp_dir();
        save_secret(&dir, "  hunter2  ").unwrap();
        assert_eq!(load_secret(&dir).unwrap().as_deref(), Some("hunter2"));
    }
}
