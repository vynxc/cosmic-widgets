use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cw_renderer::BlitzVelloDocument;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, FrameCallbackData};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
    LayerSurfaceConfigure,
};
use smithay_client_toolkit::{delegate_registry, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_region, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};

use crate::{GpuHub, GpuSurface, ShellError};

/// Thread-safe source used to regenerate declarative HTML after provider changes.
pub type HtmlProvider = Arc<dyn Fn() -> String + Send + Sync>;

/// Placement and rendering policy for one desktop widget surface.
#[derive(Debug, Clone, Copy)]
pub struct DesktopWidgetConfig {
    pub width: u32,
    pub height: u32,
    pub top: i32,
    pub right: i32,
    pub dark: bool,
}

impl Default for DesktopWidgetConfig {
    fn default() -> Self {
        Self {
            width: 320,
            height: 184,
            top: 72,
            right: 32,
            dark: true,
        }
    }
}

/// Runs a click-through bottom-layer widget until the compositor closes it.
///
/// The provider is checked once per second, but Blitz reparses the document only
/// when its HTML changes. GPU rendering and presentation remain direct.
///
/// # Errors
///
/// Returns [`ShellError`] when required Wayland globals are unavailable, event
/// dispatch fails, or the Blitz/Vello surface cannot be initialized.
pub fn run_desktop_widget(
    config: DesktopWidgetConfig,
    html_provider: HtmlProvider,
) -> Result<(), ShellError> {
    let connection =
        Connection::connect_to_env().map_err(|error| ShellError::Connection(error.to_string()))?;
    let (globals, mut event_queue) = registry_queue_init(&connection)
        .map_err(|error| ShellError::Connection(error.to_string()))?;
    let queue_handle = event_queue.handle();
    let compositor = CompositorState::bind(&globals, &queue_handle)
        .map_err(|error| ShellError::Connection(error.to_string()))?;
    let layer_shell = LayerShell::bind(&globals, &queue_handle)
        .map_err(|error| ShellError::Connection(error.to_string()))?;

    let surface = compositor.create_surface(&queue_handle);
    let empty_region = compositor.wl_compositor().create_region(&queue_handle, ());
    surface.set_input_region(Some(&empty_region));
    empty_region.destroy();

    let layer = layer_shell.create_layer_surface(
        &queue_handle,
        surface,
        Layer::Bottom,
        Some("cosmic-widgets"),
        None,
    );
    layer.set_anchor(Anchor::TOP | Anchor::RIGHT);
    layer.set_margin(config.top, config.right, 0, 0);
    layer.set_size(config.width, config.height);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.commit();

    let mut host = DesktopWidgetHost {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &queue_handle),
        gpu: None,
        document: None,
        layer,
        config,
        html_provider,
        rendered_html: String::new(),
        width: config.width,
        height: config.height,
        scale: 1.0,
        started: Instant::now(),
        last_frame: Instant::now(),
        exit: false,
        error: None,
    };

    while !host.exit {
        event_queue
            .blocking_dispatch(&mut host)
            .map_err(|error| ShellError::Connection(error.to_string()))?;
    }
    host.error.map_or(Ok(()), Err)
}

struct DesktopWidgetHost {
    registry_state: RegistryState,
    output_state: OutputState,
    // The GPU surface must be dropped before its Wayland layer.
    gpu: Option<(GpuHub, GpuSurface)>,
    document: Option<BlitzVelloDocument>,
    layer: LayerSurface,
    config: DesktopWidgetConfig,
    html_provider: HtmlProvider,
    rendered_html: String,
    width: u32,
    height: u32,
    scale: f64,
    started: Instant,
    last_frame: Instant,
    exit: bool,
    error: Option<ShellError>,
}

impl DesktopWidgetHost {
    fn draw(&mut self, connection: &Connection, queue_handle: &QueueHandle<Self>) {
        let html = (self.html_provider)();
        let logical_size = (self.width, self.height);
        if let Some(document) = &mut self.document {
            if html != self.rendered_html {
                document.set_html(&html, logical_size, self.scale, self.config.dark);
                self.rendered_html.clone_from(&html);
            }
        } else {
            self.document = Some(BlitzVelloDocument::new(
                &html,
                logical_size,
                self.scale,
                self.config.dark,
            ));
            self.rendered_html = html;
        }

        let physical_size = (
            physical_dimension(self.width, self.scale),
            physical_dimension(self.height, self.scale),
        );
        let result = (|| {
            if self.gpu.is_none() {
                let (hub, mut surface) = GpuHub::new(connection, self.layer.wl_surface())
                    .map_err(|error| ShellError::Renderer(error.to_string()))?;
                surface
                    .configure(&hub, physical_size.0, physical_size.1)
                    .map_err(|error| ShellError::Renderer(error.to_string()))?;
                self.gpu = Some((hub, surface));
            }
            let Some((hub, surface)) = &mut self.gpu else {
                return Err(ShellError::Renderer("GPU state is unavailable".into()));
            };
            if surface.size() != Some(physical_size) {
                surface
                    .configure(hub, physical_size.0, physical_size.1)
                    .map_err(|error| ShellError::Renderer(error.to_string()))?;
            }
            let Some(document) = &mut self.document else {
                return Err(ShellError::Renderer("Blitz document is unavailable".into()));
            };
            let scene = document.paint(self.started.elapsed().as_secs_f64());
            surface
                .render_scene(hub, scene)
                .map_err(|error| ShellError::Renderer(error.to_string()))
        })();
        if let Err(error) = result {
            self.error = Some(error);
            self.exit = true;
            return;
        }

        self.layer.wl_surface().frame(
            queue_handle,
            FrameCallbackData(self.layer.wl_surface().clone()),
        );
        self.layer.commit();
        self.last_frame = Instant::now();
    }
}

fn physical_dimension(logical: u32, scale: f64) -> u32 {
    let value = (f64::from(logical) * scale).ceil();
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the finite positive dimension is clamped to the u32 surface range"
    )]
    let dimension = value.min(f64::from(u32::MAX)) as u32;
    dimension.max(1)
}

impl CompositorHandler for DesktopWidgetHost {
    fn scale_factor_changed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        factor: i32,
    ) {
        if surface == self.layer.wl_surface() {
            let factor = factor.max(1);
            surface.set_buffer_scale(factor);
            self.scale = f64::from(factor);
            self.rendered_html.clear();
        }
    }

    fn transform_changed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _transform: wl_output::Transform,
    ) {
    }

    fn frame(
        &mut self,
        connection: &Connection,
        queue_handle: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
        if surface != self.layer.wl_surface() {
            return;
        }
        let target_interval = Duration::from_secs(1);
        if let Some(remaining) = target_interval.checked_sub(self.last_frame.elapsed()) {
            std::thread::sleep(remaining);
        }
        self.draw(connection, queue_handle);
    }

    fn surface_enter(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for DesktopWidgetHost {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }

    fn new_output(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn update_output(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }

    fn output_destroyed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _output: wl_output::WlOutput,
    ) {
    }
}

impl LayerShellHandler for DesktopWidgetHost {
    fn closed(
        &mut self,
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
        _layer: &LayerSurface,
    ) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        connection: &Connection,
        queue_handle: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        self.width =
            NonZeroU32::new(configure.new_size.0).map_or(self.config.width, NonZeroU32::get);
        self.height =
            NonZeroU32::new(configure.new_size.1).map_or(self.config.height, NonZeroU32::get);
        self.draw(connection, queue_handle);
    }
}

delegate_registry!(DesktopWidgetHost);

impl ProvidesRegistryState for DesktopWidgetHost {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }

    registry_handlers![OutputState];
}

impl Dispatch<wl_region::WlRegion, ()> for DesktopWidgetHost {
    fn event(
        _state: &mut Self,
        _region: &wl_region::WlRegion,
        _event: wl_region::Event,
        _data: &(),
        _connection: &Connection,
        _queue_handle: &QueueHandle<Self>,
    ) {
    }
}

smithay_client_toolkit::delegate_dispatch2!(DesktopWidgetHost);
