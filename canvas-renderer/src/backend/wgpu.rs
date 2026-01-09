//! WebGPU/wgpu rendering backend.
//!
//! This is the primary high-performance backend using the wgpu library.
//! Supports WebGPU (native and web) with automatic fallbacks.

use std::sync::Arc;

use canvas_core::{Element, ElementKind, Scene};
use wgpu::util::DeviceExt;

use crate::{BackendType, RenderError, RenderResult};

use super::RenderBackend;

/// Vertex data for quad rendering.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position: [f32; 2],
    uv: [f32; 2],
}

impl Vertex {
    const ATTRIBS: [wgpu::VertexAttribute; 2] = wgpu::vertex_attr_array![
        0 => Float32x2,
        1 => Float32x2,
    ];

    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBS,
        }
    }
}

/// Unit quad vertices (0,0 to 1,1).
const QUAD_VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.0, 0.0],
        uv: [0.0, 0.0],
    },
    Vertex {
        position: [1.0, 0.0],
        uv: [1.0, 0.0],
    },
    Vertex {
        position: [1.0, 1.0],
        uv: [1.0, 1.0],
    },
    Vertex {
        position: [0.0, 1.0],
        uv: [0.0, 1.0],
    },
];

/// Quad indices for two triangles.
const QUAD_INDICES: &[u16] = &[0, 1, 2, 0, 2, 3];

/// Uniform data for quad shader.
#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct QuadUniforms {
    /// Transform: x, y, width, height
    transform: [f32; 4],
    /// Canvas dimensions: width, height, 0, 0
    canvas_size: [f32; 4],
    /// Element color: r, g, b, a
    color: [f32; 4],
}

/// wgpu-based GPU renderer.
pub struct WgpuBackend {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    quad_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    width: u32,
    height: u32,
    background_color: wgpu::Color,
}

impl WgpuBackend {
    /// Create a new wgpu backend without a surface (headless mode).
    ///
    /// Use `with_surface` to add a rendering surface later.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails.
    pub fn new() -> RenderResult<Self> {
        // Use pollster to block on async initialization
        pollster::block_on(Self::new_async())
    }

    /// Create a new wgpu backend asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails.
    pub async fn new_async() -> RenderResult<Self> {
        let (device, queue) = Self::init_device_and_queue().await?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let (vertex_buffer, index_buffer, uniform_buffer) = Self::create_buffers(&device);
        let uniform_bind_group_layout = Self::create_bind_group_layout(&device);
        let quad_pipeline = Self::create_quad_pipeline(&device, &uniform_bind_group_layout);

        tracing::info!("wgpu backend initialized successfully");

        Ok(Self {
            device,
            queue,
            surface: None,
            surface_config: None,
            quad_pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            uniform_bind_group_layout,
            width: 800,
            height: 600,
            background_color: wgpu::Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
        })
    }

    /// Initialize the GPU device and queue.
    async fn init_device_and_queue() -> RenderResult<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| RenderError::GpuInit("No suitable GPU adapter found".to_string()))?;

        tracing::info!("Using GPU adapter: {:?}", adapter.get_info());

        adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Saorsa Canvas Device"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| RenderError::GpuInit(e.to_string()))
    }

    /// Create vertex, index, and uniform buffers.
    fn create_buffers(device: &wgpu::Device) -> (wgpu::Buffer, wgpu::Buffer, wgpu::Buffer) {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Vertex Buffer"),
            contents: bytemuck::cast_slice(QUAD_VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Quad Index Buffer"),
            contents: bytemuck::cast_slice(QUAD_INDICES),
            usage: wgpu::BufferUsages::INDEX,
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Quad Uniform Buffer"),
            size: std::mem::size_of::<QuadUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        (vertex_buffer, index_buffer, uniform_buffer)
    }

    /// Create the uniform bind group layout.
    fn create_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Quad Uniform Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        })
    }

    /// Create the quad render pipeline.
    fn create_quad_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/quad.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Quad Pipeline Layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Quad Render Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Bgra8UnormSrgb,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        })
    }

    /// Set the background color.
    pub fn set_background_color(&mut self, r: f64, g: f64, b: f64, a: f64) {
        self.background_color = wgpu::Color { r, g, b, a };
    }

    /// Get the wgpu device.
    #[must_use]
    pub fn device(&self) -> &Arc<wgpu::Device> {
        &self.device
    }

    /// Get the wgpu queue.
    #[must_use]
    pub fn queue(&self) -> &Arc<wgpu::Queue> {
        &self.queue
    }

    /// Configure a surface for rendering.
    ///
    /// # Errors
    ///
    /// Returns an error if surface configuration fails.
    pub fn configure_surface(
        &mut self,
        surface: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> RenderResult<()> {
        let caps = surface.get_capabilities(&self.get_adapter()?);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&self.device, &config);

        self.surface = Some(surface);
        self.surface_config = Some(config);
        self.width = width;
        self.height = height;

        tracing::debug!("Surface configured: {width}x{height}, format: {format:?}");

        Ok(())
    }

    /// Get the adapter (recreate if needed).
    fn get_adapter(&self) -> RenderResult<wgpu::Adapter> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: self.surface.as_ref(),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| RenderError::GpuInit("Failed to get adapter".to_string()))
    }

    /// Render a single element as a colored quad.
    #[allow(clippy::cast_precision_loss)] // Canvas dimensions fit in f32 mantissa (max ~16M)
    fn render_element_quad(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        element: &Element,
        is_first: bool,
    ) {
        // Determine element color based on kind
        let color = Self::get_element_color(element);

        // Create uniforms
        let uniforms = QuadUniforms {
            transform: [
                element.transform.x,
                element.transform.y,
                element.transform.width,
                element.transform.height,
            ],
            canvas_size: [self.width as f32, self.height as f32, 0.0, 0.0],
            color,
        };

        // Update uniform buffer
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Create bind group
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Element Bind Group"),
            layout: &self.uniform_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: self.uniform_buffer.as_entire_binding(),
            }],
        });

        // Create render pass
        let load_op = if is_first {
            wgpu::LoadOp::Clear(self.background_color)
        } else {
            wgpu::LoadOp::Load
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Element Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: load_op,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&self.quad_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..6, 0, 0..1);
    }

    /// Get the display color for an element based on its kind.
    fn get_element_color(element: &Element) -> [f32; 4] {
        // If selected, use selection color
        if element.selected {
            return [0.2, 0.6, 1.0, 0.8]; // Blue selection highlight
        }

        match &element.kind {
            ElementKind::Chart { .. } => [0.9, 0.95, 1.0, 1.0], // Light blue for charts
            ElementKind::Image { .. } => [0.95, 0.95, 0.95, 1.0], // Light gray placeholder
            ElementKind::Model3D { .. } => [0.8, 0.9, 0.8, 1.0], // Light green for 3D
            ElementKind::Video { .. } => [0.2, 0.2, 0.2, 1.0],  // Dark for video
            ElementKind::OverlayLayer { opacity, .. } => [1.0, 1.0, 1.0, *opacity], // Transparent
            ElementKind::Text { color, .. } => {
                // Parse hex color
                Self::parse_hex_color(color).unwrap_or([0.0, 0.0, 0.0, 1.0])
            }
            ElementKind::Group { .. } => [0.95, 0.95, 0.9, 0.5], // Transparent yellow for groups
        }
    }

    /// Parse a hex color string to RGBA floats.
    fn parse_hex_color(hex: &str) -> Option<[f32; 4]> {
        let hex = hex.trim_start_matches('#');

        if hex.len() == 6 {
            let r = f32::from(u8::from_str_radix(&hex[0..2], 16).ok()?) / 255.0;
            let g = f32::from(u8::from_str_radix(&hex[2..4], 16).ok()?) / 255.0;
            let b = f32::from(u8::from_str_radix(&hex[4..6], 16).ok()?) / 255.0;
            Some([r, g, b, 1.0])
        } else if hex.len() == 8 {
            let r = f32::from(u8::from_str_radix(&hex[0..2], 16).ok()?) / 255.0;
            let g = f32::from(u8::from_str_radix(&hex[2..4], 16).ok()?) / 255.0;
            let b = f32::from(u8::from_str_radix(&hex[4..6], 16).ok()?) / 255.0;
            let a = f32::from(u8::from_str_radix(&hex[6..8], 16).ok()?) / 255.0;
            Some([r, g, b, a])
        } else {
            None
        }
    }

    /// Render to a texture (for headless/offscreen rendering).
    ///
    /// # Errors
    ///
    /// Returns an error if rendering fails.
    pub fn render_to_texture(&mut self, scene: &Scene) -> RenderResult<Vec<u8>> {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Offscreen Texture"),
            size: wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Offscreen Encoder"),
            });

        // Render all elements
        let elements: Vec<_> = scene.elements().collect();

        if elements.is_empty() {
            // Clear to background color
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.background_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        } else {
            for (i, element) in elements.iter().enumerate() {
                self.render_element_quad(&mut encoder, &view, element, i == 0);
            }
        }

        // Copy texture to buffer
        let buffer_size = u64::from(self.width) * u64::from(self.height) * 4;
        let output_buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Output Buffer"),
            size: buffer_size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let bytes_per_row = self.width * 4;
        let padded_bytes_per_row = (bytes_per_row + 255) & !255;

        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &output_buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded_bytes_per_row),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );

        self.queue.submit(std::iter::once(encoder.finish()));

        // Read back the buffer
        let buffer_slice = output_buffer.slice(..);

        let (tx, rx) = std::sync::mpsc::channel();
        buffer_slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        self.device.poll(wgpu::Maintain::Wait);

        rx.recv()
            .map_err(|e| RenderError::Frame(e.to_string()))?
            .map_err(|e| RenderError::Frame(e.to_string()))?;

        let data = buffer_slice.get_mapped_range();

        // Remove padding if present
        let mut result = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in 0..self.height {
            let start = (row * padded_bytes_per_row) as usize;
            let end = start + bytes_per_row as usize;
            result.extend_from_slice(&data[start..end]);
        }

        drop(data);
        output_buffer.unmap();

        Ok(result)
    }
}

impl RenderBackend for WgpuBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::WebGpu
    }

    fn render(&mut self, scene: &Scene) -> RenderResult<()> {
        let Some(surface) = &self.surface else {
            tracing::trace!("No surface configured, skipping render");
            return Ok(());
        };

        let output = surface
            .get_current_texture()
            .map_err(|e| RenderError::Surface(format!("Failed to get surface texture: {e}")))?;

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        // Render all elements
        let elements: Vec<_> = scene.elements().collect();

        if elements.is_empty() {
            // Clear to background color
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.background_color),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        } else {
            for (i, element) in elements.iter().enumerate() {
                self.render_element_quad(&mut encoder, &view, element, i == 0);
            }
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        tracing::trace!(
            "Rendered {} elements to {}x{} surface",
            scene.element_count(),
            self.width,
            self.height
        );

        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> RenderResult<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }

        self.width = width;
        self.height = height;

        if let (Some(surface), Some(config)) = (&self.surface, &mut self.surface_config) {
            config.width = width;
            config.height = height;
            surface.configure(&self.device, config);
            tracing::debug!("wgpu surface resized to {width}x{height}");
        }

        Ok(())
    }
}

// Re-export wgpu types for surface creation
pub use wgpu;
