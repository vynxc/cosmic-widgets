use std::ffi::c_void;
use std::ptr::NonNull;

use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use thiserror::Error;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy};

/// Direct Wayland/WGPU setup failure.
#[derive(Debug, Error)]
pub enum GpuError {
    #[error("Wayland returned a null {0} pointer")]
    NullHandle(&'static str),
    #[error("unable to create a WGPU surface: {0}")]
    CreateSurface(String),
    #[error("no compatible GPU adapter is available: {0}")]
    Adapter(String),
    #[error("unable to create the shared GPU device: {0}")]
    Device(String),
    #[error("unable to initialize the shared Vello renderer: {0}")]
    Renderer(String),
    #[error("the selected adapter cannot present to this Wayland surface")]
    UnsupportedSurface,
    #[error("surface acquisition timed out")]
    SurfaceTimeout,
    #[error("surface is currently occluded")]
    SurfaceOccluded,
    #[error("surface configuration is outdated")]
    SurfaceOutdated,
    #[error("surface was lost")]
    SurfaceLost,
    #[error("surface acquisition failed validation")]
    SurfaceValidation,
}

/// GPU resources shared by every desktop and edge surface.
pub struct GpuHub {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: vello::Renderer,
}

impl GpuHub {
    /// Creates the shared GPU state using the first layer surface for adapter selection.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError`] when Wayland handles are null or WGPU cannot create a
    /// compatible adapter, device, or surface.
    pub fn new(
        connection: &Connection,
        wl_surface: &WlSurface,
    ) -> Result<(Self, GpuSurface), GpuError> {
        let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
        descriptor.backends = wgpu::Backends::VULKAN | wgpu::Backends::GL;
        let instance = wgpu::Instance::new(descriptor);
        let surface = create_surface(&instance, connection, wl_surface)?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .map_err(|error| GpuError::Adapter(error.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("cosmic-widgets shared device"),
            required_features: wgpu::Features::CLEAR_TEXTURE | wgpu::Features::PIPELINE_CACHE,
            required_limits: adapter.limits(),
            memory_hints: wgpu::MemoryHints::MemoryUsage,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::disabled(),
        }))
        .map_err(|error| GpuError::Device(error.to_string()))?;
        let renderer = vello::Renderer::new(
            &device,
            vello::RendererOptions {
                use_cpu: false,
                antialiasing_support: vello::AaSupport::area_only(),
                num_init_threads: None,
                pipeline_cache: None,
            },
        )
        .map_err(|error| GpuError::Renderer(error.to_string()))?;
        let hub = Self {
            instance,
            adapter,
            device,
            queue,
            renderer,
        };
        Ok((
            hub,
            GpuSurface {
                surface,
                configuration: None,
                intermediate: None,
            },
        ))
    }

    /// Creates another swapchain while retaining the shared device and caches.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError`] for invalid Wayland handles or an incompatible surface.
    pub fn create_surface(
        &self,
        connection: &Connection,
        wl_surface: &WlSurface,
    ) -> Result<GpuSurface, GpuError> {
        let surface = create_surface(&self.instance, connection, wl_surface)?;
        if surface.get_capabilities(&self.adapter).formats.is_empty() {
            return Err(GpuError::UnsupportedSurface);
        }
        Ok(GpuSurface {
            surface,
            configuration: None,
            intermediate: None,
        })
    }
}

/// Swapchain state owned by one composite layer-shell surface.
pub struct GpuSurface {
    surface: wgpu::Surface<'static>,
    configuration: Option<wgpu::SurfaceConfiguration>,
    intermediate: Option<IntermediateTexture>,
}

struct IntermediateTexture {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    blitter: wgpu::util::TextureBlitter,
}

impl GpuSurface {
    /// Configures transparent composition at the supplied physical size.
    ///
    /// # Errors
    ///
    /// Returns [`GpuError::UnsupportedSurface`] if the adapter exposes no usable format.
    pub fn configure(&mut self, hub: &GpuHub, width: u32, height: u32) -> Result<(), GpuError> {
        let capabilities = self.surface.get_capabilities(&hub.adapter);
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| {
                matches!(
                    format,
                    wgpu::TextureFormat::Rgba8Unorm | wgpu::TextureFormat::Bgra8Unorm
                )
            })
            .or_else(|| {
                capabilities
                    .formats
                    .iter()
                    .copied()
                    .find(wgpu::TextureFormat::is_srgb)
            })
            .or_else(|| capabilities.formats.first().copied())
            .ok_or(GpuError::UnsupportedSurface)?;
        let alpha_mode = capabilities
            .alpha_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::CompositeAlphaMode::PreMultiplied)
            .unwrap_or(wgpu::CompositeAlphaMode::Auto);
        let present_mode = capabilities
            .present_modes
            .iter()
            .copied()
            .find(|mode| *mode == wgpu::PresentMode::Mailbox)
            .unwrap_or(wgpu::PresentMode::Fifo);
        let configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: vec![format],
        };
        self.surface.configure(&hub.device, &configuration);
        let texture = hub.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cosmic-widgets Vello intermediate"),
            size: wgpu::Extent3d {
                width: configuration.width,
                height: configuration.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.intermediate = Some(IntermediateTexture {
            _texture: texture,
            view,
            blitter: wgpu::util::TextureBlitter::new(&hub.device, format),
        });
        self.configuration = Some(configuration);
        Ok(())
    }

    /// Presents a transparent frame, used to validate composition before Vello paints the scene.
    ///
    /// # Errors
    ///
    /// Returns a surface status error when the swapchain cannot supply a frame.
    pub fn clear_transparent(&self, hub: &GpuHub) -> Result<(), GpuError> {
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(GpuError::SurfaceTimeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(GpuError::SurfaceOccluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(GpuError::SurfaceOutdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(GpuError::SurfaceLost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(GpuError::SurfaceValidation),
        };
        let view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = hub
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cosmic-widgets transparent frame"),
            });
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("cosmic-widgets clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }
        hub.queue.submit([encoder.finish()]);
        texture.present();
        Ok(())
    }

    /// Renders a Vello scene directly into the current Wayland swapchain image.
    pub(crate) fn render_scene(
        &self,
        hub: &mut GpuHub,
        scene: &vello::Scene,
    ) -> Result<(), GpuError> {
        let texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            wgpu::CurrentSurfaceTexture::Timeout => return Err(GpuError::SurfaceTimeout),
            wgpu::CurrentSurfaceTexture::Occluded => return Err(GpuError::SurfaceOccluded),
            wgpu::CurrentSurfaceTexture::Outdated => return Err(GpuError::SurfaceOutdated),
            wgpu::CurrentSurfaceTexture::Lost => return Err(GpuError::SurfaceLost),
            wgpu::CurrentSurfaceTexture::Validation => return Err(GpuError::SurfaceValidation),
        };
        let target_view = texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (width, height) = self.size().ok_or(GpuError::SurfaceOutdated)?;
        let intermediate = self
            .intermediate
            .as_ref()
            .ok_or(GpuError::SurfaceOutdated)?;
        hub.renderer
            .render_to_texture(
                &hub.device,
                &hub.queue,
                scene,
                &intermediate.view,
                &vello::RenderParams {
                    base_color: vello::peniko::Color::TRANSPARENT,
                    width,
                    height,
                    antialiasing_method: vello::AaConfig::Area,
                },
            )
            .map_err(|error| GpuError::Renderer(error.to_string()))?;
        let mut encoder = hub
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("cosmic-widgets surface blit"),
            });
        intermediate
            .blitter
            .copy(&hub.device, &mut encoder, &intermediate.view, &target_view);
        hub.queue.submit([encoder.finish()]);
        texture.present();
        Ok(())
    }

    /// Returns the currently configured physical size.
    pub fn size(&self) -> Option<(u32, u32)> {
        self.configuration
            .as_ref()
            .map(|config| (config.width, config.height))
    }
}

#[expect(
    unsafe_code,
    reason = "WGPU requires raw Wayland handles; their lifetime is owned by the caller and the GPU surface is dropped before the wl_surface"
)]
fn create_surface(
    instance: &wgpu::Instance,
    connection: &Connection,
    surface: &WlSurface,
) -> Result<wgpu::Surface<'static>, GpuError> {
    let display_pointer = NonNull::new(connection.backend().display_ptr().cast::<c_void>())
        .ok_or(GpuError::NullHandle("display"))?;
    let surface_pointer = NonNull::new(surface.id().as_ptr().cast::<c_void>())
        .ok_or(GpuError::NullHandle("surface"))?;
    let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(display_pointer));
    let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(surface_pointer));

    // SAFETY: The caller owns the connection and wl_surface for the returned WGPU
    // surface's complete lifetime and must drop GpuSurface before either Wayland object.
    unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle: Some(raw_display_handle),
            raw_window_handle,
        })
    }
    .map_err(|error| GpuError::CreateSurface(error.to_string()))
}
