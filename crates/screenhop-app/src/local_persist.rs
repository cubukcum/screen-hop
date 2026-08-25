//! Versioned, local-only configuration and crash-safe persistence.
//!
//! A missing config file means "not configured yet" and loads [`LocalConfig::default`]. An
//! existing file is treated more strictly: all schema fields must be present, the schema version
//! must be supported, and the two confirmed input values must be distinct. This keeps switching
//! fail-closed when a file is truncated, hand-edited incorrectly, or belongs to another version.

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Deserializer, Serialize};

pub const CONFIG_FILE: &str = "config.json";
pub const LOCAL_CONFIG_VERSION: u32 = 2;

/// One of the two locally configured monitor inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceSlot {
    A,
    B,
}

impl SourceSlot {
    pub const ALL: [Self; 2] = [Self::A, Self::B];

    pub const fn index(self) -> usize {
        match self {
            Self::A => 0,
            Self::B => 1,
        }
    }

    pub const fn opposite(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// User-facing label and the DDC/CI input value confirmed for one source.
///
/// `None` deliberately means unconfirmed/unconfigured; it must never be written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SourceConfig {
    pub label: String,
    pub confirmed_value: Option<u16>,
}

impl SourceConfig {
    pub fn new(label: impl Into<String>, confirmed_value: Option<u16>) -> Self {
        Self {
            label: label.into(),
            confirmed_value,
        }
    }
}

/// Wrapper used only while deserializing. Unlike `Option<T>`, the wrapper itself is required, so
/// JSON must contain the field while still allowing its value to be `null`.
struct RequiredNullable<T> {
    value: Option<T>,
    present: bool,
}

impl<T> Default for RequiredNullable<T> {
    fn default() -> Self {
        Self {
            value: None,
            present: false,
        }
    }
}

impl<T> RequiredNullable<T> {
    fn required<E: serde::de::Error>(self, field: &'static str) -> Result<Option<T>, E> {
        if self.present {
            Ok(self.value)
        } else {
            Err(E::missing_field(field))
        }
    }
}

impl<'de, T> Deserialize<'de> for RequiredNullable<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<T>::deserialize(deserializer).map(|value| Self {
            value,
            present: true,
        })
    }
}

impl<'de> Deserialize<'de> for SourceConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            label: String,
            #[serde(default)]
            confirmed_value: RequiredNullable<u16>,
        }

        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            label: wire.label,
            confirmed_value: wire.confirmed_value.required("confirmed_value")?,
        })
    }
}

/// Complete local application state persisted in one versioned JSON document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalConfig {
    pub version: u32,
    /// Stable id understood by the local [`screenhop_core::MonitorDriver`].
    pub selected_monitor: Option<String>,
    /// Normalized manufacturer/model token used only for model-wide quirk lookup, never for
    /// selecting a DDC handle.
    pub selected_monitor_model_token: Option<String>,
    /// Exactly two slots. Serde rejects arrays with fewer or more entries.
    pub sources: [SourceConfig; 2],
    /// Stable/driver monitor id -> friendly user-facing name.
    pub monitor_aliases: HashMap<String, String>,
    /// Most recent source whose switch was an effective success. Used only when an inactive input
    /// makes live input read-back unavailable.
    pub last_requested: Option<SourceSlot>,
}

impl Default for LocalConfig {
    fn default() -> Self {
        Self {
            version: LOCAL_CONFIG_VERSION,
            selected_monitor: None,
            selected_monitor_model_token: None,
            sources: [
                SourceConfig::new("Source A", None),
                SourceConfig::new("Source B", None),
            ],
            monitor_aliases: HashMap::new(),
            last_requested: None,
        }
    }
}

impl<'de> Deserialize<'de> for LocalConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            version: u32,
            #[serde(default)]
            selected_monitor: RequiredNullable<String>,
            #[serde(default)]
            selected_monitor_model_token: RequiredNullable<String>,
            sources: [SourceConfig; 2],
            monitor_aliases: HashMap<String, String>,
            #[serde(default)]
            last_requested: RequiredNullable<SourceSlot>,
        }

        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            version: wire.version,
            selected_monitor: wire.selected_monitor.required("selected_monitor")?,
            selected_monitor_model_token: wire
                .selected_monitor_model_token
                .required("selected_monitor_model_token")?,
            sources: wire.sources,
            monitor_aliases: wire.monitor_aliases,
            last_requested: wire.last_requested.required("last_requested")?,
        })
    }
}

impl LocalConfig {
    pub fn source(&self, slot: SourceSlot) -> &SourceConfig {
        &self.sources[slot.index()]
    }

    pub fn source_mut(&mut self, slot: SourceSlot) -> &mut SourceConfig {
        &mut self.sources[slot.index()]
    }

    /// The pair of confirmed values, in A/B order. Returns `None` until both are configured.
    pub fn confirmed_values(&self) -> Option<[u16; 2]> {
        Some([
            self.source(SourceSlot::A).confirmed_value?,
            self.source(SourceSlot::B).confirmed_value?,
        ])
    }

    /// True only when switching has every required local input: a monitor, two distinct values,
    /// and a structurally valid configuration.
    pub fn is_ready(&self) -> bool {
        self.validate().is_ok()
            && self
                .selected_monitor
                .as_deref()
                .is_some_and(|monitor| !monitor.trim().is_empty())
            && self.confirmed_values().is_some()
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.version != LOCAL_CONFIG_VERSION {
            return Err(ConfigError::UnsupportedVersion {
                found: self.version,
                supported: LOCAL_CONFIG_VERSION,
            });
        }

        if self
            .selected_monitor
            .as_deref()
            .is_some_and(|monitor| monitor.trim().is_empty())
        {
            return Err(ConfigError::BlankSelectedMonitor);
        }

        if self
            .selected_monitor_model_token
            .as_deref()
            .is_some_and(|token| token.trim().is_empty())
        {
            return Err(ConfigError::BlankSelectedMonitorModelToken);
        }

        for slot in SourceSlot::ALL {
            if self.source(slot).label.trim().is_empty() {
                return Err(ConfigError::BlankSourceLabel(slot));
            }
        }

        if let Some([a, b]) = self.confirmed_values() {
            if a == b {
                return Err(ConfigError::DuplicateConfirmedValue(a));
            }
        }

        for (monitor_id, alias) in &self.monitor_aliases {
            if monitor_id.trim().is_empty() {
                return Err(ConfigError::BlankMonitorAliasId);
            }
            if alias.trim().is_empty() {
                return Err(ConfigError::BlankMonitorAlias(monitor_id.clone()));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    UnsupportedVersion { found: u32, supported: u32 },
    BlankSelectedMonitor,
    BlankSelectedMonitorModelToken,
    BlankSourceLabel(SourceSlot),
    DuplicateConfirmedValue(u16),
    BlankMonitorAliasId,
    BlankMonitorAlias(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedVersion { found, supported } => write!(
                formatter,
                "unsupported local config version {found}; supported version is {supported}"
            ),
            Self::BlankSelectedMonitor => formatter.write_str("selected monitor cannot be blank"),
            Self::BlankSelectedMonitorModelToken => {
                formatter.write_str("selected monitor model token cannot be blank")
            }
            Self::BlankSourceLabel(slot) => {
                write!(formatter, "source {slot:?} label cannot be blank")
            }
            Self::DuplicateConfirmedValue(value) => write!(
                formatter,
                "source A and B must use distinct input values (both are {value})"
            ),
            Self::BlankMonitorAliasId => formatter.write_str("monitor alias id cannot be blank"),
            Self::BlankMonitorAlias(monitor_id) => {
                write!(
                    formatter,
                    "monitor alias for {monitor_id:?} cannot be blank"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

/// The per-user config directory, honoring `SCREENHOP_CONFIG_DIR` when set.
pub fn default_config_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("SCREENHOP_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("", "", "screen-hop").map(|dirs| dirs.config_dir().to_path_buf())
}

/// Resolve and create the config directory.
pub fn ensure_config_dir() -> io::Result<PathBuf> {
    let dir = default_config_dir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no config directory available"))?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Write through a same-directory temporary file, then atomically rename it into place.
pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temporary = path.with_extension("tmp");
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, path)
}

pub fn save_config(dir: &Path, config: &LocalConfig) -> io::Result<()> {
    config
        .validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    fs::create_dir_all(dir)?;
    let bytes = serde_json::to_vec_pretty(config)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    atomic_write(&dir.join(CONFIG_FILE), &bytes)
}

pub fn load_config(dir: &Path) -> io::Result<LocalConfig> {
    let path = dir.join(CONFIG_FILE);
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(LocalConfig::default()),
        Err(error) => return Err(error),
    };

    let config: LocalConfig = serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    config
        .validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "screenhop-local-persist-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn atomic_write_replaces_without_a_partial_sidecar() {
        let dir = temp_dir();
        let path = dir.join(CONFIG_FILE);
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"second");
        assert!(!dir.join("config.tmp").exists());
    }

    #[test]
    fn array_shape_and_nullable_fields_are_required() {
        for json in [
            r#"{"version":2,"selected_monitor":null,"selected_monitor_model_token":null,"sources":[],"monitor_aliases":{},"last_requested":null}"#,
            r#"{"version":2,"sources":[{"label":"A","confirmed_value":null},{"label":"B","confirmed_value":null}],"monitor_aliases":{},"last_requested":null}"#,
            r#"{"version":2,"selected_monitor":null,"selected_monitor_model_token":null,"sources":[{"label":"A"},{"label":"B","confirmed_value":null}],"monitor_aliases":{},"last_requested":null}"#,
            r#"{"version":2,"selected_monitor":null,"sources":[{"label":"A","confirmed_value":null},{"label":"B","confirmed_value":null}],"monitor_aliases":{},"last_requested":null}"#,
        ] {
            assert!(
                serde_json::from_str::<LocalConfig>(json).is_err(),
                "incomplete config unexpectedly loaded: {json}"
            );
        }
    }

    #[test]
    fn save_rejects_invalid_config_without_touching_disk() {
        let dir = temp_dir();
        let mut config = LocalConfig::default();
        config.source_mut(SourceSlot::A).confirmed_value = Some(0x0f);
        config.source_mut(SourceSlot::B).confirmed_value = Some(0x0f);
        let error = save_config(&dir, &config).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(!dir.join(CONFIG_FILE).exists());
    }

    #[test]
    fn blank_selected_monitor_model_token_is_invalid() {
        let config = LocalConfig {
            selected_monitor_model_token: Some("   ".to_owned()),
            ..LocalConfig::default()
        };

        assert_eq!(
            config.validate(),
            Err(ConfigError::BlankSelectedMonitorModelToken)
        );
    }
}
