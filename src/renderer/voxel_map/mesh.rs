use crate::voxel::{VoxelChunk, CHUNK_SIZE, VoxelWorld, GlobalVoxelId, SharedVoxelRegistry, RenderType};
use crate::voxel::face::TriangleDiagonal;
use super::{VoxelFace, AoCalculator};
use glam::{IVec3, Vec3};
use glam::FloatExt;
use crate::voxel::model::{ResolvedBlockModel, ElementBounds};

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

/// Référence légère à un voxel pour les calculs de rendu
#[derive(Clone, Copy, Debug)]
pub struct VoxelRef {
    /// ID global du voxel (0 = air)
    pub id: GlobalVoxelId,
    /// Type de rendu du bloc
    pub render_type: RenderType,
    /// Est-ce que le bloc est solide (collision)
    pub collidable: bool,
}

impl VoxelRef {
    /// Crée une référence vide (air)
    pub const AIR: Self = Self {
        id: 0,
        render_type: RenderType::Invisible,
        collidable: false,
    };

    /// Crée une référence depuis un ID et le registry
    pub fn from_id(id: GlobalVoxelId, registry: &SharedVoxelRegistry) -> Self {
        let render_type = registry.get_render_type(id);
        let collidable = registry.is_collidable(id);
        Self { id, render_type, collidable }
    }

    /// Crée une référence depuis un ID (avec valeurs par défaut si registry pas dispo)
    pub const fn from_id_unchecked(id: GlobalVoxelId) -> Self {
        Self {
            id,
            render_type: if id == 0 { RenderType::Invisible } else { RenderType::Opaque },
            collidable: id != 0,
        }
    }

    /// Retourne true si c'est de l'air (vide)
    pub fn is_air(self) -> bool {
        self.id == 0
    }

    /// Retourne true si le bloc est opaque (cache les faces adjacentes)
    pub fn is_opaque(self) -> bool {
        self.render_type.culls_adjacent_faces()
    }

    /// Retourne true si c'est un bloc solide (physiquement)
    pub fn is_solid(self) -> bool {
        self.collidable
    }

    /// Retourne true si le bloc est visible
    pub fn is_visible(self) -> bool {
        self.render_type.is_visible()
    }
}

/// Vérifie si une face doit être rendue en vérifiant l'occlusion avec le bloc voisin.
///
/// # Règles de culling (basées sur RenderType):
/// - Si le voisin est opaque → face cachée (cull)
/// - Si le voisin est du même type et opaque → face cachée (cull pour éviter les faces internes)
/// - Si le voisin est invisible (air) → face visible
///
/// # Arguments
/// * `current` - Voxel actuel
/// * `neighbor` - Voxel voisin dans la direction de la face
///
/// # Retourne
/// `true` si la face doit être rendue, `false` si elle est cachée
pub fn should_render_face(current: VoxelRef, neighbor: VoxelRef) -> bool {
    // L'air ne cache jamais les faces
    if !neighbor.is_visible() { return true; }

    // Les blocs opaques cachent les faces derrière eux
    if neighbor.is_opaque() { return false; }

    // Si les deux sont du même type opaque, on cull (évite les faces internes)
    if current.id == neighbor.id && current.is_opaque() {
        return false;
    }

    true
}

/// Contexte d'accès au monde voxel pour la génération de mesh
pub struct VoxelWorldContext<'a> {
    pub world: &'a VoxelWorld,
    pub registry: &'a SharedVoxelRegistry,
}

impl<'a> VoxelWorldContext<'a> {
    /// Récupère un voxel par ses coordonnées mondiales
    pub fn get_voxel(&self, x: i32, y: i32, z: i32) -> VoxelRef {
        if let Some(id) = self.world.get_voxel_opt(x, y, z) {
            VoxelRef::from_id(id, self.registry)
        } else {
            VoxelRef::AIR
        }
    }

    /// Récupère un voxel par sa position relative (chunk + local)
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
) -> Vec<VoxelVertex> {
    generate_chunk_mesh_with_diagonal(chunk, world, cx, cy, cz, registry, TriangleDiagonal::Primary)
}

/// Génère les meshes avec un choix spécifique de diagonale pour les triangles
pub fn generate_chunk_mesh_with_diagonal(
    chunk: &VoxelChunk,
    world: &VoxelWorld,
    cx: i32,
    cy: i32,
    cz: i32,
    registry: &SharedVoxelRegistry,
    diagonal: TriangleDiagonal,
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
                let current_voxel = VoxelRef::from_id(id, registry);

                // Check if this voxel uses a model
                let definition = registry.get(id);
                if let Some(def) = definition {
                    if let Some(model_name) = &def.model_name {
                        if let Some(model) = registry.get_resolved_model(model_name) {
                            // Use model-based mesh generation
                            add_model_faces(&mut vertices, world_pos, &model, id, registry, &context);
                            continue;
                        }
                    }
                }

                // Legacy: use standard 6-face cube mesh
                for face in VoxelFace::ALL {
                    let neighbor_pos = world_pos + face.normal();
                    let neighbor = context.get_voxel(neighbor_pos.x, neighbor_pos.y, neighbor_pos.z);

                    if should_render_face(current_voxel, neighbor) {
                        add_face(&mut vertices, world_pos, face, id, registry, &context, diagonal);
                    }
                }
            }
        }
    }

    vertices
}

/// Ajoute les faces d'un modèle au mesh
fn add_model_faces(
    vertices: &mut Vec<VoxelVertex>,
    pos: IVec3,
    model: &ResolvedBlockModel,
    global_id: GlobalVoxelId,
    registry: &SharedVoxelRegistry,
    context: &VoxelWorldContext,
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
                );
            }
        }
    }
}

/// Ajoute une face d'un élément de modèle au mesh
fn add_element_face(
    vertices: &mut Vec<VoxelVertex>,
    pos: IVec3,
    bounds: &ElementBounds,
    face: VoxelFace,
    texture_id: usize,
    registry: &SharedVoxelRegistry,
    context: &VoxelWorldContext,
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
    // Les vertices sont dans l'ordre: coin0, coin1, coin2, coin0, coin2, coin3
    // quad_uvs contient les UV pour l'interpolation AO: [coin0, coin1, coin2, coin3]
    let quad_uvs = face.quad_uvs();
    // L'index du coin pour chaque vertex (0,1,2,0,2,3)
    let vertex_to_corner = [0, 1, 2, 0, 2, 3];
    let corner_ao = calculate_face_ao(context, pos, face);
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

fn calculate_face_ao(context: &VoxelWorldContext, world_pos: IVec3, face: VoxelFace) -> [f32; 4] {
    AoCalculator::calculate_face(world_pos, face, |x, y, z| {
        let v = context.get_voxel(x, y, z);
        if v.is_air() { None } else { Some(v.id) }
    })
}

fn calculate_face_light(face: VoxelFace) -> f32 {
    let normal = face.normal();
    let normal_vec = Vec3::new(normal.x as f32, normal.y as f32, normal.z as f32);
    let diffuse = normal_vec.dot(LIGHT_DIR).max(0.0) * DIFFUSE_STRENGTH;
    AMBIENT_STRENGTH + diffuse
}

fn add_face(
    vertices: &mut Vec<VoxelVertex>,
    pos: IVec3,
    face: VoxelFace,
    global_id: GlobalVoxelId,
    registry: &SharedVoxelRegistry,
    context: &VoxelWorldContext,
    diagonal: TriangleDiagonal,
) {
    let normal = face.normal_f32();
    let voxel_pos = [pos.x, pos.y, pos.z];
    let (uv_offset_x, uv_offset_y) = registry
        .get(global_id)
        .map(|def| def.get_uv_for_face(&face))
        .unwrap_or((0.0, 0.0));
    let face_vertices = face.triangles(diagonal);
    let face_uvs = face.triangle_uvs(diagonal);
    let texture_size = registry
        .get(global_id)
        .and_then(|def| Some(def.uv_top.size_in_atlas))
        .unwrap_or(0.5);

    let quad_uvs = face.quad_uvs();
    let corner_ao = calculate_face_ao(context, pos, face);
    let light = calculate_face_light(face);

    for i in 0..6 {
        let base_uv = face_uvs[i];
        let uv = [uv_offset_x + base_uv[0] * texture_size, uv_offset_y + base_uv[1] * texture_size];

        let ao = interpolate_ao_for_vertex(corner_ao, quad_uvs, base_uv);
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

fn interpolate_ao_for_vertex(corner_ao: [f32; 4], _quad_uvs: [[f32; 2]; 4], vertex_uv: [f32; 2]) -> f32 {
    let top = corner_ao[0].lerp(corner_ao[1], 1.0 - vertex_uv[0]);
    let bottom = corner_ao[3].lerp(corner_ao[2], 1.0 - vertex_uv[0]);
    top.lerp(bottom, 1.0 - vertex_uv[1])
}
