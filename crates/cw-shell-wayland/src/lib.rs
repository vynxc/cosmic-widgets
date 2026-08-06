//! COSMIC layer-shell policy and Wayland connection probing.

mod gpu;
mod live;

use std::env;

use cw_core::{Edge, LayerPolicy, Rect};
use thiserror::Error;
use wayland_client::Connection;

pub use gpu::{GpuError, GpuHub, GpuSurface};
pub use live::{DesktopWidgetConfig, HtmlProvider, run_desktop_widget};

/// Wayland session facts used by `cosmic-widgets doctor`.
#[derive(Debug, Clone)]
pub struct WaylandProbe {
    pub display: String,
    pub desktop: Option<String>,
    pub connected: bool,
}

/// Failure to establish the required client connection.
#[derive(Debug, Error)]
pub enum ShellError {
    #[error("WAYLAND_DISPLAY is not set")]
    MissingDisplay,
    #[error("unable to connect to the Wayland compositor: {0}")]
    Connection(String),
    #[error("renderer initialization failed: {0}")]
    Renderer(String),
}

/// Connects to the active session without creating surfaces.
///
/// # Errors
///
/// Returns [`ShellError::MissingDisplay`] outside a Wayland session or
/// [`ShellError::Connection`] when the compositor socket cannot be opened.
pub fn probe_session() -> Result<WaylandProbe, ShellError> {
    let display = env::var("WAYLAND_DISPLAY").map_err(|_| ShellError::MissingDisplay)?;
    Connection::connect_to_env().map_err(|error| ShellError::Connection(error.to_string()))?;
    Ok(WaylandProbe {
        display,
        desktop: env::var("XDG_CURRENT_DESKTOP").ok(),
        connected: true,
    })
}

/// Complete key for grouping widgets onto a shared layer surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SurfaceGroup {
    pub edge: Option<Edge>,
    pub policy: LayerPolicy,
}

impl SurfaceGroup {
    /// Desktop widgets share a non-exclusive bottom-layer surface.
    pub const fn desktop() -> Self {
        Self {
            edge: None,
            policy: LayerPolicy::Overlay,
        }
    }
}

/// Computes the smallest integer input region containing interactive logical bounds.
pub fn input_region(rectangles: &[Rect], scale: f64) -> Vec<(i32, i32, i32, i32)> {
    rectangles
        .iter()
        .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
        .map(|rect| {
            let x = physical_coordinate((rect.x * scale).floor());
            let y = physical_coordinate((rect.y * scale).floor());
            let right = physical_coordinate(((rect.x + rect.width) * scale).ceil());
            let bottom = physical_coordinate(((rect.y + rect.height) * scale).ceil());
            (x, y, right - x, bottom - y)
        })
        .collect()
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is made finite and clamped to the Wayland i32 coordinate range first"
)]
fn physical_coordinate(value: f64) -> i32 {
    if !value.is_finite() {
        return 0;
    }
    value.clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_region_should_round_outwards_at_fractional_scale() {
        let rectangles = [Rect {
            x: 0.5,
            y: 1.0,
            width: 10.0,
            height: 5.0,
        }];
        assert_eq!(input_region(&rectangles, 1.25), vec![(0, 1, 14, 7)]);
    }

    #[test]
    fn input_region_should_omit_empty_rectangles() {
        let rectangles = [Rect {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 10.0,
        }];
        assert!(input_region(&rectangles, 1.0).is_empty());
    }
}
