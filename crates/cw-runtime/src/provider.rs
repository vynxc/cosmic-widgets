use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Minimal change sent from a shared provider to widget documents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DataPatch {
    pub path: String,
    pub value: Value,
}

/// Shared source tree for declarative package bindings.
#[derive(Debug, Default)]
pub struct ProviderRegistry {
    roots: BTreeMap<String, Value>,
    revisions: BTreeMap<String, u64>,
}

impl ProviderRegistry {
    /// Replaces one provider root and advances its revision only when data changed.
    pub fn update(&mut self, provider: impl Into<String>, value: Value) -> Option<DataPatch> {
        let provider = provider.into();
        if self.roots.get(&provider) == Some(&value) {
            return None;
        }
        self.roots.insert(provider.clone(), value.clone());
        *self.revisions.entry(provider.clone()).or_default() += 1;
        Some(DataPatch {
            path: provider,
            value,
        })
    }

    /// Reads the combined JSON tree consumed by binding resolution and plugins.
    pub fn snapshot(&self) -> Value {
        Value::Object(
            self.roots
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect::<Map<_, _>>(),
        )
    }

    /// Returns the monotonic revision for a provider.
    pub fn revision(&self, provider: &str) -> u64 {
        self.revisions.get(provider).copied().unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn update_should_not_advance_unchanged_provider() {
        let mut registry = ProviderRegistry::default();
        let _ = registry.update("clock", json!({"time": "10:00"}));
        let patch = registry.update("clock", json!({"time": "10:00"}));
        assert!(patch.is_none());
    }

    #[test]
    fn snapshot_should_combine_provider_roots() {
        let mut registry = ProviderRegistry::default();
        let _ = registry.update("clock", json!({"time": "10:00"}));
        assert_eq!(registry.snapshot()["clock"]["time"], "10:00");
    }
}
