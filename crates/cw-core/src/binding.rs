use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Supported declarative DOM operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BindingKind {
    Text,
    Class { name: String },
    Style { property: String },
    Action { event: String },
}

/// A compiled `data-cw-*` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Binding {
    pub node: u64,
    pub kind: BindingKind,
    pub path: String,
    #[serde(default)]
    pub filters: Vec<String>,
}

/// Scalar value accepted by a declarative patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BindingValue {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
}

impl BindingValue {
    /// Converts JSON data into a DOM-safe scalar.
    pub fn from_json(value: &Value) -> Option<Self> {
        match value {
            Value::Null => Some(Self::Null),
            Value::Bool(value) => Some(Self::Bool(*value)),
            Value::Number(value) => value.as_f64().map(Self::Number),
            Value::String(value) => Some(Self::Text(value.clone())),
            Value::Array(_) | Value::Object(_) => None,
        }
    }
}

/// Resolves a dotted path without evaluating arbitrary expressions.
pub fn resolve_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(root, |value, segment| value.get(segment))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn resolve_path_should_return_nested_value() {
        let root = json!({"system": {"cpu": 42.5}});
        assert_eq!(resolve_path(&root, "system.cpu"), Some(&json!(42.5)));
    }

    #[test]
    fn binding_value_should_reject_objects() {
        assert!(BindingValue::from_json(&json!({"nested": true})).is_none());
    }
}
