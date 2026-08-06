use semver::Version;
use serde::{Deserialize, Serialize};

use crate::{Capability, PACKAGE_SCHEMA};

/// Logical widget size in compositor-independent pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// Placement families supported by a package.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlacementKind {
    Desktop,
    Edge,
}

/// How often the host should wake a declarative package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum RefreshPolicy {
    #[default]
    OnChange,
    Interval {
        milliseconds: u64,
    },
}

/// Public `widget.toml` schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(default = "default_schema")]
    pub schema: u32,
    pub id: String,
    pub version: Version,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    pub default_size: Size,
    pub min_size: Size,
    pub max_size: Size,
    #[serde(default = "default_placements")]
    pub placements: Vec<PlacementKind>,
    #[serde(default)]
    pub refresh: RefreshPolicy,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub wasm: Option<WasmManifest>,
}

/// Optional Extism module description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WasmManifest {
    #[serde(default = "default_wasm_entry")]
    pub module: String,
    #[serde(default = "default_memory_pages")]
    pub memory_pages: u32,
    #[serde(default = "default_fuel")]
    pub fuel: u64,
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

impl Manifest {
    /// Validates invariants that do not require reading package files.
    ///
    /// # Errors
    ///
    /// Returns a human-readable validation message when an identifier, path, size,
    /// refresh interval, or WASM resource limit is outside the v1 schema.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != PACKAGE_SCHEMA {
            return Err(format!(
                "unsupported schema {}, expected {PACKAGE_SCHEMA}",
                self.schema
            ));
        }
        validate_id(&self.id)?;
        if self.name.trim().is_empty() {
            return Err("name must not be empty".into());
        }
        if self.entry.starts_with('/') || self.entry.contains("..") {
            return Err("entry must be a package-relative path".into());
        }
        validate_size_range(self.min_size, self.default_size, self.max_size)?;
        if self.placements.is_empty() {
            return Err("at least one placement is required".into());
        }
        if let RefreshPolicy::Interval { milliseconds } = self.refresh
            && milliseconds < 250
        {
            return Err("refresh intervals below 250 ms are not allowed".into());
        }
        if let Some(wasm) = &self.wasm {
            if wasm.module.starts_with('/') || wasm.module.contains("..") {
                return Err("WASM module must be a package-relative path".into());
            }
            if !(1..=1024).contains(&wasm.memory_pages) {
                return Err("WASM memory_pages must be between 1 and 1024".into());
            }
            if !(10_000..=100_000_000).contains(&wasm.fuel) {
                return Err("WASM fuel must be between 10,000 and 100,000,000".into());
            }
            if !(1..=1_000).contains(&wasm.timeout_ms) {
                return Err("WASM timeout_ms must be between 1 and 1000".into());
            }
        }
        Ok(())
    }
}

fn default_schema() -> u32 {
    PACKAGE_SCHEMA
}

fn default_entry() -> String {
    "index.html".into()
}

fn default_placements() -> Vec<PlacementKind> {
    vec![PlacementKind::Desktop]
}

fn default_wasm_entry() -> String {
    "logic.wasm".into()
}

const fn default_memory_pages() -> u32 {
    256
}

const fn default_fuel() -> u64 {
    5_000_000
}

const fn default_timeout_ms() -> u64 {
    50
}

fn validate_id(id: &str) -> Result<(), String> {
    let segments: Vec<_> = id.split('.').collect();
    if segments.len() < 3
        || segments.iter().any(|segment| {
            segment.is_empty()
                || !segment
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
    {
        return Err("id must be a reverse-DNS identifier".into());
    }
    Ok(())
}

fn validate_size_range(min: Size, default: Size, max: Size) -> Result<(), String> {
    if min.width == 0 || min.height == 0 {
        return Err("minimum size must be non-zero".into());
    }
    if min.width > default.width
        || min.height > default.height
        || default.width > max.width
        || default.height > max.height
    {
        return Err("size must satisfy min <= default <= max".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest() -> Manifest {
        Manifest {
            schema: PACKAGE_SCHEMA,
            id: "io.github.vynxc.Clock".into(),
            version: Version::new(0, 1, 0),
            name: "Clock".into(),
            description: String::new(),
            entry: "index.html".into(),
            default_size: Size {
                width: 320,
                height: 180,
            },
            min_size: Size {
                width: 160,
                height: 90,
            },
            max_size: Size {
                width: 640,
                height: 360,
            },
            placements: vec![PlacementKind::Desktop],
            refresh: RefreshPolicy::OnChange,
            capabilities: Vec::new(),
            wasm: None,
        }
    }

    #[test]
    fn validate_should_accept_well_formed_manifest() {
        assert!(manifest().validate().is_ok());
    }

    #[test]
    fn validate_should_reject_inverted_size_range() {
        let mut value = manifest();
        value.min_size.width = 500;
        assert!(value.validate().is_err());
    }
}
