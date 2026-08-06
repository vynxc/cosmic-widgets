use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Logical rectangle used for placement and input regions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    /// Returns whether a logical point is inside the rectangle.
    pub fn contains(self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    /// Constrains the rectangle to the provided output bounds.
    #[must_use]
    pub fn clamp_to(self, bounds: Self) -> Self {
        let width = self.width.min(bounds.width).max(1.0);
        let height = self.height.min(bounds.height).max(1.0);
        let x = self.x.clamp(bounds.x, bounds.x + bounds.width - width);
        let y = self.y.clamp(bounds.y, bounds.y + bounds.height - height);
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Physical edge used by edge widget groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    Top,
    Right,
    Bottom,
    Left,
}

/// Layer-shell reservation and visibility behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayerPolicy {
    Overlay,
    Reserve,
    AutoHide,
}

/// Persisted placement for a widget instance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "kind")]
pub enum Placement {
    Desktop {
        bounds: Rect,
    },
    Edge {
        edge: Edge,
        policy: LayerPolicy,
        offset: f64,
        extent: f64,
        thickness: f64,
    },
}

/// Persisted instance state. Runtime state and capability grants live elsewhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WidgetInstance {
    pub id: Uuid,
    pub package_id: String,
    pub output: String,
    pub placement: Placement,
    #[serde(default)]
    pub locked: bool,
    #[serde(default = "default_opacity")]
    pub opacity: f32,
    #[serde(default = "default_visible")]
    pub visible: bool,
    #[serde(default)]
    pub z_index: i32,
}

/// Versioned user layout document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub version: u32,
    #[serde(default)]
    pub instances: Vec<WidgetInstance>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            version: 1,
            instances: Vec::new(),
        }
    }
}

const fn default_opacity() -> f32 {
    1.0
}

const fn default_visible() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_to_should_keep_rectangle_inside_output() {
        let rect = Rect {
            x: 900.0,
            y: 900.0,
            width: 300.0,
            height: 300.0,
        };
        let bounds = Rect {
            x: 0.0,
            y: 0.0,
            width: 1000.0,
            height: 1000.0,
        };
        assert!((rect.clamp_to(bounds).x - 700.0).abs() < f64::EPSILON);
    }

    #[test]
    fn contains_should_exclude_right_edge() {
        let rect = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 50.0,
        };
        assert!(!rect.contains(100.0, 10.0));
    }
}
