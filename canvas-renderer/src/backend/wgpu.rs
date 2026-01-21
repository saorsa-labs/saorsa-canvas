//! WebGPU/wgpu rendering backend.
//!
//! This is the primary high-performance backend using the wgpu library.
//! Supports WebGPU (native and web) with automatic fallbacks.

use std::collections::HashMap;
use std::sync::Arc;

use canvas_core::{Element, ElementKind, Scene};
use wgpu::util::DeviceExt;

use crate::chart::{parse_chart_config, render_chart_to_buffer};
use crate::image::{create_placeholder, load_image_from_data_uri};
use crate::{BackendType, RenderError, RenderResult};

#[cfg(target_arch = "wasm32")]
use web_sys::HtmlCanvasElement;

use super::RenderBackend;

#[cfg(not(target_arch = "wasm32"))]
use winit::window::Window;

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

/// Cached texture with GPU resources.
struct CachedTexture {
    /// The underlying GPU texture (kept alive to prevent deallocation).
    #[allow(dead_code)]
    texture: wgpu::Texture,
    /// Texture view used for binding to shaders.
    view: wgpu::TextureView,
}

/// wgpu-based GPU renderer.
pub struct WgpuBackend {
    device: Arc<wgpu::Device>,
    queue: Arc<wgpu::Queue>,
    surface: Option<wgpu::Surface<'static>>,
    surface_config: Option<wgpu::SurfaceConfiguration>,
    /// Pipeline for solid color quads.
    quad_pipeline: wgpu::RenderPipeline,
    /// Pipeline for textured quads.
    textured_pipeline: wgpu::RenderPipeline,
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    uniform_buffer: wgpu::Buffer,
    /// Bind group layout for solid color quads.
    uniform_bind_group_layout: wgpu::BindGroupLayout,
    /// Bind group layout for textured quads.
    textured_bind_group_layout: wgpu::BindGroupLayout,
    /// Sampler for texture filtering.
    sampler: wgpu::Sampler,
    /// Cached textures by element ID.
    texture_cache: HashMap<String, CachedTexture>,
    /// Cached video textures by stream ID.
    video_textures: HashMap<String, CachedTexture>,
    width: u32,
    height: u32,
    background_color: wgpu::Color,
    /// Display scale factor (1.0 = standard, 2.0 = Retina)
    scale_factor: f64,
}

impl WgpuBackend {
    /// Create a new WebGPU backend from a browser canvas element.
    ///
    /// This method properly initializes wgpu surface, adapter, and device
    /// together to ensure compatibility (mirroring the native `from_window` flow).
    /// This fixes SurfaceError::IncompatibleDevice issues in WASM.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails.
    #[cfg(target_arch = "wasm32")]
    pub fn from_canvas(canvas: HtmlCanvasElement) -> RenderResult<Self> {
        pollster::block_on(Self::from_canvas_async(canvas))
    }

    /// Create a wgpu backend from a canvas element asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails.
    #[cfg(target_arch = "wasm32")]
    pub async fn from_canvas_async(canvas: HtmlCanvasElement) -> RenderResult<Self> {
        let width = canvas.width();
        let height = canvas.height();

        // Create instance first
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL,
            ..Default::default()
        });

        // Create surface from the canvas (must use same instance for adapter)
        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| RenderError::Surface(e.to_string()))?;

        // Get adapter compatible with this surface - critical for avoiding IncompatibleDevice
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| RenderError::GpuInit("No suitable GPU adapter found".to_string()))?;

        // Create device from this specific adapter
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("Saorsa Canvas Device (WASM)"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::downlevel_webgl2_defaults(),
                    memory_hints: wgpu::MemoryHints::default(),
                },
                None,
            )
            .await
            .map_err(|e| RenderError::GpuInit(e.to_string()))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Configure surface with adapter capabilities
        let caps = surface.get_capabilities(&adapter);
        let format =
            caps.formats
                .iter()
                .find(|f| f.is_srgb())
                .copied()
                .unwrap_or(caps.formats.first().copied().ok_or_else(|| {
                    RenderError::GpuInit("No surface formats available".to_string())
                })?);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .ok_or_else(|| RenderError::GpuInit("No alpha modes available".to_string()))?,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // Create buffers and pipeline
        let (vertex_buffer, index_buffer, uniform_buffer) = Self::create_buffers(&device);
        let uniform_bind_group_layout = Self::create_bind_group_layout(&device);
        let quad_pipeline =
            Self::create_quad_pipeline_with_format(&device, &uniform_bind_group_layout, format);

        // Textured pipeline setup
        let textured_bind_group_layout = Self::create_textured_bind_group_layout(&device);
        let textured_pipeline = Self::create_textured_pipeline_with_format(
            &device,
            &textured_bind_group_layout,
            format,
        );
        let sampler = Self::create_sampler(&device);

        tracing::info!("wgpu backend initialized with canvas: {}x{}", width, height);

        Ok(Self {
            device,
            queue,
            surface: Some(surface),
            surface_config: Some(config),
            quad_pipeline,
            textured_pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            uniform_bind_group_layout,
            textured_bind_group_layout,
            sampler,
            texture_cache: HashMap::new(),
            video_textures: HashMap::new(),
            width,
            height,
            background_color: wgpu::Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            scale_factor: 1.0,
        })
    }

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
        let (device, queue) = Self::init_device_and_queue(None).await?;
        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let (vertex_buffer, index_buffer, uniform_buffer) = Self::create_buffers(&device);
        let uniform_bind_group_layout = Self::create_bind_group_layout(&device);
        let quad_pipeline = Self::create_quad_pipeline(&device, &uniform_bind_group_layout);

        // Textured pipeline setup
        let textured_bind_group_layout = Self::create_textured_bind_group_layout(&device);
        let textured_pipeline = Self::create_textured_pipeline_with_format(
            &device,
            &textured_bind_group_layout,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        );
        let sampler = Self::create_sampler(&device);

        tracing::info!("wgpu backend initialized successfully");

        Ok(Self {
            device,
            queue,
            surface: None,
            surface_config: None,
            quad_pipeline,
            textured_pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            uniform_bind_group_layout,
            textured_bind_group_layout,
            sampler,
            texture_cache: HashMap::new(),
            video_textures: HashMap::new(),
            width: 800,
            height: 600,
            background_color: wgpu::Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            scale_factor: 1.0,
        })
    }

    /// Create a wgpu backend from a winit window.
    ///
    /// This method properly initializes the wgpu surface, adapter, and device
    /// together to ensure compatibility.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_window(window: Arc<Window>) -> RenderResult<Self> {
        pollster::block_on(Self::from_window_async(window))
    }

    /// Create a wgpu backend from a winit window asynchronously.
    ///
    /// # Errors
    ///
    /// Returns an error if GPU initialization fails.
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::too_many_lines)]
    pub async fn from_window_async(window: Arc<Window>) -> RenderResult<Self> {
        let size = window.inner_size();
        let width = size.width;
        let height = size.height;
        let scale_factor = window.scale_factor();

        // Create instance
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        // Create surface from window (must use same instance for adapter)
        let surface = instance
            .create_surface(window)
            .map_err(|e| RenderError::Surface(e.to_string()))?;

        // Get adapter compatible with this surface
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .ok_or_else(|| RenderError::GpuInit("No suitable GPU adapter found".to_string()))?;

        tracing::info!("Using GPU adapter: {:?}", adapter.get_info());

        // Create device from this adapter
        let (device, queue) = adapter
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
            .map_err(|e| RenderError::GpuInit(e.to_string()))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        // Configure surface
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| RenderError::GpuInit("No surface formats available".to_string()))?;

        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .ok_or_else(|| RenderError::GpuInit("No alpha modes available".to_string()))?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &config);

        // Create buffers and pipeline
        let (vertex_buffer, index_buffer, uniform_buffer) = Self::create_buffers(&device);
        let uniform_bind_group_layout = Self::create_bind_group_layout(&device);
        let quad_pipeline =
            Self::create_quad_pipeline_with_format(&device, &uniform_bind_group_layout, format);

        // Textured pipeline setup
        let textured_bind_group_layout = Self::create_textured_bind_group_layout(&device);
        let textured_pipeline = Self::create_textured_pipeline_with_format(
            &device,
            &textured_bind_group_layout,
            format,
        );
        let sampler = Self::create_sampler(&device);

        tracing::info!(
            "wgpu backend initialized with window: {}x{} (scale: {})",
            width,
            height,
            scale_factor
        );

        Ok(Self {
            device,
            queue,
            surface: Some(surface),
            surface_config: Some(config),
            quad_pipeline,
            textured_pipeline,
            vertex_buffer,
            index_buffer,
            uniform_buffer,
            uniform_bind_group_layout,
            textured_bind_group_layout,
            sampler,
            texture_cache: HashMap::new(),
            video_textures: HashMap::new(),
            width,
            height,
            background_color: wgpu::Color {
                r: 1.0,
                g: 1.0,
                b: 1.0,
                a: 1.0,
            },
            scale_factor,
        })
    }

    /// Initialize the GPU device and queue.
    async fn init_device_and_queue(
        _surface: Option<&wgpu::Surface<'_>>,
    ) -> RenderResult<(wgpu::Device, wgpu::Queue)> {
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

    /// Create the quad render pipeline with default format.
    fn create_quad_pipeline(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
    ) -> wgpu::RenderPipeline {
        Self::create_quad_pipeline_with_format(
            device,
            bind_group_layout,
            wgpu::TextureFormat::Bgra8UnormSrgb,
        )
    }

    /// Create the quad render pipeline with a specific texture format.
    fn create_quad_pipeline_with_format(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
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
                    format,
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

    /// Create the textured bind group layout.
    fn create_textured_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Textured Bind Group Layout"),
            entries: &[
                // Uniforms
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Texture
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Sampler
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        })
    }

    /// Create the textured quad pipeline with a specific texture format.
    fn create_textured_pipeline_with_format(
        device: &wgpu::Device,
        bind_group_layout: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Textured Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/textured.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Textured Pipeline Layout"),
            bind_group_layouts: &[bind_group_layout],
            push_constant_ranges: &[],
        });

        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Textured Render Pipeline"),
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
                    format,
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

    /// Create a linear filtering sampler.
    fn create_sampler(device: &wgpu::Device) -> wgpu::Sampler {
        device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Texture Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        })
    }

    /// Create a GPU texture from RGBA pixel data.
    fn texture_from_rgba(
        &self,
        data: &[u8],
        width: u32,
        height: u32,
        label: &str,
    ) -> RenderResult<CachedTexture> {
        let expected_size = (width * height * 4) as usize;
        if data.len() != expected_size {
            return Err(RenderError::Frame(format!(
                "Invalid texture data size: expected {expected_size}, got {}",
                data.len()
            )));
        }

        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });

        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        Ok(CachedTexture { texture, view })
    }

    /// Remove a texture from the cache.
    pub fn invalidate_texture(&mut self, key: &str) {
        self.texture_cache.remove(key);
    }

    /// Clear all cached textures.
    pub fn clear_texture_cache(&mut self) {
        self.texture_cache.clear();
        tracing::debug!("Texture cache cleared");
    }

    /// Set the background color.
    pub fn set_background_color(&mut self, r: f64, g: f64, b: f64, a: f64) {
        self.background_color = wgpu::Color { r, g, b, a };
    }

    /// Reconfigure the surface with current settings.
    ///
    /// Call this after surface lost/outdated errors.
    ///
    /// # Errors
    ///
    /// Returns an error if no surface is configured.
    pub fn reconfigure_surface(&mut self) -> RenderResult<()> {
        if let (Some(surface), Some(config)) = (&self.surface, &self.surface_config) {
            surface.configure(&self.device, config);
            tracing::debug!("Surface reconfigured: {}x{}", config.width, config.height);
            Ok(())
        } else {
            Err(RenderError::Surface(
                "No surface to reconfigure".to_string(),
            ))
        }
    }

    /// Drop the surface (for suspend/minimize).
    ///
    /// The surface can be recreated by calling `from_window` again.
    pub fn drop_surface(&mut self) {
        self.surface = None;
        self.surface_config = None;
        tracing::debug!("Surface dropped");
    }

    /// Check if a surface is configured.
    #[must_use]
    pub fn has_surface(&self) -> bool {
        self.surface.is_some()
    }

    /// Get the current scale factor.
    #[must_use]
    pub fn scale_factor(&self) -> f64 {
        self.scale_factor
    }

    /// Set the scale factor (for Retina/HiDPI displays).
    pub fn set_scale_factor(&mut self, factor: f64) {
        if (self.scale_factor - factor).abs() > f64::EPSILON {
            tracing::info!("Scale factor changed: {} -> {}", self.scale_factor, factor);
            self.scale_factor = factor;
        }
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
        let caps = surface.get_capabilities(&self.get_adapter(Some(&surface))?);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| RenderError::GpuInit("No surface formats available".to_string()))?;

        let alpha_mode = caps
            .alpha_modes
            .first()
            .copied()
            .ok_or_else(|| RenderError::GpuInit("No alpha modes available".to_string()))?;

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width,
            height,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode,
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
    fn get_adapter(
        &self,
        compat_surface: Option<&wgpu::Surface<'_>>,
    ) -> RenderResult<wgpu::Adapter> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: compat_surface.or(self.surface.as_ref()),
            force_fallback_adapter: false,
        }))
        .ok_or_else(|| RenderError::GpuInit("Failed to get adapter".to_string()))
    }

    #[cfg(target_arch = "wasm32")]
    fn configure_canvas_surface(
        &mut self,
        canvas: HtmlCanvasElement,
        width: u32,
        height: u32,
    ) -> RenderResult<()> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        });

        let surface = instance
            .create_surface(wgpu::SurfaceTarget::Canvas(canvas))
            .map_err(|e| RenderError::Surface(e.to_string()))?;

        self.configure_surface(surface, width, height)
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

    /// Render a single element as a textured quad.
    #[allow(clippy::cast_precision_loss)] // Canvas dimensions fit in f32 mantissa (max ~16M)
    fn render_textured_element(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        element: &Element,
        texture_view: &wgpu::TextureView,
        is_first: bool,
    ) {
        // Create uniforms (color is not used for textured rendering but keep struct compatible)
        let uniforms = QuadUniforms {
            transform: [
                element.transform.x,
                element.transform.y,
                element.transform.width,
                element.transform.height,
            ],
            canvas_size: [self.width as f32, self.height as f32, 0.0, 0.0],
            color: [1.0, 1.0, 1.0, 1.0], // White (texture color will be used as-is)
        };

        // Update uniform buffer
        self.queue
            .write_buffer(&self.uniform_buffer, 0, bytemuck::cast_slice(&[uniforms]));

        // Create bind group with texture
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Textured Element Bind Group"),
            layout: &self.textured_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });

        // Create render pass
        let load_op = if is_first {
            wgpu::LoadOp::Clear(self.background_color)
        } else {
            wgpu::LoadOp::Load
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Textured Element Render Pass"),
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

        render_pass.set_pipeline(&self.textured_pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        render_pass.set_index_buffer(self.index_buffer.slice(..), wgpu::IndexFormat::Uint16);
        render_pass.draw_indexed(0..6, 0, 0..1);
    }

    /// Render a chart element to a texture, caching the result.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_chart_texture(
        &mut self,
        element: &Element,
        chart_type: &str,
        data: &serde_json::Value,
    ) -> RenderResult<()> {
        let key = element.id.to_string();

        // Skip if already cached
        if self.texture_cache.contains_key(&key) {
            return Ok(());
        }

        // Parse chart config and render to buffer
        let width = element.transform.width as u32;
        let height = element.transform.height as u32;

        let config = parse_chart_config(chart_type, data, width, height)?;
        let rgba_data = render_chart_to_buffer(&config)?;

        // Create GPU texture
        let label = format!("Chart: {key}");
        let cached = self.texture_from_rgba(&rgba_data, width, height, &label)?;
        self.texture_cache.insert(key, cached);

        tracing::debug!("Created chart texture {}x{}", width, height);

        Ok(())
    }

    /// Render an image element to a texture, caching the result.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_image_texture(&mut self, element: &Element, src: &str) -> RenderResult<()> {
        let key = element.id.to_string();

        // Skip if already cached
        if self.texture_cache.contains_key(&key) {
            return Ok(());
        }

        // Load image from data URI or create placeholder
        let texture_data = if src.starts_with("data:") {
            load_image_from_data_uri(src)?
        } else {
            // For non-data URIs, create a placeholder
            // TODO: Support file loading in native builds
            tracing::warn!("Non-data URI images not yet supported: {}", src);
            let width = element.transform.width as u32;
            let height = element.transform.height as u32;
            create_placeholder(width, height)
        };

        // Create GPU texture
        let label = format!("Image: {key}");
        let cached = self.texture_from_rgba(
            &texture_data.data,
            texture_data.width,
            texture_data.height,
            &label,
        )?;
        self.texture_cache.insert(key, cached);

        tracing::debug!(
            "Created image texture {}x{}",
            texture_data.width,
            texture_data.height
        );

        Ok(())
    }

    /// Render a video element to a texture, using placeholder if stream not available.
    ///
    /// For video elements, we check if a video texture exists for the `stream_id`.
    /// If not, we create a placeholder texture. The actual video frames are
    /// uploaded separately via `update_video_frame()`.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn render_video_texture(&mut self, element: &Element, stream_id: &str) -> RenderResult<()> {
        let key = element.id.to_string();

        // Skip if already cached
        if self.texture_cache.contains_key(&key) {
            return Ok(());
        }

        // Check if we have a video texture for this stream
        let video_key = format!("video:{stream_id}");
        if self.video_textures.contains_key(&video_key) {
            tracing::debug!(
                "Video stream {stream_id} has cached texture, using placeholder for now"
            );
        }

        // Create placeholder texture for video (dark gray)
        let width = element.transform.width.max(1.0) as u32;
        let height = element.transform.height.max(1.0) as u32;
        let placeholder = crate::image::create_solid_color(width, height, 32, 32, 32, 255);

        // Create GPU texture
        let label = format!("Video: {key}");
        let cached = self.texture_from_rgba(
            &placeholder.data,
            placeholder.width,
            placeholder.height,
            &label,
        )?;
        self.texture_cache.insert(key, cached);

        tracing::debug!(
            "Created video placeholder texture {}x{} for stream {stream_id}",
            width,
            height
        );

        Ok(())
    }

    /// Update a video frame for a specific stream.
    ///
    /// Call this when new video frame data is available from a WebRTC stream
    /// or media playback. The frame data should be RGBA format.
    ///
    /// # Errors
    ///
    /// Returns an error if the texture cannot be created.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    pub fn update_video_frame(
        &mut self,
        stream_id: &str,
        width: u32,
        height: u32,
        rgba_data: &[u8],
    ) -> RenderResult<()> {
        let expected_size = (width as usize) * (height as usize) * 4;
        if rgba_data.len() != expected_size {
            return Err(RenderError::Frame(format!(
                "Invalid video frame data size: expected {expected_size}, got {}",
                rgba_data.len()
            )));
        }

        let video_key = format!("video:{stream_id}");
        let label = format!("Video Stream: {stream_id}");

        let cached = self.texture_from_rgba(rgba_data, width, height, &label)?;
        self.video_textures.insert(video_key, cached);

        tracing::trace!(
            "Updated video frame {}x{} for stream {stream_id}",
            width,
            height
        );

        Ok(())
    }

    /// Remove a video stream texture.
    ///
    /// Call this when a video stream ends.
    pub fn remove_video_stream(&mut self, stream_id: &str) {
        let video_key = format!("video:{stream_id}");
        self.video_textures.remove(&video_key);
        tracing::debug!("Removed video stream texture: {stream_id}");
    }

    /// Clear all video stream textures.
    pub fn clear_video_textures(&mut self) {
        self.video_textures.clear();
        tracing::debug!("Cleared all video textures");
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

        // Helper to parse a hex byte at a given offset
        let parse_byte = |offset: usize| -> Option<f32> {
            let byte = u8::from_str_radix(&hex[offset..offset + 2], 16).ok()?;
            Some(f32::from(byte) / 255.0)
        };

        match hex.len() {
            6 => Some([parse_byte(0)?, parse_byte(2)?, parse_byte(4)?, 1.0]),
            8 => Some([
                parse_byte(0)?,
                parse_byte(2)?,
                parse_byte(4)?,
                parse_byte(6)?,
            ]),
            _ => None,
        }
    }

    /// Render scene elements to a texture view.
    ///
    /// Handles both empty scenes (clears to background) and scenes with elements.
    /// For Chart and Image elements, renders textures; otherwise renders colored quads.
    fn render_scene_elements(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        scene: &Scene,
    ) {
        let elements: Vec<_> = scene.elements().cloned().collect();

        if elements.is_empty() {
            // Clear to background color
            encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
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
            return;
        }

        // First pass: Prepare textures for Chart and Image elements
        for element in &elements {
            match &element.kind {
                ElementKind::Chart { chart_type, data } => {
                    if let Err(e) = self.render_chart_texture(element, chart_type, data) {
                        tracing::warn!("Failed to render chart texture: {e}");
                    }
                }
                ElementKind::Image { src, .. } => {
                    if let Err(e) = self.render_image_texture(element, src) {
                        tracing::warn!("Failed to render image texture: {e}");
                    }
                }
                ElementKind::Video { stream_id, .. } => {
                    if let Err(e) = self.render_video_texture(element, stream_id) {
                        tracing::warn!("Failed to render video texture: {e}");
                    }
                }
                _ => {}
            }
        }

        // Second pass: Render all elements
        for (i, element) in elements.iter().enumerate() {
            let is_first = i == 0;
            let key = element.id.to_string();

            // Check if we have a cached texture for this element
            if let Some(cached) = self.texture_cache.get(&key) {
                self.render_textured_element(encoder, view, element, &cached.view, is_first);
            } else {
                // Fallback to colored quad for non-textured elements
                self.render_element_quad(encoder, view, element, is_first);
            }
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

        self.render_scene_elements(&mut encoder, &view, scene);

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

        // Handle surface errors with recovery
        let output = match surface.get_current_texture() {
            Ok(output) => output,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                tracing::warn!("Surface lost/outdated, reconfiguring...");
                // Need to reborrow surface after reconfigure
                self.reconfigure_surface()?;
                // Retry getting texture
                self.surface
                    .as_ref()
                    .ok_or_else(|| RenderError::Surface("Surface lost during reconfigure".into()))?
                    .get_current_texture()
                    .map_err(|e| RenderError::Surface(format!("Failed after reconfigure: {e}")))?
            }
            Err(wgpu::SurfaceError::Timeout) => {
                tracing::warn!("Surface timeout, skipping frame");
                return Ok(());
            }
            Err(wgpu::SurfaceError::OutOfMemory) => {
                return Err(RenderError::Surface("GPU out of memory".to_string()));
            }
            Err(wgpu::SurfaceError::Other) => {
                return Err(RenderError::Surface("Unknown surface error".to_string()));
            }
        };

        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        self.render_scene_elements(&mut encoder, &view, scene);

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
