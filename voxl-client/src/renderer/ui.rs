//! UI rendering system for 2D screen elements
//!
//! Used for crosshair, hotbar, etc.

use wgpu;
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct UIVertex {
    pub position: [f32; 2],  // Screen position in pixels (x, y) from top-left
    pub uv: [f32; 2],        // Texture coordinates (0-1)
    pub color: [f32; 4],      // RGBA color (0-1)
}

impl UIVertex {
    /// Create a new UI vertex
    pub fn new(x: f32, y: f32, u: f32, v: f32, color: [f32; 4]) -> Self {
        Self {
            position: [x, y],
            uv: [u, v],
            color,
        }
    }

    /// Create a quad (4 vertices) for rendering
    pub fn quad(x: f32, y: f32, width: f32, height: f32, color: [f32; 4]) -> [Self; 4] {
        [
            // Top-left
            Self::new(x, y, 0.0, 0.0, color),
            // Top-right
            Self::new(x + width, y, 1.0, 0.0, color),
            // Bottom-left
            Self::new(x, y + height, 0.0, 1.0, color),
            // Bottom-right
            Self::new(x + width, y + height, 1.0, 1.0, color),
        ]
    }
}

/// Create the UI render pipeline
pub fn create_ui_pipeline(
    device: &wgpu::Device,
    surface_config: &wgpu::SurfaceConfiguration,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("UI Shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("../../../assets/shaders/ui.wgsl").into()),
    });

    let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("UI Bind Group Layout"),
        entries: &[
            // 0: Screen size uniform
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            // 1: Texture (optional - can be null for solid colors)
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    multisampled: false,
                    view_dimension: wgpu::TextureViewDimension::D2,
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                },
                count: None,
            },
            // 2: Sampler
            wgpu::BindGroupLayoutEntry {
                binding: 2,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("UI Pipeline Layout"),
        bind_group_layouts: &[&bind_group_layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("UI Render Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: "vs_main",
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<UIVertex>() as wgpu::BufferAddress,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &[
                    wgpu::VertexAttribute {
                        offset: 0,
                        shader_location: 0,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                        shader_location: 1,
                        format: wgpu::VertexFormat::Float32x2,
                    },
                    wgpu::VertexAttribute {
                        offset: (std::mem::size_of::<[f32; 2]>() * 2) as wgpu::BufferAddress,
                        shader_location: 2,
                        format: wgpu::VertexFormat::Float32x4,
                    },
                ],
            }],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: "fs_main",
            targets: &[Some(wgpu::ColorTargetState {
                format: surface_config.format.add_srgb_suffix(),
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            strip_index_format: None,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: None,
            polygon_mode: wgpu::PolygonMode::Fill,
            unclipped_depth: false,
            conservative: false,
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
    });

    pipeline
}

/// Create the screen size uniform buffer
pub fn create_screen_uniform_buffer(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("UI Screen Size Buffer"),
        size: std::mem::size_of::<[f32; 2]>() as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

/// Generate a 16x16 crosshair texture with anti-aliased circle
/// Like Minecraft: small dark circle with smooth edges
pub fn create_crosshair_texture(device: &wgpu::Device) -> wgpu::Texture {
    const SIZE: usize = 16;
    const CENTER: f32 = SIZE as f32 / 2.0;
    const RADIUS: f32 = 2.5;  // Slightly larger for anti-aliasing

    let mut data = [0u8; SIZE * SIZE * 4]; // RGBA

    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = (y * SIZE + x) * 4;
            let dx = x as f32 - CENTER + 0.5;
            let dy = y as f32 - CENTER + 0.5;
            let distance = (dx * dx + dy * dy).sqrt();

            // Anti-aliased circle using smoothstep
            let alpha = if distance < RADIUS - 0.5 {
                1.0  // Fully inside
            } else if distance < RADIUS + 0.5 {
                // Anti-aliasing edge (0.5 pixel fade)
                (RADIUS + 0.5 - distance)  // 0.0 to 1.0
            } else {
                0.0  // Fully outside
            };

            if alpha > 0.01 {
                // Dark color like Minecraft (slightly transparent black)
                let intensity = 40.0 * alpha;  // Max intensity 40/255
                data[idx] = intensity as u8;     // R
                data[idx + 1] = intensity as u8; // G
                data[idx + 2] = intensity as u8; // B
                data[idx + 3] = (alpha * 220.0) as u8;  // A (slightly transparent)
            }
        }
    }

    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Crosshair Texture"),
        size: wgpu::Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    })
}

/// Upload crosshair data to texture
pub fn upload_crosshair_texture(_device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) {
    const SIZE: usize = 16;
    const CENTER: f32 = SIZE as f32 / 2.0;
    const RADIUS: f32 = 2.5;

    let mut data = [0u8; SIZE * SIZE * 4]; // RGBA

    for y in 0..SIZE {
        for x in 0..SIZE {
            let idx = (y * SIZE + x) * 4;
            let dx = x as f32 - CENTER + 0.5;
            let dy = y as f32 - CENTER + 0.5;
            let distance = (dx * dx + dy * dy).sqrt();

            let alpha = if distance < RADIUS - 0.5 {
                1.0
            } else if distance < RADIUS + 0.5 {
                (RADIUS + 0.5 - distance)
            } else {
                0.0
            };

            if alpha > 0.01 {
                let intensity = 40.0 * alpha;
                data[idx] = intensity as u8;
                data[idx + 1] = intensity as u8;
                data[idx + 2] = intensity as u8;
                data[idx + 3] = (alpha * 220.0) as u8;
            }
        }
    }

    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &data,
        wgpu::ImageDataLayout {
            offset: 0,
            bytes_per_row: Some(SIZE as u32 * 4),
            rows_per_image: Some(SIZE as u32),
        },
        wgpu::Extent3d {
            width: SIZE as u32,
            height: SIZE as u32,
            depth_or_array_layers: 1,
        },
    );
}
