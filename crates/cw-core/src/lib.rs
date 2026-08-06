//! Stable package, layout, capability, binding, and theme types for cosmic-widgets.

mod binding;
mod capability;
mod layout;
mod manifest;
mod package;
mod theme;

pub use binding::{Binding, BindingKind, BindingValue, resolve_path};
pub use capability::{Capability, CapabilityGrant, CapabilitySet};
pub use layout::{Edge, LayerPolicy, Layout, Placement, Rect, WidgetInstance};
pub use manifest::{Manifest, PlacementKind, RefreshPolicy, Size};
pub use package::{PackageError, PackageLimits, ValidatedPackage, validate_package};
pub use theme::{CosmicTheme, ThemeMode};

/// D-Bus application and service identifier.
pub const APP_ID: &str = "io.github.vynxc.CosmicWidgets";

/// Current package manifest schema.
pub const PACKAGE_SCHEMA: u32 = 1;
