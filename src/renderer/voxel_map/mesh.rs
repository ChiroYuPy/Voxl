use crate::voxel::{VoxelChunk, CHUNK_SIZE, VoxelWorld, GlobalVoxelId, SharedVoxelRegistry, WORLD_HEIGHT};
use crate::voxel::face::TriangleDiagonal;
use super::{VoxelFace, AoCalculator};
use glam::{IVec3, Vec3};
use glam::FloatExt;  // Pour la méthode lerp sur f32

#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct VoxelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub voxel_pos: [i32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],  // RGBA - couleur calculée sur CPU (light * AO)
}

/// Direction de la lumière (normalisée à la main pour const)
const LIGHT_DIR: Vec3 = Vec3::new(0.4472136, 0.8944272, 0.0);  // normalize(0.5, 1.0, 0.3)

/// Intensité de lumière ambiante
const AMBIENT_STRENGTH: f32 = 0.3;

/// Intensité maximale de lumière diffuse
const DIFFUSE_STRENGTH: f32 = 0.7;

pub fn is_face_visible_inter<F>(pos: IVec3, face: VoxelFace, mut get_voxel: F) -> bool
where
    F: FnMut(i32, i32, i32) -> Option<GlobalVoxelId>,
{
    let normal = face.normal();
    let n_pos = pos + normal;
    if n_pos.y < 0 || n_pos.y >= WORLD_HEIGHT as i32 {
        return true;
    }
    get_voxel(n_pos.x, n_pos.y, n_pos.z).map_or(true, |id| id == 0)
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

    let get_voxel = |wx: i32, wy: i32, wz: i32| -> Option<GlobalVoxelId> {
        let cx = wx.div_euclid(CHUNK_SIZE as i32);
        let cy = wy.div_euclid(CHUNK_SIZE as i32);
        let cz = wz.div_euclid(CHUNK_SIZE as i32);
        let x = wx.rem_euclid(CHUNK_SIZE as i32) as u32;
        let y = wy.rem_euclid(CHUNK_SIZE as i32) as u32;
        let z = wz.rem_euclid(CHUNK_SIZE as i32) as u32;

        if let Some(chunk) = world.get_chunk_existing(cx, cy, cz) {
            chunk.get(x, y, z)
        } else {
            None
        }
    };

    for x in 0..CHUNK_SIZE {
        for y in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let global_id = chunk.get(x, y, z);
                let Some(id) = global_id else {
                    continue;
                };
                if id == 0 {
                    continue; // Air
                }

                let world_pos = chunk_offset + IVec3::new(x as i32, y as i32, z as i32);

                for face in VoxelFace::ALL {
                    if is_face_visible_inter(world_pos, face, &get_voxel) {
                        add_face(&mut vertices, world_pos, face, id, registry, &get_voxel, diagonal);
                    }
                }
            }
        }
    }

    vertices
}

/// Calcule les valeurs d'AO pour les 4 coins d'une face
/// Le résultat est dans l'ordre: [coin0, coin1, coin2, coin3]
fn calculate_face_ao<F>(world_pos: IVec3, face: VoxelFace, get_voxel: F) -> [f32; 4]
where
    F: FnMut(i32, i32, i32) -> Option<GlobalVoxelId>,
{
    AoCalculator::calculate_face(world_pos, face, get_voxel)
}

/// Calcule la lumière (ambient + diffuse) pour une face donnée
fn calculate_face_light(face: VoxelFace) -> f32 {
    let normal = face.normal();
    let normal_vec = Vec3::new(normal.x as f32, normal.y as f32, normal.z as f32);
    let diffuse = normal_vec.dot(LIGHT_DIR).max(0.0) * DIFFUSE_STRENGTH;
    AMBIENT_STRENGTH + diffuse
}

fn add_face<F>(
    vertices: &mut Vec<VoxelVertex>,
    pos: IVec3,
    face: VoxelFace,
    global_id: GlobalVoxelId,
    registry: &SharedVoxelRegistry,
    get_voxel: &F,
    diagonal: TriangleDiagonal,
) where
    F: Fn(i32, i32, i32) -> Option<GlobalVoxelId>,
{
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

    // Obtenir les UV du quad pour l'interpolation de l'AO
    let quad_uvs = face.quad_uvs();

    // Calculer l'AO pour les 4 coins de la face
    // Ordre: coin0=(1,1), coin1=(0,1), coin2=(0,0), coin3=(1,0)
    let corner_ao = calculate_face_ao(pos, face, get_voxel);

    // Calculer la lumière pour cette face
    let light = calculate_face_light(face);

    // Pour chaque vertex, calculer la couleur finale (light * AO)
    for i in 0..6 {
        let base_uv = face_uvs[i];
        let uv = [uv_offset_x + base_uv[0] * texture_size, uv_offset_y + base_uv[1] * texture_size];

        // Trouver l'AO pour ce vertex en interpolant selon les UV du quad
        let ao = interpolate_ao_for_vertex(corner_ao, quad_uvs, base_uv);

        // Couleur finale = lumière * AO (le shader multipliera par la texture)
        let final_light = light * ao;

        vertices.push(VoxelVertex {
            position: face_vertices[i],
            normal,
            voxel_pos,
            uv,
            color: [final_light, final_light, final_light, 1.0],  // RGB = même valeur, A = 1.0
        });
    }
}

/// Interpole l'AO pour un vertex donné selon ses UV
/// Les corner_ao sont dans l'ordre: [coin0=(1,1), coin1=(0,1), coin2=(0,0), coin3=(1,0)]
fn interpolate_ao_for_vertex(corner_ao: [f32; 4], _quad_uvs: [[f32; 2]; 4], vertex_uv: [f32; 2]) -> f32 {
    // Interpolation bilinéaire comme dans le shader
    let top = corner_ao[0].lerp(corner_ao[1], 1.0 - vertex_uv[0]);  // coin0 vers coin1
    let bottom = corner_ao[3].lerp(corner_ao[2], 1.0 - vertex_uv[0]);  // coin3 vers coin2
    top.lerp(bottom, 1.0 - vertex_uv[1])  // top vers bottom
}
