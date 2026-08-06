//! Renderer boundary isolating the pre-alpha Blitz dependency.

use std::collections::BTreeMap;

use cw_core::{BindingValue, CosmicTheme, Rect};
use thiserror::Error;
use uuid::Uuid;

/// Identifier for one compositor surface and composite document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SurfaceId(pub Uuid);

/// Package content inserted into a scoped composite root.
#[derive(Debug, Clone)]
pub struct WidgetDocument {
    pub instance_id: Uuid,
    pub package_id: String,
    pub html: String,
    pub css: String,
    pub bounds: Rect,
}

/// Minimal DOM change produced by providers or Extism.
#[derive(Debug, Clone, PartialEq)]
pub struct DomPatch {
    pub node: u64,
    pub property: String,
    pub value: BindingValue,
}

/// Renderer integration failure.
#[derive(Debug, Error)]
pub enum RendererError {
    #[error("unknown surface {0:?}")]
    UnknownSurface(SurfaceId),
    #[error("widget document is invalid: {0}")]
    InvalidDocument(String),
    #[error("renderer backend failed: {0}")]
    Backend(String),
}

/// Interface implemented by Blitz/Vello without leaking its types into the host.
pub trait DocumentRenderer {
    /// Allocates renderer state for a compositor surface.
    fn create_surface(&mut self, logical_size: (u32, u32), scale: f64) -> SurfaceId;
    /// Removes renderer state.
    ///
    /// # Errors
    ///
    /// Returns [`RendererError::UnknownSurface`] when the identifier is stale.
    fn destroy_surface(&mut self, surface: SurfaceId) -> Result<(), RendererError>;
    /// Replaces inherited COSMIC tokens for every surface.
    ///
    /// # Errors
    ///
    /// Returns [`RendererError::Backend`] when the backend cannot update styles.
    fn set_theme(&mut self, theme: &CosmicTheme) -> Result<(), RendererError>;
    /// Mounts one scoped widget root into a composite surface.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown surface or invalid document.
    fn mount(&mut self, surface: SurfaceId, document: WidgetDocument) -> Result<(), RendererError>;
    /// Applies minimal declarative changes to a composite document.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown surface or rejected patch.
    fn patch(&mut self, surface: SurfaceId, patches: &[DomPatch]) -> Result<(), RendererError>;
    /// Resolves layout and presents the latest scene.
    ///
    /// # Errors
    ///
    /// Returns an error for an unknown surface or graphics backend failure.
    fn render(&mut self, surface: SurfaceId, monotonic_seconds: f64) -> Result<(), RendererError>;
}

/// Deterministic non-GPU backend used by runtime tests and headless tooling.
#[derive(Debug, Default)]
pub struct RecordingRenderer {
    surfaces: BTreeMap<SurfaceId, Vec<WidgetDocument>>,
    pub patches: Vec<DomPatch>,
    pub theme_css: String,
}

impl DocumentRenderer for RecordingRenderer {
    fn create_surface(&mut self, _logical_size: (u32, u32), _scale: f64) -> SurfaceId {
        let id = SurfaceId(Uuid::new_v4());
        self.surfaces.insert(id, Vec::new());
        id
    }

    fn destroy_surface(&mut self, surface: SurfaceId) -> Result<(), RendererError> {
        self.surfaces
            .remove(&surface)
            .map(|_| ())
            .ok_or(RendererError::UnknownSurface(surface))
    }

    fn set_theme(&mut self, theme: &CosmicTheme) -> Result<(), RendererError> {
        self.theme_css = theme.to_css();
        Ok(())
    }

    fn mount(&mut self, surface: SurfaceId, document: WidgetDocument) -> Result<(), RendererError> {
        let documents = self
            .surfaces
            .get_mut(&surface)
            .ok_or(RendererError::UnknownSurface(surface))?;
        documents.push(document);
        Ok(())
    }

    fn patch(&mut self, surface: SurfaceId, patches: &[DomPatch]) -> Result<(), RendererError> {
        if !self.surfaces.contains_key(&surface) {
            return Err(RendererError::UnknownSurface(surface));
        }
        self.patches.extend_from_slice(patches);
        Ok(())
    }

    fn render(&mut self, surface: SurfaceId, _monotonic_seconds: f64) -> Result<(), RendererError> {
        self.surfaces
            .contains_key(&surface)
            .then_some(())
            .ok_or(RendererError::UnknownSurface(surface))
    }
}

/// Prefixes package content with an instance root understood by the Blitz adapter.
pub fn composite_fragment(document: &WidgetDocument) -> String {
    format!(
        "<section class=\"cw-instance\" data-cw-instance=\"{}\" data-cw-package=\"{}\" style=\"left:{}px;top:{}px;width:{}px;height:{}px\"><style>{}</style>{}</section>",
        document.instance_id,
        escape_attribute(&document.package_id),
        document.bounds.x,
        document.bounds.y,
        document.bounds.width,
        document.bounds.height,
        document.css,
        document.html,
    )
}

fn escape_attribute(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

#[cfg(feature = "blitz")]
/// Parses a composite HTML document using Blitz without creating a Winit window.
pub fn parse_blitz_document(html: &str) -> blitz_html::HtmlDocument {
    blitz_html::HtmlDocument::from_html(html, blitz_dom::DocumentConfig::default())
}

#[cfg(feature = "blitz")]
/// Resolved Blitz document capable of producing backend-independent `AnyRender` scenes.
pub struct BlitzSceneDocument {
    document: blitz_html::HtmlDocument,
    physical_size: (u32, u32),
    scale: f64,
}

#[cfg(feature = "blitz")]
impl BlitzSceneDocument {
    /// Parses HTML with a fixed local viewport and no network provider.
    pub fn new(html: &str, logical_size: (u32, u32), scale: f64, dark: bool) -> Self {
        let physical_size = (
            physical_dimension(logical_size.0, scale),
            physical_dimension(logical_size.1, scale),
        );
        let color_scheme = if dark {
            blitz_traits::shell::ColorScheme::Dark
        } else {
            blitz_traits::shell::ColorScheme::Light
        };
        let viewport = blitz_traits::shell::Viewport::new(
            physical_size.0,
            physical_size.1,
            blitz_scale(scale),
            color_scheme,
        );
        let document = blitz_html::HtmlDocument::from_html(
            html,
            blitz_dom::DocumentConfig {
                viewport: Some(viewport),
                ..Default::default()
            },
        );
        Self {
            document,
            physical_size,
            scale,
        }
    }

    /// Resolves CSS, text, and layout, then records paint commands into an `AnyRender` scene.
    pub fn paint(&mut self, animation_seconds: f64) -> anyrender::Scene {
        self.document.resolve(animation_seconds);
        let mut scene = anyrender::Scene::new();
        blitz_paint::paint_scene(
            &mut scene,
            &mut self.document,
            self.scale,
            self.physical_size.0,
            self.physical_size.1,
            0,
            0,
        );
        scene
    }

    /// Replaces the viewport after an output scale or surface-size change.
    pub fn resize(&mut self, logical_size: (u32, u32), scale: f64, dark: bool) {
        self.scale = scale;
        self.physical_size = (
            physical_dimension(logical_size.0, scale),
            physical_dimension(logical_size.1, scale),
        );
        let color_scheme = if dark {
            blitz_traits::shell::ColorScheme::Dark
        } else {
            blitz_traits::shell::ColorScheme::Light
        };
        self.document
            .set_viewport(blitz_traits::shell::Viewport::new(
                self.physical_size.0,
                self.physical_size.1,
                blitz_scale(scale),
                color_scheme,
            ));
    }
}

#[cfg(feature = "vello")]
/// A Blitz document that paints directly into a reusable Vello command scene.
pub struct BlitzVelloDocument {
    document: BlitzSceneDocument,
    scene: vello::Scene,
}

#[cfg(feature = "vello")]
impl BlitzVelloDocument {
    /// Creates a local HTML document and an initially empty Vello scene.
    pub fn new(html: &str, logical_size: (u32, u32), scale: f64, dark: bool) -> Self {
        Self {
            document: BlitzSceneDocument::new(html, logical_size, scale, dark),
            scene: vello::Scene::new(),
        }
    }

    /// Replaces the local document after provider data changes.
    pub fn set_html(&mut self, html: &str, logical_size: (u32, u32), scale: f64, dark: bool) {
        self.document = BlitzSceneDocument::new(html, logical_size, scale, dark);
    }

    /// Resolves the document and returns Vello commands ready for GPU submission.
    pub fn paint(&mut self, animation_seconds: f64) -> &vello::Scene {
        let document = &mut self.document.document;
        let scale = self.document.scale;
        let physical_size = self.document.physical_size;
        document.resolve(animation_seconds);
        self.scene.reset();
        let mut painter = anyrender_vello::VelloScenePainter::new(&mut self.scene);
        blitz_paint::paint_scene(
            &mut painter,
            document,
            scale,
            physical_size.0,
            physical_size.1,
            0,
            0,
        );
        &self.scene
    }
}

#[cfg(feature = "blitz")]
fn physical_dimension(logical: u32, scale: f64) -> u32 {
    let scaled = (f64::from(logical) * scale).ceil();
    if !scaled.is_finite() || scaled <= 0.0 {
        return 1;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the finite positive dimension is clamped to the u32 surface range"
    )]
    let result = scaled.min(f64::from(u32::MAX)) as u32;
    result.max(1)
}

#[cfg(feature = "blitz")]
#[expect(
    clippy::cast_possible_truncation,
    reason = "the finite scale is clamped to the f32 viewport range first"
)]
fn blitz_scale(scale: f64) -> f32 {
    if !scale.is_finite() || scale <= 0.0 {
        return 1.0;
    }
    scale.min(f64::from(f32::MAX)) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recording_renderer_should_reject_unknown_surface() {
        let mut renderer = RecordingRenderer::default();
        let result = renderer.render(SurfaceId(Uuid::nil()), 0.0);
        assert!(matches!(result, Err(RendererError::UnknownSurface(_))));
    }

    #[test]
    fn composite_fragment_should_escape_package_attribute() {
        let document = WidgetDocument {
            instance_id: Uuid::nil(),
            package_id: "example\" bad".into(),
            html: "<p>Safe</p>".into(),
            css: String::new(),
            bounds: Rect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 50.0,
            },
        };
        assert!(composite_fragment(&document).contains("example&quot; bad"));
    }

    #[cfg(feature = "blitz")]
    #[test]
    fn blitz_scene_should_resolve_and_paint_local_html() {
        let mut document = BlitzSceneDocument::new(
            "<style>body{background:transparent;color:white}</style><p>Hello COSMIC</p>",
            (320, 180),
            1.25,
            true,
        );
        let _scene = document.paint(0.0);
    }
}
