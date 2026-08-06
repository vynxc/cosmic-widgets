use serde::{Deserialize, Serialize};

/// COSMIC appearance mode exposed to widget CSS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    Light,
    Dark,
}

/// Renderer-independent COSMIC design tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CosmicTheme {
    pub mode: ThemeMode,
    pub high_contrast: bool,
    pub frosted: bool,
    pub background: String,
    pub surface: String,
    pub surface_alt: String,
    pub on_background: String,
    pub on_surface: String,
    pub accent: String,
    pub success: String,
    pub warning: String,
    pub destructive: String,
    pub divider: String,
    pub radius_small: u16,
    pub radius_medium: u16,
    pub radius_large: u16,
}

impl CosmicTheme {
    /// Produces the small inherited stylesheet inserted before widget styles.
    pub fn to_css(&self) -> String {
        let mode = match self.mode {
            ThemeMode::Light => "light",
            ThemeMode::Dark => "dark",
        };
        format!(
            ":root{{--cw-mode:{mode};--cw-bg:{};--cw-surface:{};--cw-surface-alt:{};--cw-on-bg:{};--cw-on-surface:{};--cw-accent:{};--cw-success:{};--cw-warning:{};--cw-destructive:{};--cw-divider:{};--cw-radius-sm:{}px;--cw-radius-md:{}px;--cw-radius-lg:{}px;--cw-space-1:4px;--cw-space-2:8px;--cw-space-3:12px;--cw-space-4:16px;--cw-space-6:24px;color-scheme:{mode};}}",
            self.background,
            self.surface,
            self.surface_alt,
            self.on_background,
            self.on_surface,
            self.accent,
            self.success,
            self.warning,
            self.destructive,
            self.divider,
            self.radius_small,
            self.radius_medium,
            self.radius_large,
        )
    }
}

impl Default for CosmicTheme {
    fn default() -> Self {
        Self {
            mode: ThemeMode::Dark,
            high_contrast: false,
            frosted: true,
            background: "#18181b".into(),
            surface: "rgba(43, 43, 49, 0.92)".into(),
            surface_alt: "rgba(58, 58, 66, 0.9)".into(),
            on_background: "#f3f3f5".into(),
            on_surface: "#f3f3f5".into(),
            accent: "#74b9ff".into(),
            success: "#57d39b".into(),
            warning: "#f6c177".into(),
            destructive: "#ff7b86".into(),
            divider: "rgba(255, 255, 255, 0.12)".into(),
            radius_small: 8,
            radius_medium: 12,
            radius_large: 18,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_css_should_include_accent_token() {
        let css = CosmicTheme::default().to_css();
        assert!(css.contains("--cw-accent:#74b9ff"));
    }
}
