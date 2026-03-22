use voxl_common::voxel::{VoxelChunk, CHUNK_SIZE, VoxelWorld, GlobalVoxelId, SharedVoxelRegistry, RenderType};
use voxl_common::voxel::face::TriangleDiagonal;
use super::{VoxelFace, AoCalculator};
use glam::{IVec3, Vec3};
use glam::FloatExt;
use voxl_common::voxel::model::{ResolvedBlockModel, ResolvedModelElement, ResolvedElementFace, ElementBounds};

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub voxel_pos: [i32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

const LIGHT_DIR: Vec3 = Vec3::new(0.4472136, 0.8944272, 0.0);
const AMBIENT_STRENGTH: f32 = 0.3;
const DIFFUSE_STRENGTH: f32 = 0.7;

/// Lightweight voxel reference for rendering calculations
#[derive(Clone, Copy, Debug)]
pub struct VoxelRef {
    /// Global voxel ID (0 = air)
    pub id: GlobalVoxelId,
    /// Block render type
    pub render_type: RenderType,
    /// Is the block solid (collision)
    pub collidable: bool,
}

impl VoxelRef {
    /// Creates an empty reference (air)
    pub const AIR: Self = Self {
        id: 0,
        render_type: RenderType::Invisible,
        collidable: false,
    };

    /// Creates a reference from ID and registry
    pub fn from_id(id: GlobalVoxelId, registry: &SharedVoxelRegistry) -> Self {
        let render_type = registry.get_render_type(id);
        let collidable = registry.is_collidable(id);
        Self { id, render_type, collidable }
    }

    /// Creates a reference from ID (with default values if registry not available)
    pub const fn from_id_unchecked(id: GlobalVoxelId) -> Self {
        Self {
            id,
            render_type: if id == 0 { RenderType::Invisible } else { RenderType::Opaque },
            collidable: id != 0,
        }
    }

    /// Returns true if this is air (empty)
    pub fn is_air(self) -> bool {
        self.id == 0
    }

    /// Returns true if the block is opaque (hides adjacent faces)
    pub fn is_opaque(self) -> bool {
        self.render_type.culls_adjacent_faces()
    }

    /// Returns true if this is a solid block (physically)
    pub fn is_solid(self) -> bool {
        self.collidable
    }

    /// Returns true if the block is visible
    pub fn is_visible(self) -> bool {
        self.render_type.is_visible()
    }
}

/// Checks if a face should be rendered by checking occlusion with neighbor block
///
/// # Culling rules (based on RenderType):
/// - If neighbor is opaque → face hidden (culled)
/// - If neighbor is same type and opaque → face hidden (culled to avoid internal faces)
/// - If neighbor is invisible (air) → face visible
///
/// # Arguments
/// * `current` - Current voxel
/// * `neighbor` - Neighbor voxel in the face direction
///
/// # Returns
/// `true` if the face should be rendered, `false` if it's hidden
pub fn should_render_face(current: VoxelRef, neighbor: VoxelRef) -> bool {
    // Air never hides faces
    if !neighbor.is_visible() { return true; }

    // Opaque blocks hide faces behind them
    if neighbor.is_opaque() { return false; }

    // If both are same opaque type, cull (avoids internal faces)
    if current.id == neighbor.id && current.is_opaque() {
        return false;
    }

    true
}

/// Voxel world access context for mesh generation
pub struct VoxelWorldContext<'a> {
    pub world: &'a VoxelWorld,
    pub registry: &'a SharedVoxelRegistry,
}

impl<'a> VoxelWorldContext<'a> {
    /// Gets a voxel by world coordinates
    pub fn get_voxel(&self, x: i32, y: i32, z: i32) -> VoxelRef {
        if let Some(id) = self.world.get_voxel_opt(x, y, z) {
            VoxelRef::from_id(id, self.registry)
        } else {
            VoxelRef::AIR
        }
    }

    /// Gets a voxel by chunk-relative position
    pub fn get_voxel_chunk_local(&self, cx: i32, cy: i32, cz: i32, lx: u32, ly: u32, lz: u32) -> VoxelRef {
        self.world.get_chunk_existing(cx, cy, cz)
            .and_then(|chunk| chunk.get(lx, ly, lz))
            .map_or(VoxelRef::AIR, |id| VoxelRef::from_id(id, self.registry))
    }
}

pub fn generate_chunk_mesh(
    chunk: &VoxelChunk,
    world: &VoxelWorld,
    cx: i32,
    cy: i32,
    cz: i32,
    registry: &SharedVoxelRegistry,
    ao_intensity: f32,
) -> Vec<VoxelVertex> {
    generate_chunk_mesh_with_diagonal(chunk, world, cx, cy, cz, registry, TriangleDiagonal::Primary, ao_intensity)
}

/// Generates chunk meshes with a specific triangle diagonal choice
pub fn generate_chunk_mesh_with_diagonal(
    chunk: &VoxelChunk,
    world: &VoxelWorld,
    cx: i32,
    cy: i32,
    cz: i32,
    registry: &SharedVoxelRegistry,
    diagonal: TriangleDiagonal,
    ao_intensity: f32,
) -> Vec<VoxelVertex> {
    let mut vertices = Vec::new();
    let chunk_offset = IVec3::new(cx * CHUNK_SIZE as i32, cy * CHUNK_SIZE as i32, cz * CHUNK_SIZE as i32);

    let context = VoxelWorldContext { world, registry };

    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let global_id = chunk.get(x, y, z);
                let Some(id) = global_id else { continue; };
                if id == 0 { continue; }

                let world_pos = chunk_offset + IVec3::new(x as i32, y as i32, z as i32);

                // Always use the model system (data-driven)
                let model = get_or_create_model(id, registry);
                add_model_faces(&mut vertices, world_pos, &model, id, registry, &context, ao_intensity);
            }
        }
    }

    vertices
}

/// Gets the model for a block - all blocks must have a model defined (data-driven)
fn get_or_create_model(id: GlobalVoxelId, registry: &SharedVoxelRegistry) -> ResolvedBlockModel {
    // Get the model from the registry
    let definition = registry.get(id);
    if let Some(def) = definition {
        if let Some(model_name) = &def.model_name {
            if let Some(model) = registry.get_resolved_model(model_name) {
                return model.clone();
            }
        }
    }

    // If no model is defined, create a simple cube model using a default texture
    // This should only happen for blocks without proper model definitions
    create_fallback_cube_model(id, registry)
}

/// Creates a fallback cube model (used when no model is defined)
fn create_fallback_cube_model(id: GlobalVoxelId, registry: &SharedVoxelRegistry) -> ResolvedBlockModel {
    // Try to get the block's string_id to use as texture name
    let texture_name = registry.get(id)
        .map(|def| def.string_id.clone())
        .unwrap_or_else(|| "stone".to_string());

    // Try to get texture UV to determine texture_id
    let texture_id = registry.get_texture_uv(&texture_name)
        .map(|_| {
            // If texture exists in atlas, find its ID
            // This is a simplified approach - in reality we'd need to track texture IDs
            0usize // Default to first texture
        })
        .unwrap_or(0);

    // Create a single element (full cube)
    let bounds = ElementBounds {
        from: [0.0, 0.0, 0.0],
        to: [16.0, 16.0, 16.0],
    };

    // Create faces for all 6 sides
    let mut faces = std::collections::HashMap::new();
    for face in [
        VoxelFace::Top, VoxelFace::Bottom, VoxelFace::North,
        VoxelFace::South, VoxelFace::East, VoxelFace::West,
    ] {
        faces.insert(face, ResolvedElementFace {
            texture_id,
            enabled: true,
            cull_face: Some(face),
        });
    }

    ResolvedBlockModel {
        name: format!("fallback_{}", texture_name),
        elements: vec![ResolvedModelElement {
            bounds,
            faces,
        }],
        render_type: "opaque".to_string(),
        collidable: true,
    }
}

/// Adds model faces to the mesh
fn add_model_faces(
    vertices: &mut Vec<VoxelVertex>,
    pos: IVec3,
    model: &ResolvedBlockModel,
    global_id: GlobalVoxelId,
    registry: &SharedVoxelRegistry,
    context: &VoxelWorldContext,
    ao_intensity: f32,
) {
    let current_voxel = VoxelRef::from_id(global_id, registry);

    for element in &model.elements {
        for (face, element_face) in &element.faces {
            if !element_face.enabled {
                continue;
            }

            // Determine which face to check for culling
            let cull_face = element_face.cull_face.unwrap_or(*face);
            let neighbor_pos = pos + cull_face.normal();
            let neighbor = context.get_voxel(neighbor_pos.x, neighbor_pos.y, neighbor_pos.z);

            if should_render_face(current_voxel, neighbor) {
                add_element_face(
                    vertices,
                    pos,
                    &element.bounds,
                    *face,
                    element_face.texture_id,
                    registry,
                    context,
                    ao_intensity,
                );
            }
        }
    }
}

/// Adds an element face to the mesh
fn add_element_face(
    vertices: &mut Vec<VoxelVertex>,
    pos: IVec3,
    bounds: &ElementBounds,
    face: VoxelFace,
    texture_id: usize,
    registry: &SharedVoxelRegistry,
    context: &VoxelWorldContext,
    ao_intensity: f32,
) {
    let normal = face.normal_f32();
    let voxel_pos = [pos.x, pos.y, pos.z];

    // Get vertices and UVs for this element face
    let face_vertices = bounds.get_face_vertices(face);
    let face_uvs = bounds.get_face_uvs(face);

    // Get UV from texture atlas using texture_id
    let texture_uv = registry.get_texture_uv_by_id(texture_id);
    let (uv_offset_x, uv_offset_y) = (texture_uv.u_min, texture_uv.v_min);
    let texture_size = texture_uv.size_in_atlas;

    // For AO, we use the voxel position (not the element position)
    let quad_uvs = face.quad_uvs();
    let vertex_to_corner = [0, 1, 2, 0, 2, 3];
    let corner_ao = calculate_face_ao(context, pos, face, ao_intensity);
    let light = calculate_face_light(face);

    for i in 0..6 {
        let base_uv = face_uvs[i];
        let uv = [
            uv_offset_x + base_uv[0] * texture_size,
            uv_offset_y + (1.0 - base_uv[1]) * texture_size, // Flip Y for UV
        ];

        // Interpolate AO using the quad UV corresponding to this vertex's corner
        let corner_uv = quad_uvs[vertex_to_corner[i]];
        let ao = interpolate_ao_for_vertex(corner_ao, quad_uvs, corner_uv);
        let final_light = light * ao;

        vertices.push(VoxelVertex {
            position: face_vertices[i],
            normal,
            voxel_pos,
            uv,
            color: [final_light, final_light, final_light, 1.0],
        });
    }
}

fn calculate_face_ao(context: &VoxelWorldContext, world_pos: IVec3, face: VoxelFace, ao_intensity: f32) -> [f32; 4] {
    AoCalculator::calculate_face_with_intensity(world_pos, face, |x, y, z| {
        let v = context.get_voxel(x, y, z);
        if v.is_air() { None } else { Some(v.id) }
    }, ao_intensity)
}

fn calculate_face_light(face: VoxelFace) -> f32 {
    let normal = face.normal();
    let normal_vec = Vec3::new(normal.x as f32, normal.y as f32, normal.z as f32);
    let diffuse = normal_vec.dot(LIGHT_DIR).max(0.0) * DIFFUSE_STRENGTH;
    AMBIENT_STRENGTH + diffuse
}

fn interpolate_ao_for_vertex(corner_ao: [f32; 4], _quad_uvs: [[f32; 2]; 4], vertex_uv: [f32; 2]) -> f32 {
    let top = corner_ao[0].lerp(corner_ao[1], 1.0 - vertex_uv[0]);
    let bottom = corner_ao[3].lerp(corner_ao[2], 1.0 - vertex_uv[0]);
    top.lerp(bottom, 1.0 - vertex_uv[1])
}
