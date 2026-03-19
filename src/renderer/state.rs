use crate::voxel::{VoxelWorld, ChunkPos, GlobalVoxelId, SharedVoxelRegistry, TextureUV, VoxelFace};
use crate::renderer::voxel_map::{DirtyChunkSet, generate_chunk_mesh};
use crate::renderer::VoxelVertex;
use crate::renderer::mesh_system::MeshBuildSystem;
use crate::terrain::TerrainGenerator;
use crate::debug::EguiState;
use winit::window::Window;
use glam::{Mat4, Vec3A};
use wgpu::util::DeviceExt;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Structure de caméra
#[derive(Clone, Copy)]
pub struct Camera {
    pub position: Vec3A,
    pub pitch: f32, // Rotation autour de X (haut/bas)
    pub yaw: f32,   // Rotation autour de Y (gauche/droite)
}

impl Camera {
    pub fn new() -> Self {
        Self {
            position: Vec3A::new(32.0, 30.0, 32.0),
            pitch: -0.3,
            yaw: std::f32::consts::PI / 4.0,
        }
    }

    /// Obtenir la direction avant de la caméra
    pub fn forward(&self) -> Vec3A {
        let x = self.yaw.cos() * self.pitch.cos();
        let y = self.pitch.sin();
        let z = self.yaw.sin() * self.pitch.cos();
        Vec3A::new(x, y, z).normalize()
    }

    /// Obtenir la direction droite de la caméra
    pub fn right(&self) -> Vec3A {
        let forward = self.forward();
        Vec3A::new(0.0, 1.0, 0.0).cross(forward).normalize()
    }

    /// Obtenir la direction haute de la caméra
    pub fn up(&self) -> Vec3A {
        let forward = self.forward();
        let right = self.right();
        right.cross(forward).normalize()
    }

    /// Obtenir la matrice view-projection
    pub fn view_projection(&self, aspect_ratio: f32) -> Mat4 {
        let forward = self.forward();
        let right = self.right();
        let up = right.cross(forward);

        // Matrice de vue (look-at)
        let view = Mat4::look_at_rh(
            self.position.into(),
            (self.position + forward).into(),
            up.into(),
        );

        // Inverser Y pour le système de coordonnées wgpu (Y- vers le bas)
        let flip_y = Mat4::from_cols_array_2d(&[
            [1.0, 0.0, 0.0, 0.0],
            [0.0, -1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]);

        // Matrice de projection (perspective)
        let projection = Mat4::perspective_rh(std::f32::consts::PI / 4.0, aspect_ratio, 0.1, 1000.0);

        projection * flip_y * view
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

/// Mesh d'un chunk
struct ChunkMesh {
    vertex_buffer: wgpu::Buffer,
    vertex_count: u32,
}

/// Cible du highlight: position + face visée
#[derive(Clone, Copy, Debug)]
pub struct HighlightTarget {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub face: VoxelFace,
}

impl HighlightTarget {
    pub fn new(x: i32, y: i32, z: i32, face: VoxelFace) -> Self {
        Self { x, y, z, face }
    }
}

/// Position d'un bloc (backward compat)
#[derive(Clone, Copy, Debug)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl BlockPosition {
    pub fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }
}

pub struct WgpuState {
    // L'ordre des champs est important pour la Drop (inverse de l'ordre de déclaration)
    _instance: Box<wgpu::Instance>,
    _surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    // Texture atlas et bind group
    _atlas_texture: wgpu::Texture,
    _atlas_sampler: wgpu::Sampler,
    atlas_bind_group: wgpu::BindGroup,
    // Depth buffer
    depth_texture: wgpu::Texture,
    depth_texture_view: wgpu::TextureView,
    _voxel_pipeline: wgpu::RenderPipeline,
    _highlight_pipeline: wgpu::RenderPipeline,
    _highlight_time_bind_group_layout: wgpu::BindGroupLayout,
    highlight_time_buffer: wgpu::Buffer,
    highlight_time_bind_group: wgpu::BindGroup,
    queue: wgpu::Queue,
    device: wgpu::Device,

    // Monde voxel et meshs dynamiques
    world: Arc<RwLock<VoxelWorld>>,
    chunk_meshes: HashMap<ChunkPos, ChunkMesh>,
    mesh_rebuild: MeshBuildSystem,
    dirty_chunks: DirtyChunkSet,

    // Highlight du bloc visé
    highlight_vertex_buffer: Option<wgpu::Buffer>,
    highlight_face_buffer: Option<wgpu::Buffer>,
    highlight_target: Option<HighlightTarget>,

    // Temps écoulé depuis le début (pour l'animation de l'overlay)
    start_time: std::time::Instant,

    // Camera pour les déplacements
    camera: Camera,

    // Egui UI
    egui_state: EguiState,
    egui_renderer: egui_wgpu::Renderer,

    // Commande soumise depuis le chat
    submitted_command: Option<String>,

    // Flag indiquant que le chat vient de se fermer (pour recapture souris)
    chat_just_closed: bool,

    // Événements à passer à egui
    egui_events: Vec<egui::Event>,
    egui_mouse_position: (f32, f32),
}

impl WgpuState {
    pub async fn new(window: &Window) -> Self {
        let size = window.inner_size();

        let instance = Box::new(wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            ..Default::default()
        }));

        let surface = instance.create_surface(window).unwrap();
        let surface: wgpu::Surface<'static> = unsafe { std::mem::transmute(surface) };

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .unwrap();

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
            }, None)
            .await
            .unwrap();

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };

        surface.configure(&device, &surface_config);

        let voxel_pipeline = crate::renderer::pipeline::create_voxel_pipeline(&device, &surface_config);
        let (highlight_pipeline, highlight_time_bind_group_layout) =
            crate::renderer::pipeline::create_highlight_pipeline(&device, &surface_config);

        // Créer le buffer de caméra
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Camera Buffer"),
            size: 64, // Une matrice 4x4 = 16 floats * 4 octets
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Créer le bind group pour la caméra
        let camera_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Camera Bind Group Layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera Bind Group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Charger les configurations de blocks
        let block_configs = crate::renderer::atlas::load_block_configs()
            .unwrap_or_else(|e| {
                eprintln!("Warning: Failed to load block configs: {}", e);
                Vec::new()
            });

        // Collecter toutes les textures nécessaires
        let mut texture_names = Vec::new();
        for (_, config) in &block_configs {
            texture_names.push(config.texture.clone());
            if let Some(ref side) = config.texture_side {
                texture_names.push(side.clone());
            }
            if let Some(ref bottom) = config.texture_bottom {
                texture_names.push(bottom.clone());
            }
        }

        // Générer l'atlas de textures dynamiquement
        let (atlas_texture, atlas_texture_view, texture_uvs, texture_size_in_atlas) =
            crate::renderer::atlas::generate_texture_atlas(&texture_names, &device, &queue)
                .unwrap_or_else(|e| {
                    eprintln!("Failed to generate texture atlas: {}", e);
                    eprintln!("Using fallback atlas...");

                    // Fallback: créer un atlas vide avec une texture par défaut
                    let size = 16;
                    let texture = device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Fallback Atlas"),
                        size: wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8UnormSrgb,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

                    // Texture magenta par défaut
                    let pixel_count = (size * size) as usize;
                    let data: Vec<u8> = std::iter::once(255u8)
                        .chain(std::iter::once(0))
                        .chain(std::iter::once(255))
                        .chain(std::iter::once(255))
                        .cycle()
                        .take(pixel_count * 4)
                        .collect();
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
                            bytes_per_row: Some(size * 4),
                            rows_per_image: Some(size),
                        },
                        wgpu::Extent3d { width: size, height: size, depth_or_array_layers: 1 },
                    );

                    let mut uvs = HashMap::new();
                    uvs.insert("default".to_string(), (0.0, 0.0, 1.0, 1.0));

                    (texture, view, uvs, 1.0) // texture_size = 1.0 pour l'atlas fallback
                });

        // Créer le sampler pour l'atlas
        let atlas_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Créer le bind group pour l'atlas (doit correspondre au layout de la pipeline)
        // La pipeline a: binding 0 (camera), binding 1 (texture), binding 2 (sampler)
        let atlas_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Atlas Bind Group Layout"),
            entries: &[
                // 0: Camera uniform
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
                // 1: Texture atlas
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

        let atlas_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Atlas Bind Group"),
            layout: &atlas_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        // Créer le depth buffer
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth Texture"),
            size: wgpu::Extent3d {
                width: surface_config.width,
                height: surface_config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth24PlusStencil8,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor {
            label: Some("Depth Texture View"),
            format: Some(wgpu::TextureFormat::Depth24PlusStencil8),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            mip_level_count: None,
            base_mip_level: 0,
            array_layer_count: None,
            base_array_layer: 0,
        });

        // Créer le buffer de temps pour l'animation de l'overlay
        let highlight_time_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Highlight Time Buffer"),
            size: 4, // Un seul f32
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let highlight_time_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Highlight Time Bind Group"),
            layout: &highlight_time_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: highlight_time_buffer.as_entire_binding(),
            }],
        });

        let start_time = std::time::Instant::now();

        // Initialize egui state
        let egui_state = EguiState::new(window);

        // Créer le registry de voxels
        let registry = SharedVoxelRegistry::new();

        // Enregistrer les blocks avec leurs UVs depuis l'atlas
        if !block_configs.is_empty() {
            let mut blocks_data = Vec::new();
            for (string_id, config) in block_configs {
                // Récupérer les UVs pour chaque texture
                let get_uv = |name: &str| -> TextureUV {
                    let (u_min, v_min, u_max, v_max) = texture_uvs.get(name)
                        .copied()
                        .unwrap_or((0.0, 0.0, 1.0, 1.0));
                    TextureUV::new(u_min, v_min, u_max, v_max, texture_size_in_atlas)
                };

                let uv_top = get_uv(&config.texture);
                let uv_side = get_uv(&config.texture_side.as_ref().unwrap_or(&config.texture));
                let uv_bottom = get_uv(&config.texture_bottom.as_ref().unwrap_or(&config.texture));

                blocks_data.push((string_id, config, uv_top, uv_side, uv_bottom));
            }
            registry.register_with_uvs(blocks_data, texture_size_in_atlas);
        } else {
            // Fallback: utiliser les blocks par défaut
            eprintln!("Using fallback hardcoded blocks...");
            registry.register_voxel("grass", "Grass", 0);
            registry.register_voxel("dirt", "Dirt", 1);
            registry.register_voxel("bedrock", "Bedrock", 2);
            registry.register_voxel("stone", "Stone", 3);
        }

        // Générer le monde voxel
        let mut world = VoxelWorld::new(registry.clone());
        let generator = TerrainGenerator::new();
        generator.generate_test_world(&mut world);

        let world_arc = Arc::new(RwLock::new(world));

        // Créer le système de rebuild de meshs
        let mesh_rebuild = MeshBuildSystem::new(world_arc.clone());

        // Générer les meshs initiaux pour tous les chunks
        let chunk_meshes = Self::generate_initial_meshes(&device, &world_arc);

        println!("Generated {} chunk meshes", chunk_meshes.len());

        // Initialize egui renderer
        let egui_renderer = egui_wgpu::Renderer::new(&device, surface_config.format, None, 1);

        Self {
            _instance: instance,
            _surface: surface,
            surface_config,
            camera_buffer,
            camera_bind_group,
            _atlas_texture: atlas_texture,
            _atlas_sampler: atlas_sampler,
            atlas_bind_group,
            depth_texture,
            depth_texture_view,
            _voxel_pipeline: voxel_pipeline,
            _highlight_pipeline: highlight_pipeline,
            _highlight_time_bind_group_layout: highlight_time_bind_group_layout,
            highlight_time_buffer,
            highlight_time_bind_group,
            queue,
            device,
            world: world_arc,
            chunk_meshes,
            mesh_rebuild,
            dirty_chunks: DirtyChunkSet::new(),
            highlight_vertex_buffer: None,
            highlight_face_buffer: None,
            highlight_target: None,
            start_time,
            camera: Camera::new(),
            egui_state,
            egui_renderer,
            submitted_command: None,
            chat_just_closed: false,
            egui_events: Vec::new(),
            egui_mouse_position: (0.0, 0.0),
        }
    }

    /// Générer les meshs initiaux pour tous les chunks
    fn generate_initial_meshes(device: &wgpu::Device, world: &Arc<RwLock<VoxelWorld>>) -> HashMap<ChunkPos, ChunkMesh> {
        let mut chunk_meshes = HashMap::new();

        let world_read = world.read().unwrap();
        let registry = world_read.registry();
        for (&(cx, cy, cz), chunk) in world_read.chunks_iter() {
            let vertices = generate_chunk_mesh(chunk, &world_read, cx, cy, cz, registry);

            if !vertices.is_empty() {
                let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("Chunk Mesh ({}, {}, {})", cx, cy, cz)),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });

                chunk_meshes.insert((cx, cy, cz), ChunkMesh {
                    vertex_buffer,
                    vertex_count: vertices.len() as u32,
                });
            }
        }

        chunk_meshes
    }

    /// Définir un voxel et marquer les chunks comme dirty
    pub fn set_voxel(&mut self, x: i32, y: i32, z: i32, global_id: Option<GlobalVoxelId>) {
        if let Ok(mut world) = self.world.write() {
            let result = world.set_voxel(x, y, z, global_id);

            // Marquer le chunk modifié comme dirty
            let (cx, cy, cz) = result.modified_chunk;
            self.dirty_chunks.mark_dirty(cx, cy, cz);

            // Marquer les chunks voisins comme dirty
            for (nx, ny, nz) in result.neighbor_chunks {
                self.dirty_chunks.mark_dirty(nx, ny, nz);
            }
        }
    }

    /// Obtenir un voxel
    pub fn get_voxel(&self, x: i32, y: i32, z: i32) -> Option<GlobalVoxelId> {
        if let Ok(world) = self.world.read() {
            world.get_voxel(x, y, z)
        } else {
            None
        }
    }

    /// Définir la cible du highlight (avec face)
    pub fn set_highlight_target(&mut self, target: Option<HighlightTarget>) {
        self.highlight_target = target;
        if target.is_none() {
            self.highlight_vertex_buffer = None;
            self.highlight_face_buffer = None;
        }
    }

    /// Mettre à jour le mesh de highlight
    fn update_highlight_mesh(&mut self) {
        if let Some(target) = self.highlight_target {
            // Créer les vertices pour le highlight (wireframe cube)
            let vertices = Self::create_highlight_vertices(target.x, target.y, target.z);

            if !vertices.is_empty() {
                let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Highlight Vertex Buffer"),
                    contents: bytemuck::cast_slice(&vertices),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
                self.highlight_vertex_buffer = Some(vertex_buffer);
            }

            // Créer les vertices pour la face colorée
            let face_vertices = Self::create_face_overlay(target.x, target.y, target.z, target.face);

            if !face_vertices.is_empty() {
                let face_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Face Overlay Buffer"),
                    contents: bytemuck::cast_slice(&face_vertices),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });
                self.highlight_face_buffer = Some(face_buffer);
            }
        }
    }

    /// Créer les vertices pour un highlight de bloc (lignes)
    fn create_highlight_vertices(x: i32, y: i32, z: i32) -> Vec<VoxelVertex> {

        let mut vertices = Vec::new();

        // Créer des lignes sur les arêtes du bloc
        let edges = [
            // Bottom edges
            ([0.0, 0.0, 0.0], [1.0, 0.0, 0.0]),
            ([1.0, 0.0, 0.0], [1.0, 0.0, 1.0]),
            ([1.0, 0.0, 1.0], [0.0, 0.0, 1.0]),
            ([0.0, 0.0, 1.0], [0.0, 0.0, 0.0]),
            // Top edges
            ([0.0, 1.0, 0.0], [1.0, 1.0, 0.0]),
            ([1.0, 1.0, 0.0], [1.0, 1.0, 1.0]),
            ([1.0, 1.0, 1.0], [0.0, 1.0, 1.0]),
            ([0.0, 1.0, 1.0], [0.0, 1.0, 0.0]),
            // Vertical edges
            ([0.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
            ([1.0, 0.0, 0.0], [1.0, 1.0, 0.0]),
            ([1.0, 0.0, 1.0], [1.0, 1.0, 1.0]),
            ([0.0, 0.0, 1.0], [0.0, 1.0, 1.0]),
        ];

        for (start, end) in edges {
            vertices.push(VoxelVertex {
                position: start,
                normal: [0.0, 1.0, 0.0],
                voxel_pos: [x, y, z],
                uv: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            });
            vertices.push(VoxelVertex {
                position: end,
                normal: [0.0, 1.0, 0.0],
                voxel_pos: [x, y, z],
                uv: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }

        vertices
    }

    /// Créer les vertices pour l'overlay de la face visée (quad coloré semi-transparent)
    fn create_face_overlay(x: i32, y: i32, z: i32, face: VoxelFace) -> Vec<VoxelVertex> {
        let mut vertices = Vec::new();

        // Légèrement décalé vers la caméra pour éviter le z-fighting
        let offset = 0.001;

        let face_verts = match face {
            VoxelFace::Top => {
                let y = 1.0 + offset;
                [
                    [0.0, y, 1.0], [1.0, y, 1.0], [1.0, y, 0.0],
                    [0.0, y, 1.0], [1.0, y, 0.0], [0.0, y, 0.0],
                ]
            }
            VoxelFace::Bottom => {
                let y = 0.0 - offset;
                [
                    [0.0, y, 0.0], [1.0, y, 0.0], [1.0, y, 1.0],
                    [0.0, y, 0.0], [1.0, y, 1.0], [0.0, y, 1.0],
                ]
            }
            VoxelFace::North => {
                let z = 1.0 + offset;
                [
                    [0.0, 1.0, z], [0.0, 0.0, z], [1.0, 0.0, z],
                    [0.0, 1.0, z], [1.0, 0.0, z], [1.0, 1.0, z],
                ]
            }
            VoxelFace::South => {
                let z = 0.0 - offset;
                [
                    [1.0, 1.0, z], [1.0, 0.0, z], [0.0, 0.0, z],
                    [1.0, 1.0, z], [0.0, 0.0, z], [0.0, 1.0, z],
                ]
            }
            VoxelFace::East => {
                let x = 1.0 + offset;
                [
                    [x, 1.0, 1.0], [x, 0.0, 1.0], [x, 0.0, 0.0],
                    [x, 1.0, 1.0], [x, 0.0, 0.0], [x, 1.0, 0.0],
                ]
            }
            VoxelFace::West => {
                let x = 0.0 - offset;
                [
                    [x, 1.0, 0.0], [x, 0.0, 0.0], [x, 0.0, 1.0],
                    [x, 1.0, 0.0], [x, 0.0, 1.0], [x, 1.0, 1.0],
                ]
            }
        };

        let normal = face.normal();
        for pos in face_verts {
            vertices.push(VoxelVertex {
                position: pos,
                normal: [normal.x as f32, normal.y as f32, normal.z as f32],
                voxel_pos: [x, y, z],
                uv: [0.0, 0.0],
                color: [1.0, 1.0, 1.0, 1.0],
            });
        }

        vertices
    }

    /// Traiter les mises à jour de meshs (meshs rebuildés)
    pub fn process_mesh_updates(&mut self) {
        // Poll les meshs rebuildés
        while let Some(rebuilt) = self.mesh_rebuild.try_recv() {
            if rebuilt.vertices.is_empty() {
                // Si le mesh est vide, supprimer le chunk du hashmap
                self.chunk_meshes.remove(&rebuilt.chunk_pos);
            } else {
                let (cx, cy, cz) = rebuilt.chunk_pos;
                let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(&format!("Chunk Mesh ({}, {}, {})", cx, cy, cz)),
                    contents: bytemuck::cast_slice(&rebuilt.vertices),
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                });

                self.chunk_meshes.insert(rebuilt.chunk_pos, ChunkMesh {
                    vertex_buffer,
                    vertex_count: rebuilt.vertices.len() as u32,
                });
            }
        }

        // Demander le rebuild des chunks dirty
        let dirty = self.dirty_chunks.take_dirty();
        if !dirty.is_empty() {
            self.mesh_rebuild.request_rebuild_many(&dirty);
        }

        // Mettre à jour le highlight
        self.update_highlight_mesh();
    }

    /// Demander un rebuild immédiat pour un chunk spécifique
    pub fn request_immediate_rebuild(&mut self, cx: i32, cy: i32, cz: i32) {
        self.dirty_chunks.mark_dirty(cx, cy, cz);
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self._surface.configure(&self.device, &self.surface_config);

            // Update egui pixels_per_point
            // Note: in a real app you'd get this from the window, but here we assume 1.0
            self.egui_state.update_pixels_per_point(1.0);

            // Recréer le depth buffer avec les nouvelles dimensions
            self.depth_texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Depth Texture"),
                size: wgpu::Extent3d {
                    width: new_size.width,
                    height: new_size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Depth24PlusStencil8,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });

            self.depth_texture_view = self.depth_texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("Depth Texture View"),
                format: Some(wgpu::TextureFormat::Depth24PlusStencil8),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                mip_level_count: None,
                base_mip_level: 0,
                array_layer_count: None,
                base_array_layer: 0,
            });
        }
    }

    pub fn render(&mut self, selected_block: (u32, String)) -> Result<(), wgpu::SurfaceError> {
        self.egui_state.update_fps();

        let aspect_ratio = self.surface_config.width as f32 / self.surface_config.height as f32;
        let view_proj = self.camera.view_projection(aspect_ratio);
        let view_proj_array: [[f32; 4]; 4] = view_proj.to_cols_array_2d();
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[view_proj_array]));

        let elapsed = self.start_time.elapsed().as_secs_f32();
        self.queue.write_buffer(&self.highlight_time_buffer, 0, bytemuck::cast_slice(&[elapsed]));

        let output = self._surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let world_ref = if let Ok(w) = self.world.read() { w } else { return Ok(()) };

        // Calculate chunk coordinates
        let pos = self.camera.position;
        let chunk_x = pos.x.floor() as i32 / 16;
        let chunk_y = pos.y.floor() as i32 / 16;
        let chunk_z = pos.z.floor() as i32 / 16;

        // Calculate yaw and pitch (convert radians to degrees)
        let yaw = self.camera.yaw * 180.0 / std::f32::consts::PI;
        let pitch = self.camera.pitch * 180.0 / std::f32::consts::PI;

        let block_count = world_ref.registry().len();

        // Update egui - set pixels_per_point on context before frame
        self.egui_state.ctx.set_pixels_per_point(self.egui_state.pixels_per_point);

        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_max(
                egui::pos2(0.0, 0.0),
                egui::pos2(self.surface_config.width as f32, self.surface_config.height as f32),
            )),
            events: std::mem::take(&mut self.egui_events),
            ..Default::default()
        };
        self.egui_state.context_mut().begin_frame(raw_input);

        self.egui_state.show_debug_ui(
            (pos.x, pos.y, pos.z),
            yaw,
            pitch,
            (chunk_x, chunk_y, chunk_z),
            selected_block,
            block_count,
        );

        // Show chat UI and get submitted command
        let was_chat_open = self.egui_state.is_chat_open();
        if let Some(command) = self.egui_state.show_chat_ui() {
            self.submitted_command = Some(command);
        }
        // Si le chat était ouvert et est maintenant fermé, set le flag
        if was_chat_open && !self.egui_state.is_chat_open() {
            self.chat_just_closed = true;
        }

        let egui_output = self.egui_state.context_mut().end_frame();
        let paint_jobs = self.egui_state.context().tessellate(egui_output.shapes, self.egui_state.pixels_per_point);

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Render Encoder"),
        });

        let screen_desc = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.surface_config.width, self.surface_config.height],
            pixels_per_point: self.egui_state.pixels_per_point,
        };

        for (id, image_delta) in &egui_output.textures_delta.set {
            self.egui_renderer.update_texture(&self.device, &self.queue, *id, image_delta);
        }

        self.egui_renderer.update_buffers(
            &self.device,
            &self.queue,
            &mut encoder,
            &paint_jobs,
            &screen_desc,
        );

        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.5,
                            g: 0.7,
                            b: 0.9,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_texture_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_pipeline(&self._voxel_pipeline);
            render_pass.set_bind_group(0, &self.atlas_bind_group, &[]);

            for (_chunk_pos, mesh) in &self.chunk_meshes {
                render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                render_pass.draw(0..mesh.vertex_count, 0..1);
            }

            if let Some(ref face_buffer) = self.highlight_face_buffer {
                render_pass.set_pipeline(&self._highlight_pipeline);
                render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                render_pass.set_bind_group(1, &self.highlight_time_bind_group, &[]);
                render_pass.set_vertex_buffer(0, face_buffer.slice(..));
                render_pass.draw(0..6, 0..1);
            }
        }

        // New render pass for egui
        {
            let mut egui_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Egui Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.egui_renderer.render(
                &mut egui_pass,
                &paint_jobs,
                &screen_desc,
            );
        }

        for id in &egui_output.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
        Ok(())
    }

    /// Obtenir une référence au monde (pour raycasting)
    pub fn world(&self) -> &Arc<RwLock<VoxelWorld>> {
        &self.world
    }

    /// Get a reference to the camera.
    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Get a mutable reference to the camera.
    pub fn camera_mut(&mut self) -> &mut Camera {
        &mut self.camera
    }

    /// Toggle debug UI
    pub fn toggle_debug_ui(&mut self) {
        self.egui_state.enabled = !self.egui_state.enabled;
    }

    /// Check if debug UI is enabled
    pub fn is_debug_ui_enabled(&self) -> bool {
        self.egui_state.enabled
    }

    /// Open chat
    pub fn open_chat(&mut self) {
        self.egui_state.open_chat();
    }

    /// Close chat
    pub fn close_chat(&mut self) {
        self.egui_state.close_chat();
    }

    /// Check if chat is open
    pub fn is_chat_open(&self) -> bool {
        self.egui_state.is_chat_open()
    }

    /// Check and consume the "chat just closed" flag
    pub fn take_chat_just_closed(&mut self) -> bool {
        let result = self.chat_just_closed;
        self.chat_just_closed = false;
        result
    }

    /// Add message to chat
    pub fn add_chat_message(&mut self, text: String, is_command: bool) {
        self.egui_state.add_chat_message(text, is_command);
    }

    /// Clear chat history
    pub fn clear_chat(&mut self) {
        self.egui_state.clear_chat();
    }

    /// Get submitted command from chat (and clear it)
    pub fn get_submitted_command(&mut self) -> Option<String> {
        self.submitted_command.take()
    }

    /// Get egui context (for input handling)
    pub fn egui_context(&self) -> &egui::Context {
        self.egui_state.context()
    }

    /// Enregistrer un événement clavier pour egui
    pub fn handle_key_event(&mut self, key: &winit::keyboard::Key, pressed: bool) {
        use winit::keyboard::Key;
        use winit::keyboard::NamedKey;

        if self.egui_state.is_chat_open() {
            // Convertir winit::Key en egui::Key
            let egui_key = match key {
                Key::Named(named) => match named {
                    NamedKey::Escape => Some(egui::Key::Escape),
                    NamedKey::Enter => Some(egui::Key::Enter),
                    NamedKey::Tab => Some(egui::Key::Tab),
                    NamedKey::Backspace => Some(egui::Key::Backspace),
                    NamedKey::Space => Some(egui::Key::Space),
                    NamedKey::Delete => Some(egui::Key::Delete),
                    NamedKey::Insert => Some(egui::Key::Insert),
                    NamedKey::Home => Some(egui::Key::Home),
                    NamedKey::End => Some(egui::Key::End),
                    NamedKey::PageUp => Some(egui::Key::PageUp),
                    NamedKey::PageDown => Some(egui::Key::PageDown),
                    NamedKey::ArrowLeft => Some(egui::Key::ArrowLeft),
                    NamedKey::ArrowRight => Some(egui::Key::ArrowRight),
                    NamedKey::ArrowUp => Some(egui::Key::ArrowUp),
                    NamedKey::ArrowDown => Some(egui::Key::ArrowDown),
                    _ => None,
                },
                Key::Character(s) => {
                    // Pour les caractères, on utilise Event::Text
                    if pressed && !s.is_empty() {
                        let c = s.chars().next().unwrap();
                        if c.is_ascii() && !c.is_ascii_control() {
                            self.egui_events.push(egui::Event::Text(s.to_string()));
                        }
                    }
                    None
                },
                _ => None,
            };

            if let Some(ek) = egui_key {
                self.egui_events.push(egui::Event::Key {
                    key: ek,
                    pressed,
                    physical_key: Some(egui::Key::Escape), // Placeholder
                    repeat: false,
                    modifiers: egui::Modifiers::default(),
                });

                // Pour space, aussi envoyer un event Text pour que ça s'affiche
                if pressed && ek == egui::Key::Space {
                    self.egui_events.push(egui::Event::Text(" ".to_string()));
                }
            }

            // Gérer les caractères comme événements Key aussi
            if let Key::Character(s) = key {
                if pressed && !s.is_empty() {
                    let c = s.chars().next().unwrap();
                    if c.is_ascii() && !c.is_ascii_control() {
                        if let Some(named_key) = egui::Key::from_name(s) {
                            self.egui_events.push(egui::Event::Key {
                                key: named_key,
                                pressed: true,
                                physical_key: Some(egui::Key::Escape), // Placeholder
                                repeat: false,
                                modifiers: egui::Modifiers::default(),
                            });
                        }
                    }
                }
            }
        }
    }

    /// Enregistrer la position de la souris pour egui
    pub fn handle_mouse_move(&mut self, x: f64, y: f64) {
        self.egui_mouse_position = (x as f32, y as f32);
    }

    /// Enregistrer un clic de souris pour egui
    pub fn handle_mouse_click(&mut self, pressed: bool, button: u16) {
        if self.egui_state.is_chat_open() {
            let button = match button {
                1 => egui::PointerButton::Primary,
                2 => egui::PointerButton::Middle,
                3 => egui::PointerButton::Secondary,
                _ => return,
            };
            self.egui_events.push(egui::Event::PointerButton {
                button,
                pressed,
                pos: egui::pos2(self.egui_mouse_position.0, self.egui_mouse_position.1),
                modifiers: egui::Modifiers::default(),
            });
        }
    }

    /// Vérifier si egui veut capturer la souris
    pub fn egui_wants_pointer(&self) -> bool {
        self.egui_state.is_chat_open()
    }

    /// Vérifier si egui veut capturer le clavier
    pub fn egui_wants_keyboard(&self) -> bool {
        self.egui_state.is_chat_open()
    }
}
