use voxl_common::voxel::{VoxelWorld, ChunkPos, GlobalVoxelId, SharedVoxelRegistry, TextureUV, VoxelFace, CHUNK_SIZE, VoxelChunk, chunk::VERTICAL_CHUNKS};
use crate::chunk_tracker_compat::SharedChunkTracker;
use crate::renderer::queue_system::{ChunkGenerationQueue, MeshQueue, MeshPriority};
use crate::renderer::pipeline::ChunkBorderVertex;
use crate::renderer::VoxelVertex;
use crate::worldgen::WorldGenerator;
use crate::debug::EguiState;
use voxl_common::entities::{EntityWorld, GameMode};
use voxl_common::config::GameConfig;
use crate::performance::PerformanceCollector;
use std::collections::{HashMap, HashSet};
use tracing::{info, warn, error, debug};
use winit::window::Window;
use glam::{Mat4, Vec3A};
use wgpu::util::DeviceExt;
use std::sync::{Arc, RwLock};

/// Modes d'affichage des bordures de chunks
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChunkBorderMode {
    Disabled,
    ChunkBorders,      // Lignes entre les chunks seulement
    FullGrid,          // Lignes entre chunks + grille 16x16
}

impl ChunkBorderMode {
    pub fn next(self) -> Self {
        match self {
            ChunkBorderMode::Disabled => ChunkBorderMode::ChunkBorders,
            ChunkBorderMode::ChunkBorders => ChunkBorderMode::FullGrid,
            ChunkBorderMode::FullGrid => ChunkBorderMode::Disabled,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ChunkBorderMode::Disabled => "désactivé",
            ChunkBorderMode::ChunkBorders => "bordures de chunks",
            ChunkBorderMode::FullGrid => "grille complète",
        }
    }
}

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
            position: Vec3A::new(0.0, 80.0, 0.0),
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
    // Configuration du jeu
    config: GameConfig,

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
    _chunk_border_pipeline: wgpu::RenderPipeline,
    chunk_border_camera_bind_group: wgpu::BindGroup,
    chunk_border_vertex_buffer: wgpu::Buffer,
    chunk_border_vertex_count: u32,
    chunk_border_mode: ChunkBorderMode,
    queue: wgpu::Queue,
    device: wgpu::Device,

    // Monde voxel et meshs dynamiques
    world: Arc<RwLock<VoxelWorld>>,
    chunk_meshes: HashMap<ChunkPos, ChunkMesh>,
    visible_chunks: Vec<ChunkPos>,  // Chunks visibles ce frame (frustum culling)

    // NOUVEAU: Chunk tracking avec dirty system
    chunk_tracker: SharedChunkTracker,
    chunk_gen_queue: Option<ChunkGenerationQueue>,  // DISABLED: Server-only chunk generation
    mesh_queue: Option<MeshQueue>,

    // Pending mesh requests (batching pour éviter les duplicats lors de la génération)
    pending_mesh_requests: HashSet<ChunkPos>,

    // Monde des entités ECS
    entity_world: EntityWorld,

    // Highlight du bloc visé
    highlight_vertex_buffer: Option<wgpu::Buffer>,
    highlight_face_buffer: Option<wgpu::Buffer>,
    highlight_target: Option<HighlightTarget>,

    // Temps écoulé depuis le début (pour l'animation de l'overlay)
    start_time: std::time::Instant,

    // Timer pour le logging de statut
    last_status_log: std::time::Instant,
    // Système de ticks (10 ticks par seconde = 100ms par tick)
    tick_accumulator: std::time::Duration,
    last_tick_time: std::time::Instant,

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

    // Performance tracking
    performance_collector: PerformanceCollector,
}

impl WgpuState {
    pub async fn new(window: &Window, config: &voxl_common::config::GameConfig) -> Self {
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

        // Choisir le present_mode selon le VSync
        // Use Mailbox for non-vsync as it's widely supported (including Wayland)
        // Immediate is not available on some platforms (like Wayland)
        let present_mode = if config.graphics.vsync {
            wgpu::PresentMode::Fifo  // VSync actif
        } else {
            wgpu::PresentMode::Mailbox  // Low latency, widely supported
        };

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode,
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
                warn!("Warning: Failed to load block configs: {}", e);
                Vec::new()
            });

        // Créer le registry de voxels AVANT de charger les configs
        let registry = SharedVoxelRegistry::new();

        // IMPORTANT: Charger les modèles AVANT les blocs !
        // Les blocs ont besoin des modèles pour être enregistrés
        let mut model_textures = Vec::new();
        if let Ok(textures) = registry.load_models() {
            model_textures = textures;
        }

        // Charger les définitions de blocs depuis le dossier blocks/
        // (après avoir chargé les modèles)
        if let Err(e) = registry.load_from_folder() {
            tracing::warn!("Failed to load block definitions: {}", e);
        }

        // Collecter toutes les textures nécessaires (y compris des modèles)
        let mut texture_names = model_textures.clone();
        let mut blocks_with_models = 0;
        let mut blocks_without_models = 0;
        let mut missing_models = Vec::new();

        for (string_id, config) in &block_configs {
            if config.uses_model() {
                // Ajouter les textures du modèle
                if let Some(model_name) = &config.model {
                    if let Some(model) = registry.get_model(model_name) {
                        let textures = model.get_used_textures();
                        for tex in textures {
                            if !texture_names.contains(&tex) {
                                texture_names.push(tex);
                            }
                        }
                        blocks_with_models += 1;
                    } else {
                        missing_models.push((string_id.clone(), model_name.clone()));
                    }
                }
            } else {
                blocks_without_models += 1;
            }
        }

        info!("Textures to load: {} (from {} blocks with models, {} without)",
            texture_names.len(), blocks_with_models, blocks_without_models);

        if !missing_models.is_empty() {
            warn!("Warning: {} blocks reference missing models: {}",
                missing_models.len(),
                missing_models.iter()
                    .map(|(b, m)| format!("'{}' -> '{}'", b, m))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        // Générer l'atlas de textures dynamiquement
        let (atlas_texture, atlas_texture_view, texture_uvs, texture_size_in_atlas) =
            crate::renderer::atlas::generate_texture_atlas(&texture_names, &device, &queue)
                .unwrap_or_else(|e| {
                    warn!("Failed to generate texture atlas: {}", e);
                    warn!("Using fallback atlas...");

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

        // Construire la map texture_name -> (texture_id, TextureUV) pour les modèles
        let mut texture_uv_map = std::collections::HashMap::new();
        for (idx, texture_name) in texture_names.iter().enumerate() {
            if let Some((u_min, v_min, u_max, v_max)) = texture_uvs.get(texture_name) {
                let uv = TextureUV::new(*u_min, *v_min, *u_max, *v_max, texture_size_in_atlas);
                texture_uv_map.insert(texture_name.clone(), (idx, uv));
            } else {
                warn!("Texture '{}' not found in atlas!", texture_name);
            }
        }
        info!("Registered {} textures in atlas", texture_uv_map.len());
        registry.register_texture_uvs(texture_uv_map);

        // Résoudre les modèles avec les IDs de texture
        registry.resolve_models();

        // Check which models were successfully resolved
        let voxel_count = registry.len();
        let mut resolved_count = 0;
        let mut unresolved_models = Vec::new();

        for id in 1..voxel_count {
            if let Some(def) = registry.get(id) {
                if let Some(model_name) = &def.model_name {
                    if let Some(model) = registry.get_resolved_model(model_name) {
                        resolved_count += 1;
                    } else {
                        unresolved_models.push((def.string_id.clone(), id, model_name.clone()));
                    }
                }
            }
        }

        if resolved_count > 0 {
            info!("Resolved {} block models successfully", resolved_count);
        }

        if !unresolved_models.is_empty() {
            warn!("Warning: {} blocks have unresolved models: {}",
                unresolved_models.len(),
                unresolved_models.iter()
                    .map(|(name, id, model)| format!("'{}' (ID {}) -> '{}'", name, id, model))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

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

        let depth_texture_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());

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

        // Créer le pipeline de bordure de chunk
        let (chunk_border_pipeline, chunk_border_camera_layout) =
            crate::renderer::pipeline::create_chunk_border_pipeline(&device, &surface_config);

        let chunk_border_camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Chunk Border Camera Bind Group"),
            layout: &chunk_border_camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        // Créer le buffer de vertex pour les lignes de chunk (initialement vide)
        // Mode FullGrid optimisé avec lignes continues: ~280 vertices per chunk × 125 chunks = ~35000
        const CHUNK_BORDER_INITIAL_VERTICES: usize = 50000;
        let chunk_border_vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Chunk Border Vertex Buffer"),
            size: (CHUNK_BORDER_INITIAL_VERTICES * std::mem::size_of::<ChunkBorderVertex>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let chunk_border_vertex_count = 0u32;

        let start_time = std::time::Instant::now();

        // Initialize egui state
        let egui_state = EguiState::new(window);

        // Register blocks from the loaded configs
        // Texture UVs are already registered in the registry from the model loading
        if block_configs.is_empty() {
            // Fallback: use default blocks
            warn!("Using fallback hardcoded blocks...");
            registry.register_voxel("grass", "Grass", 0);
            registry.register_voxel("dirt", "Dirt", 1);
            registry.register_voxel("bedrock", "Bedrock", 2);
            registry.register_voxel("stone", "Stone", 3);
        } else {
            // Register all blocks from the loaded configs
            let registered_count = block_configs.len();
            for (string_id, config) in &block_configs {
                registry.register_from_config(string_id, config);
            }
            info!("Registered {} block definitions", registry.len());
        }

        // Créer le monde voxel vide - PAS DE GÉNÉRATION SYNCHRONE
        // Le jeu démarre instantanément, les chunks apparaîtront progressivement
        let world = VoxelWorld::new(registry.clone());
        let world_arc = Arc::new(RwLock::new(world));

        // Créer le générateur de monde data-driven
        let mut worldgen = WorldGenerator::default();
        worldgen.init_block_ids(&registry);

        // NOUVEAU: Créer le tracker et les files d'attente
        let chunk_tracker: SharedChunkTracker = Arc::new(RwLock::new(crate::chunk_tracker_compat::ChunkTrackerCompat::new()));
        // TODO: Remove when server handles chunk generation completely
        // let chunk_gen_queue = ChunkGenerationQueue::new(world_arc.clone(), &worldgen, 4);
        let mesh_queue = MeshQueue::new(world_arc.clone(), 2, config.graphics.ao_intensity);

        info!("[WorldGen] Queue system ready - mesh (2 workers), chunk gen disabled (server only)");

        // Pas de meshs initiaux - ils seront générés progressivement
        let chunk_meshes = HashMap::new();
        let visible_chunks = Vec::new();

        // Initialize egui renderer
        let egui_renderer = egui_wgpu::Renderer::new(&device, surface_config.format, None, 1);

        // Créer le monde des entités ECS
        let mut entity_world = EntityWorld::new();
        // Spawner le joueur à une position proche du niveau du terrain (Y≈70)
        let spawn_pos = glam::Vec3::new(0.0, 60.0, 0.0);  // On terrain surface (terrain base is 50)
        entity_world.spawn_player(spawn_pos);
        info!("[Renderer] Player spawned at ({}, {}, {})", spawn_pos.x, spawn_pos.y, spawn_pos.z);

        Self {
            config: config.clone(),
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
            _chunk_border_pipeline: chunk_border_pipeline,
            chunk_border_camera_bind_group,
            chunk_border_vertex_buffer,
            chunk_border_vertex_count,
            chunk_border_mode: ChunkBorderMode::Disabled,
            queue,
            device,
            world: world_arc,
            chunk_meshes,
            visible_chunks,
            chunk_tracker,
            chunk_gen_queue: None,  // DISABLED: Server-only chunk generation
            mesh_queue: Some(mesh_queue),
            pending_mesh_requests: HashSet::new(),
            entity_world,
            highlight_vertex_buffer: None,
            highlight_face_buffer: None,
            highlight_target: None,
            start_time,
            last_status_log: std::time::Instant::now(),
            tick_accumulator: std::time::Duration::ZERO,
            last_tick_time: std::time::Instant::now(),
            camera: Camera::new(),
            egui_state,
            egui_renderer,
            submitted_command: None,
            chat_just_closed: false,
            egui_events: Vec::new(),
            egui_mouse_position: (0.0, 0.0),
            performance_collector: PerformanceCollector::new(),
        }
    }

    /// Définir un voxel et marquer les chunks affectés pour remeshing (priorité haute)
    /// Les chunks sont déterminés intelligemment: seuls ceux impactés par la modification sont remeshés
    pub fn set_voxel(&mut self, x: i32, y: i32, z: i32, global_id: Option<GlobalVoxelId>) {
        // Collecter les chunks à mesher (avec déduplication via HashSet)
        let mut chunks_to_mesh = std::collections::HashSet::new();

        if let Ok(mut world) = self.world.write() {
            let result = world.set_voxel(x, y, z, global_id);

            // Ajouter le chunk modifié
            let (cx, cy, cz) = result.modified_chunk;
            if cy >= 0 && cy < VERTICAL_CHUNKS as i32 {
                chunks_to_mesh.insert((cx, cy, cz));
            }

            // Ajouter les chunks voisins affectés (seulement ceux où le bloc est sur le bord)
            for (nx, ny, nz) in result.neighbor_chunks {
                if ny >= 0 && ny < VERTICAL_CHUNKS as i32 {
                    chunks_to_mesh.insert((nx, ny, nz));
                }
            }
        } // Le lock est relâché ici

        // Demander le mesh pour tous les chunks uniques affectés (priorité haute)
        // NOTE: On ne demande PAS les voisins supplémentaires car les voisins impactés
        // sont déjà inclus dans `result.neighbor_chunks`
        for (cx, cy, cz) in chunks_to_mesh {
            self.request_mesh_single(cx, cy, cz, MeshPriority::Modified);
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

    /// Génère les vertices pour les bordures de chunks autour du joueur
    /// Rayon: 5x5x5 chunks autour du chunk du joueur
    fn update_chunk_border_vertices(&mut self) {
        let mode = self.chunk_border_mode;
        if mode == ChunkBorderMode::Disabled {
            self.chunk_border_vertex_count = 0;
            return;
        }

        let player_pos = self.camera.position;
        let player_chunk_x = (player_pos.x / CHUNK_SIZE as f32).floor() as i32;
        let player_chunk_y = (player_pos.y / CHUNK_SIZE as f32).floor() as i32;
        let player_chunk_z = (player_pos.z / CHUNK_SIZE as f32).floor() as i32;

        const RADIUS: i32 = 2; // 5x5x5 = -2 to +2

        let mut vertices = Vec::new();

        // Couleurs style Minecraft
        // Noir pour les bordures de chunks (plus visibles)
        const CHUNK_BORDER_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 0.8];  // Noir avec alpha 0.8
        // Blanc pour la grille interne (très visible)
        const GRID_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.8];  // Blanc avec alpha 0.8

        let full_grid = mode == ChunkBorderMode::FullGrid;

        // Pour chaque chunk dans le rayon
        for dx in -RADIUS..=RADIUS {
            for dy in -RADIUS..=RADIUS {
                for dz in -RADIUS..=RADIUS {
                    let cx = player_chunk_x + dx;
                    let cy = player_chunk_y + dy;
                    let cz = player_chunk_z + dz;

                    // Skip si hors limites verticales
                    if cy < 0 || cy >= VERTICAL_CHUNKS as i32 {
                        continue;
                    }

                    // Position monde du chunk
                    let world_x = cx * CHUNK_SIZE as i32;
                    let world_y = cy * CHUNK_SIZE as i32;
                    let world_z = cz * CHUNK_SIZE as i32;

                    let chunk_size_f = CHUNK_SIZE as f32;
                    let x0 = world_x as f32;
                    let y0 = world_y as f32;
                    let z0 = world_z as f32;
                    let x1 = x0 + chunk_size_f;
                    let y1 = y0 + chunk_size_f;
                    let z1 = z0 + chunk_size_f;

                    // Helper pour ajouter une ligne
                    let mut add_line = |x1: f32, y1: f32, z1: f32, x2: f32, y2: f32, z2: f32, color: [f32; 4]| {
                        vertices.push(ChunkBorderVertex { position: [x1, y1, z1], color });
                        vertices.push(ChunkBorderVertex { position: [x2, y2, z2], color });
                    };

                    // Bordures du chunk (arêtes extérieures) - toujours affichées
                    // Arêtes verticales (4 coins)
                    add_line(x0, y0, z0, x0, y1, z0, CHUNK_BORDER_COLOR);
                    add_line(x1, y0, z0, x1, y1, z0, CHUNK_BORDER_COLOR);
                    add_line(x0, y0, z1, x0, y1, z1, CHUNK_BORDER_COLOR);
                    add_line(x1, y0, z1, x1, y1, z1, CHUNK_BORDER_COLOR);

                    // Arêtes horizontales en bas
                    add_line(x0, y0, z0, x1, y0, z0, CHUNK_BORDER_COLOR);
                    add_line(x1, y0, z0, x1, y0, z1, CHUNK_BORDER_COLOR);
                    add_line(x1, y0, z1, x0, y0, z1, CHUNK_BORDER_COLOR);
                    add_line(x0, y0, z1, x0, y0, z0, CHUNK_BORDER_COLOR);

                    // Arêtes horizontales en haut
                    add_line(x0, y1, z0, x1, y1, z0, CHUNK_BORDER_COLOR);
                    add_line(x1, y1, z0, x1, y1, z1, CHUNK_BORDER_COLOR);
                    add_line(x1, y1, z1, x0, y1, z1, CHUNK_BORDER_COLOR);
                    add_line(x0, y1, z1, x0, y1, z0, CHUNK_BORDER_COLOR);

                    // Grille interne (lignes tous les blocs) - seulement sur le chunk actuel du joueur
                    if full_grid && dx == 0 && dy == 0 && dz == 0 {
                        // Lignes verticales internes sur les 4 faces latérales
                        for ix in 1..16 {
                            let x = x0 + ix as f32;
                            add_line(x, y0, z0, x, y1, z0, GRID_COLOR);  // Face avant
                            add_line(x, y0, z1, x, y1, z1, GRID_COLOR);  // Face arrière
                        }
                        for iz in 1..16 {
                            let z = z0 + iz as f32;
                            add_line(x0, y0, z, x0, y1, z, GRID_COLOR);  // Face gauche
                            add_line(x1, y0, z, x1, y1, z, GRID_COLOR);  // Face droite
                        }

                        // Lignes horizontales continues sur les 4 faces latérales (pour chaque niveau Y)
                        for iy in 1..16 {
                            let y = y0 + iy as f32;
                            add_line(x0, y, z0, x1, y, z0, GRID_COLOR);  // Face avant
                            add_line(x0, y, z1, x1, y, z1, GRID_COLOR);  // Face arrière
                            add_line(x0, y, z0, x0, y, z1, GRID_COLOR);  // Face gauche
                            add_line(x1, y, z0, x1, y, z1, GRID_COLOR);  // Face droite
                        }

                        // Grille au sol (Y = 0) - lignes horizontales
                        for ix in 1..16 {
                            let x = x0 + ix as f32;
                            add_line(x, y0, z0, x, y0, z1, GRID_COLOR);  // Lignes Z
                        }
                        for iz in 1..16 {
                            let z = z0 + iz as f32;
                            add_line(x0, y0, z, x1, y0, z, GRID_COLOR);  // Lignes X
                        }

                        // Grille au plafond (Y = 16) - lignes horizontales
                        for ix in 1..16 {
                            let x = x0 + ix as f32;
                            add_line(x, y1, z0, x, y1, z1, GRID_COLOR);  // Lignes Z
                        }
                        for iz in 1..16 {
                            let z = z0 + iz as f32;
                            add_line(x0, y1, z, x1, y1, z, GRID_COLOR);  // Lignes X
                        }
                    }
                }
            }
        }

        self.chunk_border_vertex_count = vertices.len() as u32;

        // Upload to GPU
        self.queue.write_buffer(&self.chunk_border_vertex_buffer, 0, bytemuck::cast_slice(&vertices));
    }

    /// Traiter les mises à jour de chunks et meshs (appelé chaque frame)
    pub fn process_updates(&mut self) {
        // Accumuler le temps écoulé
        let now = std::time::Instant::now();
        let delta = now.duration_since(self.last_tick_time);
        self.last_tick_time = now;
        self.tick_accumulator = self.tick_accumulator.saturating_add(delta);

        // Const TICK_DURATION: 100ms = 10 ticks/seconde
        const TICK_DURATION: std::time::Duration = std::time::Duration::from_millis(100);
        // Max 3 ticks par frame pour éviter spiral of death (si lag)
        const MAX_TICKS_PER_FRAME: u32 = 3;

        let mut ticks_processed = 0;
        while self.tick_accumulator >= TICK_DURATION && ticks_processed < MAX_TICKS_PER_FRAME {
            self.tick_accumulator = self.tick_accumulator.saturating_sub(TICK_DURATION);
            self.process_tick();
            ticks_processed += 1;
        }

        // Opérations per-frame (pas limitées par les ticks)
        // 1. Traiter les chunks générés (arrivant de la file) - DISABLED: Server-only
        // self.process_generated_chunks();

        // 2. Mettre à jour le highlight (per-frame pour responsiveness)
        self.update_highlight_mesh();

        // 3. Log de statut (toutes les secondes, pas lié aux ticks)
        const LOG_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
        if self.last_status_log.elapsed() >= LOG_INTERVAL {
            self.last_status_log = std::time::Instant::now();

            let (pending_gen, generating, pending_mesh) = self.chunk_tracker.read().unwrap().get_stats();
            let generated_count = if let Ok(world) = self.world.read() {
                world.chunk_count()
            } else {
                0
            };
            let meshed_count = self.chunk_meshes.len();

            // Calculer combien de chunks devraient exister (autour du joueur)
            let load_radius = 8;
            let vertical_radius = 4;
            let horizontal_size = load_radius * 2 + 1;  // 9
            let expected_count = horizontal_size * horizontal_size * (vertical_radius * 2 + 1);  // 9*9*5 = 405

            debug!("[Chunks] expected: {} | generated: {} | meshed: {} | pending_gen: {} | generating: {} | pending_mesh: {}",
                expected_count, generated_count, meshed_count, pending_gen, generating, pending_mesh);
        }

        // 4. Flush les requêtes de mesh en attente (batching pour éviter les duplicats)
        // IMPORTANT: Was after process_generated_chunks() - now chunks come from server
        self.flush_pending_mesh_requests();
    }

    /// Traitement d'un tick (10 fois par seconde, frame-independent)
    fn process_tick(&mut self) {
        // Compteur de ticks pour les opérations périodiques
        // On utilise un compteur statique local à la fonction
        use std::sync::atomic::{AtomicU32, Ordering};
        static TICK_COUNTER: AtomicU32 = AtomicU32::new(0);

        let tick = TICK_COUNTER.fetch_add(1, Ordering::Relaxed);

        // 1. Tous les ticks: demander des chunks (max 4 par tick)
        self.request_chunks_tick(4);

        // 2. Traiter les meshs terminés (max 20 par tick = 200/seconde)
        self.process_meshes_tick(20);  // 20 meshes per tick = 200/second

        // 3. Cleanup désactivé temporairement (causait des race conditions)
        // Les chunks dans pending_generation seront nettoyés quand leurs signaux already_exists arrivent
        // if tick % 50 == 0 {
        //     self.cleanup_orphaned_states();
        // }

        // 4. Cleanup out-of-bounds désactivé temporairement (causait des race conditions)
        // if tick % 10 == 0 {
        //     self.chunk_tracker.cleanup_out_of_bounds(VERTICAL_CHUNKS as i32);
        // }
    }

    /// Traiter les meshs terminés (tick-based, max `max_count` par tick)
    fn process_meshes_tick(&mut self, max_count: usize) {
        let Some(ref mesh_queue) = self.mesh_queue else {
            return;
        };

        let mut processed = 0;
        while processed < max_count {
            if let Some(result) = mesh_queue.try_recv() {
                let pos = (result.cx, result.cy, result.cz);

                // Only log empty chunks (potential problems)
                if result.vertices.is_empty() {
                    debug!("[Renderer] Built mesh for chunk ({},{},{}) - EMPTY", result.cx, result.cy, result.cz);
                }

                if result.vertices.is_empty() {
                    // Mesh vide = chunk vide, retirer du cache
                    self.chunk_meshes.remove(&pos);
                    self.chunk_tracker.read().unwrap().mark_meshed(pos);
                } else if result.vertices.len() < 200000 {
                    // Créer le buffer GPU (remplace l'ancien mesh si existant)
                    let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some(&format!("Chunk Mesh ({}, {}, {})", result.cx, result.cy, result.cz)),
                        contents: bytemuck::cast_slice(&result.vertices),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    });

                    // NOTE: Le mesh est remplacé SEULEMENT maintenant, pas avant
                    // Donc l'ancien mesh reste visible jusqu'à ce que le nouveau soit prêt
                    self.chunk_meshes.insert(pos, ChunkMesh {
                        vertex_buffer,
                        vertex_count: result.vertices.len() as u32,
                    });
                    self.chunk_tracker.read().unwrap().mark_meshed(pos);
                } else {
                    // Mesh trop grand, juste marquer comme meshé
                    self.chunk_tracker.read().unwrap().mark_meshed(pos);
                }
                processed += 1;
            } else {
                break; // Plus de meshs à traiter
            }
        }
    }

    /// Version legacy pour compatibilité
    pub fn process_mesh_updates(&mut self) {
        self.process_updates();
    }

    /// Demander un rebuild immédiat pour un chunk spécifique
    pub fn request_immediate_rebuild(&mut self, cx: i32, cy: i32, cz: i32) {
        self.request_mesh_single(cx, cy, cz, MeshPriority::Modified);
    }

    /// Log chunk statistics (for debugging)
    pub fn log_chunk_stats(&self) {
        if let Ok(world) = self.world.read() {
            let chunk_count = world.chunk_count();
            info!("[Renderer] Total chunks in world: {}", chunk_count);
        }
    }

    /// Checks if a chunk has a mesh
    pub fn has_chunk_mesh(&self, cx: i32, cy: i32, cz: i32) -> bool {
        self.chunk_meshes.contains_key(&(cx, cy, cz))
    }

    /// Traiter les chunks qui viennent d'être générés
    /// DISABLED: Server-only chunk generation
    fn process_generated_chunks(&mut self) {
        // DISABLED: Chunks are now received from server, not generated locally
        return;

        // Collecter d'abord tous les résultats
        let mut results = Vec::new();
        if let Some(ref gen_queue) = self.chunk_gen_queue {
            const MAX_PER_FRAME: usize = 5;
            for _ in 0..MAX_PER_FRAME {
                if let Some(result) = gen_queue.try_recv() {
                    results.push(result);
                } else {
                    break;
                }
            }
        }

        // Traiter les résultats (sans lock sur gen_queue)
        for result in results {
            let pos = (result.cx, result.cy, result.cz);

            // Cas 1: Le chunk existait déjà (signal du worker - race condition)
            // On le marque juste comme généré pour retirer de pending_generation
            if result.already_exists {
                self.chunk_tracker.read().unwrap().mark_generated(pos);
                // IMPORTANT: NE PAS demander les voisins ici !
                // Le chunk existe mais n'a peut-être pas de mesh, donc on demande juste son mesh
                if !self.chunk_tracker.read().unwrap().is_meshing_or_pending(&pos) {
                    self.request_mesh_single(result.cx, result.cy, result.cz, MeshPriority::New);
                }
                continue;
            }

            // Cas 2: Nouvelle génération de chunk
            let Some(voxels) = result.voxels else {
                continue; // Ne devrait pas arriver si already_exists == false
            };
            let Some(palette) = result.palette else {
                continue;
            };

            // Vérifier que le chunk n'existe pas déjà (race condition check)
            let already_exists_in_world = if let Ok(world) = self.world.read() {
                world.get_chunk_existing(result.cx, result.cy, result.cz).is_some()
            } else {
                false
            };

            if already_exists_in_world {
                // Le chunk a été inséré entre temps, marquer comme généré
                self.chunk_tracker.read().unwrap().mark_generated(pos);
                continue;
            }

            // Insérer le chunk dans le monde
            let chunk = VoxelChunk::from_data(voxels, palette);
            if let Ok(mut world) = self.world.write() {
                world.insert_chunk(result.cx, result.cy, result.cz, chunk);
            }

            // Marquer comme généré
            self.chunk_tracker.read().unwrap().mark_generated(pos);

            // Demander le mesh pour le chunk et ses voisins
            self.request_mesh_for_chunk_and_neighbors(result.cx, result.cy, result.cz);
        }
    }

    /// Demande le mesh pour un chunk et ses voisins
    /// Utilisé quand un chunk est nouvellement généré:
    /// - Le chunk lui-même et ses voisins sont ajoutés au pending set (batching)
    /// - Les voisins qui existent déjà sont marqués pour remesh (pour mettre à jour les faces de bord)
    /// NOTE: Les requêtes ne sont PAS envoyées immédiatement - elles sont batchées
    /// et envoyées à la fin du tick via flush_pending_mesh_requests()
    fn request_mesh_for_chunk_and_neighbors(&mut self, cx: i32, cy: i32, cz: i32) {
        // Ajouter le chunk lui-même au pending set
        self.pending_mesh_requests.insert((cx, cy, cz));

        // Vérifier une fois quels chunks existent pour les voisins
        let existing_neighbors: Vec<(i32, i32, i32)> = if let Ok(world) = self.world.read() {
            let potential_neighbors = [
                (cx + 1, cy, cz), (cx - 1, cy, cz),
                (cx, cy + 1, cz), (cx, cy - 1, cz),
                (cx, cy, cz + 1), (cx, cy, cz - 1),
            ];

            potential_neighbors.iter()
                .filter(|(nx, ny, nz)| {
                    *ny >= 0 && *ny < VERTICAL_CHUNKS as i32 &&
                    world.get_chunk_existing(*nx, *ny, *nz).is_some()
                })
                .copied()
                .collect()
        } else {
            Vec::new()
        };

        // Ajouter les voisins existants au pending set
        // Ils seront meshés une seule fois à la fin du tick, même si demandés plusieurs fois
        for (nx, ny, nz) in existing_neighbors {
            self.pending_mesh_requests.insert((nx, ny, nz));
        }
    }

    /// Envoie toutes les requêtes de mesh en attente au mesh queue
    /// Appelé à la fin de chaque tick pour batcher les requêtes et éviter les duplicats
    fn flush_pending_mesh_requests(&mut self) {
        if self.pending_mesh_requests.is_empty() {
            return;
        }

        let Some(ref mesh_queue) = self.mesh_queue else {
            return;
        };

        // Prendre toutes les positions en attente
        let positions: Vec<ChunkPos> = self.pending_mesh_requests.drain().collect();

        // Vérifier une fois quels chunks existent (avant de boucler)
        let existing_chunks: Vec<ChunkPos> = if let Ok(world) = self.world.read() {
            positions.iter()
                .filter(|(cx, cy, cz)| {
                    *cy >= 0 && *cy < VERTICAL_CHUNKS as i32 &&
                    world.get_chunk_existing(*cx, *cy, *cz).is_some()
                })
                .copied()
                .collect()
        } else {
            Vec::new()
        };

        // Envoyer les requêtes de mesh pour tous les chunks existants
        for (cx, cy, cz) in existing_chunks {
            let pos = (cx, cy, cz);

            // Vérifier si déjà en cours de meshing
            if !self.chunk_tracker.read().unwrap().is_meshing_or_pending(&pos) {
                self.chunk_tracker.read().unwrap().mark_pending_mesh_direct(pos);
                mesh_queue.request_mesh(cx, cy, cz, MeshPriority::New);
            }
        }
    }

    /// Demande le mesh pour un seul chunk (sans duplication)
    fn request_mesh_single(&mut self, cx: i32, cy: i32, cz: i32, priority: MeshPriority) {
        // Vérifier que le chunk existe
        let exists = if let Ok(world) = self.world.read() {
            world.get_chunk_existing(cx, cy, cz).is_some()
        } else {
            false
        };

        if !exists {
            debug!("[Mesh] Chunk ({},{},{}) doesn't exist yet, skipping mesh request", cx, cy, cz);
            return; // Le chunk n'existe pas encore, on ne demande pas le mesh
        }

        debug!("[Mesh] Requesting mesh for chunk ({},{},{})", cx, cy, cz);

        let pos = (cx, cy, cz);

        // Pour les requêtes de haute priorité (bloc modifié), on force le remesh
        let is_high_priority = priority == MeshPriority::Modified;
        if is_high_priority {
            // Nettoyer tous les états de meshing pour forcer un remesh complet
            self.chunk_tracker.read().unwrap().clear_mesh_state(pos);
        }

        // Vérifier si déjà en cours de meshing
        let already_meshing = self.chunk_tracker.read().unwrap().is_meshing_or_pending(&pos);

        if !already_meshing {
            self.chunk_tracker.read().unwrap().mark_pending_mesh_direct(pos);

            if let Some(ref mesh_queue) = self.mesh_queue {
                mesh_queue.request_mesh(cx, cy, cz, priority);
            }
        }
    }

    /// Nettoyer les états orphelins (chunks générés mais encore marqués pending)
    /// Appelé depuis le tick système (tous les 50 ticks = 5 secondes)
    fn cleanup_orphaned_states(&mut self) {
        // Nettoyer pending_generation en vérifiant directement dans le monde
        let world = self.world.clone();
        self.chunk_tracker.read().unwrap().cleanup_pending_generation_verify(|pos| {
            if let Ok(w) = world.read() {
                w.get_chunk_existing(pos.0, pos.1, pos.2).is_some()
            } else {
                false
            }
        });
    }

    /// Demander la génération des chunks autour du joueur (tick-based)
    /// Appelé 10 fois par seconde, max `max_count` chunks par tick
    /// Priorité par distance au joueur (plus proches d'abord)
    fn request_chunks_tick(&mut self, max_count: usize) {
        let Some(ref gen_queue) = self.chunk_gen_queue else {
            return;
        };

        let player_pos = self.camera.position;
        let player_chunk_x = (player_pos.x / CHUNK_SIZE as f32).floor() as i32;
        let player_chunk_y = (player_pos.y / CHUNK_SIZE as f32).floor() as i32;
        let player_chunk_z = (player_pos.z / CHUNK_SIZE as f32).floor() as i32;

        // Rayon de chargement (en chunks)
        const LOAD_RADIUS: i32 = 4;
        const VERTICAL_RADIUS: i32 = 2;

        // Collecter tous les chunks à vérifier avec leur distance au joueur
        let mut chunks_to_request: Vec<(i32, i32, i32, f32)> = Vec::new();

        for dx in -LOAD_RADIUS..=LOAD_RADIUS {
            for dz in -LOAD_RADIUS..=LOAD_RADIUS {
                for dy in -VERTICAL_RADIUS..=VERTICAL_RADIUS {
                    let cx = player_chunk_x + dx;
                    let cy = player_chunk_y + dy;
                    let cz = player_chunk_z + dz;
                    let pos = (cx, cy, cz);

                    // Skip si hors des limites du monde (vertical)
                    if cy < 0 || cy >= VERTICAL_CHUNKS as i32 {
                        continue;
                    }

                    // Skip si déjà en cours de génération ou généré
                    if self.chunk_tracker.read().unwrap().is_generating(&pos) {
                        continue;
                    }

                    // Skip si déjà généré
                    if self.chunk_tracker.read().unwrap().is_generated(&pos) {
                        continue;
                    }

                    // Calculer la distance au joueur (world space)
                    let dx_world = (cx * CHUNK_SIZE as i32) as f32 - player_pos.x;
                    let dy_world = (cy * CHUNK_SIZE as i32) as f32 - player_pos.y;
                    let dz_world = (cz * CHUNK_SIZE as i32) as f32 - player_pos.z;
                    let dist_sq = dx_world * dx_world + dy_world * dy_world + dz_world * dz_world;

                    chunks_to_request.push((cx, cy, cz, dist_sq));
                }
            }
        }

        // Trier par distance (plus proches d'abord = priorité haute)
        chunks_to_request.sort_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal));

        // Demander seulement max_count chunks ce tick
        for (cx, cy, cz, _) in chunks_to_request.into_iter().take(max_count) {
            let pos = (cx, cy, cz);
            // Double-check (un autre tick aurait pu avoir généré le chunk entre-temps)
            if self.chunk_tracker.read().unwrap().is_generated(&pos) || self.chunk_tracker.read().unwrap().is_generating(&pos) {
                continue;
            }
            if self.chunk_tracker.read().unwrap().mark_pending_generation(pos) {
                gen_queue.request_chunk(cx, cy, cz, 0);
            }
        }
    }

    /// Version legacy pour compatibilité (plus utilisée, remplacée par request_chunks_tick)
    #[allow(dead_code)]
    fn request_chunks_around_player(&mut self) {
        // Cette fonction n'est plus appelée, remplacée par request_chunks_tick
        // mais la garde pour compatibilité au cas où
    }

    pub fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.surface_config.width = new_size.width;
            self.surface_config.height = new_size.height;
            self._surface.configure(&self.device, &self.surface_config);

            // Update egui pixels_per_point
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

            self.depth_texture_view = self.depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
        }
    }

    /// Met à jour la liste des chunks visibles avec frustum culling
    fn update_visible_chunks(&mut self, aspect_ratio: f32) {
        use crate::renderer::frustum::Frustum;

        let view_proj = self.camera.view_projection(aspect_ratio);
        let frustum = Frustum::from_view_proj(view_proj);

        self.visible_chunks.clear();
        self.visible_chunks.reserve(self.chunk_meshes.len());

        for (&pos, _) in &self.chunk_meshes {
            if frustum.is_chunk_visible(pos.0, pos.1, pos.2) {
                self.visible_chunks.push(pos);
            }
        }
    }

    pub fn render(&mut self, selected_block: (u32, String)) -> Result<(), wgpu::SurfaceError> {
        let render_start = std::time::Instant::now();
        self.egui_state.update_fps();

        // Log chunk stats periodically (every ~5 seconds at 60fps)
        // Note: Using atomic counter instead of static mut for safety
        use std::sync::atomic::{AtomicU32, Ordering};
        static LOG_COUNTER: AtomicU32 = AtomicU32::new(0);
        let count = LOG_COUNTER.fetch_add(1, Ordering::Relaxed);
        if count % 300 == 0 {
            self.log_chunk_stats();
        }

        // Mettre à jour les bordures de chunks avant le rendu (seulement si pas désactivé)
        if self.chunk_border_mode != ChunkBorderMode::Disabled {
            self.update_chunk_border_vertices();
        }

        let aspect_ratio = self.surface_config.width as f32 / self.surface_config.height as f32;

        // Frustum culling : mettre à jour les chunks visibles
        let render_prep_start = std::time::Instant::now();
        self.update_visible_chunks(aspect_ratio);
        let render_prep_duration = render_prep_start.elapsed();
        self.performance_collector_mut().record_render_prep_time(render_prep_duration);

        let view_proj = self.camera.view_projection(aspect_ratio);
        let view_proj_array: [[f32; 4]; 4] = view_proj.to_cols_array_2d();
        self.queue.write_buffer(&self.camera_buffer, 0, bytemuck::cast_slice(&[view_proj_array]));

        let elapsed = self.start_time.elapsed().as_secs_f32();
        self.queue.write_buffer(&self.highlight_time_buffer, 0, bytemuck::cast_slice(&[elapsed]));

        let output = self._surface.get_current_texture()?;
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        // Show settings menu before world borrow
        self.show_settings_menu_if_open();

        // Calculate chunk coordinates, block count, camera forward, and target block (needs world borrow)
        let (pos, chunk_x, chunk_y, chunk_z, yaw, pitch, block_count, camera_forward, target_block) = {
            let world_ref = if let Ok(w) = self.world.read() { w } else { return Ok(()) };

            let pos = self.camera.position;
            let forward = self.camera.forward();
            let chunk_x = pos.x.floor() as i32 / 16;
            let chunk_y = pos.y.floor() as i32 / 16;
            let chunk_z = pos.z.floor() as i32 / 16;

            // Calculate yaw and pitch (convert radians to degrees)
            let yaw = self.camera.yaw * 180.0 / std::f32::consts::PI;
            let pitch = self.camera.pitch * 180.0 / std::f32::consts::PI;

            let block_count = world_ref.registry().len();

            // Get target block info with block name
            let target_block = self.highlight_target.as_ref().and_then(|target| {
                // Get the block at the target position
                world_ref.get_voxel_opt(target.x, target.y, target.z)
                    .and_then(|id| {
                        world_ref.registry().get(id)
                            .map(|def| def.name.clone())
                            .map(|name| (target.x, target.y, target.z, target.face, name))
                    })
            });

            (pos, chunk_x, chunk_y, chunk_z, yaw, pitch, block_count, (forward.x, forward.y, forward.z), target_block)
        }; // world_ref is dropped here

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

        // Start UI timing
        let ui_start = std::time::Instant::now();
        self.egui_state.context_mut().begin_frame(raw_input);
        let perf_snapshot = self.performance_collector.snapshot();

        self.egui_state.show_debug_ui(
            (pos.x, pos.y, pos.z),
            camera_forward,
            yaw,
            pitch,
            (chunk_x, chunk_y, chunk_z),
            target_block,
            selected_block,
            block_count,
            &perf_snapshot,
            self.visible_chunks.len(),
            self.chunk_meshes.len(),
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

        // Record UI timing
        let ui_duration = ui_start.elapsed();
        self.performance_collector_mut().record_ui_time(ui_duration);

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

            // Rendu des chunks visibles (frustum culling)
            for chunk_pos in &self.visible_chunks {
                if let Some(mesh) = self.chunk_meshes.get(chunk_pos) {
                    render_pass.set_vertex_buffer(0, mesh.vertex_buffer.slice(..));
                    render_pass.draw(0..mesh.vertex_count, 0..1);
                }
            }

            if let Some(ref face_buffer) = self.highlight_face_buffer {
                render_pass.set_pipeline(&self._highlight_pipeline);
                render_pass.set_bind_group(0, &self.camera_bind_group, &[]);
                render_pass.set_bind_group(1, &self.highlight_time_bind_group, &[]);
                render_pass.set_vertex_buffer(0, face_buffer.slice(..));
                render_pass.draw(0..6, 0..1);
            }

            // Rendu des bordures de chunks (seulement si pas désactivé)
            if self.chunk_border_mode != ChunkBorderMode::Disabled && self.chunk_border_vertex_count > 0 {
                render_pass.set_pipeline(&self._chunk_border_pipeline);
                render_pass.set_bind_group(0, &self.chunk_border_camera_bind_group, &[]);
                render_pass.set_vertex_buffer(0, self.chunk_border_vertex_buffer.slice(..));
                render_pass.draw(0..self.chunk_border_vertex_count, 0..1);
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

        // Measure GPU submission time
        let gpu_start = std::time::Instant::now();
        self.queue.submit(Some(encoder.finish()));
        let gpu_duration = gpu_start.elapsed();

        output.present();

        // Record timing
        let render_duration = render_start.elapsed();
        self.performance_collector.record_gpu_time(gpu_duration);

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

    /// Toggle l'affichage des bordures de chunks (F6)
    /// Cycle entre: Disabled -> ChunkBorders -> FullGrid -> Disabled
    pub fn toggle_chunk_borders(&mut self) {
        self.chunk_border_mode = self.chunk_border_mode.next();
        debug!("[Debug] Chunk borders: {}", self.chunk_border_mode.as_str());
    }

    /// Obtenir une référence au monde des entités ECS
    pub fn entity_world(&self) -> &EntityWorld {
        &self.entity_world
    }

    /// Obtenir une référence mutable au monde des entités ECS
    pub fn entity_world_mut(&mut self) -> &mut EntityWorld {
        &mut self.entity_world
    }

    /// Met à jour la caméra depuis l'entité joueur
    /// Retourne true si la mise à jour a réussi
    pub fn update_camera_from_player(&mut self) -> bool {
        let player_entity = match self.entity_world.player_entity() {
            Some(e) => e,
            None => return false,
        };

        use crate::client_systems::camera_sync_system;

        match camera_sync_system(self.entity_world.world_read(), player_entity) {
            Some((pos, yaw, pitch)) => {
                self.camera.position = Vec3A::new(pos.x, pos.y, pos.z);
                self.camera.yaw = yaw;
                self.camera.pitch = pitch;
                true
            }
            None => false,
        }
    }

    /// Téléporte le joueur à une position absolue
    pub fn teleport_player(&mut self, pos: glam::Vec3) {
        use voxl_common::entities::Position;

        if let Some(player_entity) = self.entity_world.player_entity() {
            let Ok(position) = self.entity_world.world().query_one_mut::<&mut Position>(player_entity) else {
                return;
            };
            position.set(pos);
            // Mettre aussi à jour la caméra directement
            self.update_camera_from_player();
        }
    }

    /// Téléporte le joueur relativement à sa position actuelle
    pub fn teleport_player_relative(&mut self, delta: glam::Vec3) {
        use voxl_common::entities::Position;

        if let Some(player_entity) = self.entity_world.player_entity() {
            let Ok(position) = self.entity_world.world().query_one_mut::<&mut Position>(player_entity) else {
                return;
            };
            let old_pos = position.as_vec3();
            position.set(old_pos + delta);
            // Mettre aussi à jour la caméra directement
            self.update_camera_from_player();
        }
    }

    /// Change le mode de jeu du joueur
    pub fn set_game_mode(&mut self, mode: GameMode) -> bool {
        self.entity_world.set_game_mode(mode)
    }

    /// Toggle le fly mode du joueur
    pub fn toggle_fly(&mut self) -> bool {
        self.entity_world.toggle_fly()
    }

    /// Retourne le mode de jeu actuel
    pub fn get_game_mode(&self) -> Option<GameMode> {
        self.entity_world.get_game_mode()
    }

    /// Toggle debug UI
    pub fn toggle_debug_ui(&mut self) {
        self.egui_state.enabled = !self.egui_state.enabled;
    }

    /// Request stats dump to file (triggered by F7)
    pub fn request_dump_stats(&mut self) {
        self.egui_state.request_dump_stats();
    }

    /// Check if stats dump was requested and perform it
    pub fn check_dump_stats(&mut self) {
        // Get current state info needed for stats dump
        let (camera_pos, camera_yaw, camera_pitch) = {
            let cam = &self.camera;
            ((cam.position.x, cam.position.y, cam.position.z), cam.yaw, cam.pitch)
        };

        let (chunk_coords, selected_block, block_count, visible_chunks, total_chunks) = {
            let pos = self.camera.position;
            let chunk_x = pos.x.floor() as i32 / 16;
            let chunk_y = pos.y.floor() as i32 / 16;
            let chunk_z = pos.z.floor() as i32 / 16;

            let block_count = if let Ok(world) = self.world.read() {
                world.registry().len()
            } else {
                0
            };

            let visible = self.visible_chunks.len();
            let total = self.chunk_meshes.len();

            ((chunk_x, chunk_y, chunk_z), (0, "None".to_string()), block_count, visible, total)
        };

        let perf = self.performance_collector.snapshot();

        self.egui_state.check_dump_stats(
            camera_pos, camera_yaw, camera_pitch,
            chunk_coords, selected_block, block_count,
            &perf, visible_chunks, total_chunks,
        );
    }

    /// Get a reference to the performance collector
    pub fn performance_collector(&self) -> &PerformanceCollector {
        &self.performance_collector
    }

    /// Get a mutable reference to the performance collector
    pub fn performance_collector_mut(&mut self) -> &mut PerformanceCollector {
        &mut self.performance_collector
    }

    /// Get the number of loaded meshes
    pub fn loaded_meshes_count(&self) -> usize {
        self.chunk_meshes.len()
    }

    /// Get the number of pending mesh requests
    pub fn pending_mesh_requests_count(&self) -> usize {
        self.pending_mesh_requests.len()
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

    /// Get egui state (for settings menu access)
    pub fn egui_state_mut(&mut self) -> &mut crate::debug::EguiState {
        &mut self.egui_state
    }

    /// Check if settings menu is open
    pub fn is_settings_open(&self) -> bool {
        self.egui_state.settings_open
    }

    /// Get the current configuration
    pub fn config(&self) -> &GameConfig {
        &self.config
    }

    /// Update the configuration (e.g., after changes in settings menu)
    pub fn update_config(&mut self, config: GameConfig) {
        let needs_reconfigure = self.config.graphics.vsync != config.graphics.vsync;
        let render_distance_changed = self.config.graphics.render_distance != config.graphics.render_distance;

        self.config = config;

        // Reconfigure surface if vsync changed
        if needs_reconfigure {
            // Use Mailbox for non-vsync as it's widely supported (including Wayland)
            // Immediate is not available on some platforms (like Wayland)
            let present_mode = if self.config.graphics.vsync {
                wgpu::PresentMode::Fifo
            } else {
                wgpu::PresentMode::Mailbox
            };
            self.surface_config.present_mode = present_mode;
            self._surface.configure(&self.device, &self.surface_config);
            info!("Surface reconfigured with vsync={}", self.config.graphics.vsync);
        }

        // Update render distance in chunk tracker if changed
        // TODO: Implement render distance update in chunk tracker
        // if render_distance_changed {
        //     self.chunk_tracker.set_render_distance(self.config.graphics.render_distance);
        // }
    }

    /// Get the AO intensity for meshing
    pub fn ao_intensity(&self) -> f32 {
        self.config.graphics.ao_intensity
    }

    /// Show settings menu if it's open
    fn show_settings_menu_if_open(&mut self) {
        if self.egui_state.settings_open {
            crate::ui::settings_menu(
                &self.egui_state.ctx,
                &mut self.config,
                &mut self.egui_state.settings_open,
            );
        }
    }

    /// Check if FPS limiting is enabled
    pub fn should_limit_fps(&self) -> bool {
        self.config.graphics.effective_max_fps().is_some()
    }

    /// Get the minimum frame time for FPS limiting
    pub fn min_frame_time(&self) -> Option<std::time::Duration> {
        self.config.graphics.effective_max_fps().map(|fps| {
            std::time::Duration::from_secs_f64(1.0 / fps as f64)
        })
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
