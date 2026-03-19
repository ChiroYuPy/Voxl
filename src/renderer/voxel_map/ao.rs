use crate::voxel::{VoxelFace, GlobalVoxelId};
use glam::IVec3;

/// Valeur d'AO pour un coin (0.0 = sombre, 1.0 = clair)
#[derive(Clone, Copy, Debug)]
pub struct AoLevel {
    pub value: f32,
}

impl AoLevel {
    pub const MAX: Self = Self { value: 1.0 };
    pub const MIN: Self = Self { value: 0.0 };

    /// Valeurs d'AO possibles dans Minecraft: 1.0, 0.66..., 0.33..., 0.0
    /// Plus il y a de blocs opaques autour, plus sombre
    pub fn from_neighbors(side1: bool, side2: bool, corner: bool) -> Self {
        let value = if side1 && side2 {
            0.0  // Les deux côtés sont bloqués → coin très sombre
        } else {
            3.0 - (side1 as u8 + side2 as u8 + corner as u8) as f32
        };
        Self { value: value / 3.0 }
    }

    pub fn as_f32(self) -> f32 {
        self.value
    }
}

/// Données de voisinage pour une direction de face
///
/// Pour chaque coin (0, 1, 2, 3), définit les directions des voisins à vérifier:
/// - side1: Premier voisin orthogonal (sur face adjacente)
/// - side2: Deuxième voisin orthogonal (sur autre face adjacente)
struct NeighborData {
    /// Les 2 directions orthogonales pour chaque coin [direction1, direction2]
    side_directions: [[VoxelFace; 2]; 4],
}

impl NeighborData {
    fn new(side_dirs: [[VoxelFace; 2]; 4]) -> Self {
        Self {
            side_directions: side_dirs,
        }
    }

    /// Obtient les données de voisinage pour une direction de face
    fn get_for_face(face: VoxelFace) -> Self {
        match face {
            // Face UP (y+): Les coins sont vus du dessus
            // L'ordre des coins suit les UV: (1,1), (0,1), (0,0), (1,0)
            VoxelFace::Top => {
                Self::new([
                    [VoxelFace::South, VoxelFace::West],  // Coin 0 - SW
                    [VoxelFace::South, VoxelFace::East],  // Coin 1 - SE
                    [VoxelFace::North, VoxelFace::East],  // Coin 2 - NE
                    [VoxelFace::North, VoxelFace::West],  // Coin 3 - NW
                ])
            }

            // Face DOWN (y-)
            VoxelFace::Bottom => {
                Self::new([
                    [VoxelFace::North, VoxelFace::West],  // Coin 0
                    [VoxelFace::North, VoxelFace::East],  // Coin 1
                    [VoxelFace::South, VoxelFace::East],  // Coin 2
                    [VoxelFace::South, VoxelFace::West],  // Coin 3
                ])
            }

            // Face NORTH (z+)
            VoxelFace::North => {
                Self::new([
                    [VoxelFace::East, VoxelFace::Bottom],  // Coin 0
                    [VoxelFace::West, VoxelFace::Bottom],  // Coin 1
                    [VoxelFace::West, VoxelFace::Top],     // Coin 2
                    [VoxelFace::East, VoxelFace::Top],     // Coin 3
                ])
            }

            // Face SOUTH (z-)
            VoxelFace::South => {
                Self::new([
                    [VoxelFace::West, VoxelFace::Bottom],  // Coin 0
                    [VoxelFace::East, VoxelFace::Bottom],  // Coin 1
                    [VoxelFace::East, VoxelFace::Top],     // Coin 2
                    [VoxelFace::West, VoxelFace::Top],     // Coin 3
                ])
            }

            // Face EAST (x+)
            VoxelFace::East => {
                Self::new([
                    [VoxelFace::South, VoxelFace::Bottom],     // Coin 0
                    [VoxelFace::North, VoxelFace::Bottom],     // Coin 1
                    [VoxelFace::North, VoxelFace::Top],  // Coin 2
                    [VoxelFace::South, VoxelFace::Top],  // Coin 3
                ])
            }

            // Face WEST (x-)
            VoxelFace::West => {
                Self::new([
                    [VoxelFace::North, VoxelFace::Bottom],     // Coin 0
                    [VoxelFace::South, VoxelFace::Bottom],     // Coin 1
                    [VoxelFace::South, VoxelFace::Top],  // Coin 2
                    [VoxelFace::North, VoxelFace::Top],  // Coin 3
                ])
            }
        }
    }
}

/// Calculateur d'Ambient Occlusion
pub struct AoCalculator;

impl AoCalculator {
    /// Intensité de l'AO (0.0 = pas d'AO, 1.0 = AO maximale)
    pub const INTENSITY: f32 = 0.7;

    /// Calcule les valeurs d'AO pour les 4 coins d'une face
    ///
    /// IMPORTANT: Les voisins sont vérifiés depuis la position de la FACE (bloc + normale),
    /// pas depuis le bloc lui-même. Cela garantit qu'un sol plat a un AO de 1.0.
    ///
    /// # Arguments
    /// * `block_pos` - Position du bloc
    /// * `face` - Direction de la face
    /// * `get_voxel` - Fonction pour récupérer un voxel (retourne None si vide/air)
    ///
    /// # Retourne
    /// Un tableau de 4 valeurs d'AO [coin0, coin1, coin2, coin3]
    pub fn calculate_face<F>(block_pos: IVec3, face: VoxelFace, get_voxel: F) -> [f32; 4]
    where
        F: FnMut(i32, i32, i32) -> Option<GlobalVoxelId>,
    {
        Self::calculate_face_with_intensity(block_pos, face, get_voxel, Self::INTENSITY)
    }

    /// Calcule les valeurs d'AO avec une intensité spécifique
    pub fn calculate_face_with_intensity<F>(block_pos: IVec3, face: VoxelFace, mut get_voxel: F, intensity: f32) -> [f32; 4]
    where
        F: FnMut(i32, i32, i32) -> Option<GlobalVoxelId>,
    {
        let neighbor_data = NeighborData::get_for_face(face);

        // Position de la FACE (pas du bloc) - c'est crucial pour un AO correct
        let face_pos = block_pos + face.normal();

        let mut ao_values = [1.0f32; 4];

        for corner_idx in 0..4 {
            let side1_dir = neighbor_data.side_directions[corner_idx][0];
            let side2_dir = neighbor_data.side_directions[corner_idx][1];

            // Les voisins sont vérifiés depuis la position de la face
            // side1: voisin orthogonal 1 (sur face adjacente)
            let side1_pos = face_pos + side1_dir.normal();
            // side2: voisin orthogonal 2 (sur autre face adjacente)
            let side2_pos = face_pos + side2_dir.normal();
            // corner: voisin diagonal (combinaison des deux directions orthogonales)
            let corner_pos = face_pos + side1_dir.normal() + side2_dir.normal();

            // Vérifier l'opacité des voisins
            let side1_blocks = Self::is_opaque(side1_pos, &mut get_voxel);
            let side2_blocks = Self::is_opaque(side2_pos, &mut get_voxel);
            let corner_blocks = Self::is_opaque(corner_pos, &mut get_voxel);

            // Calculer l'AO brute pour ce coin
            let ao = AoLevel::from_neighbors(side1_blocks, side2_blocks, corner_blocks);

            // Appliquer l'intensité: lerp entre 1.0 (pas d'AO) et la valeur calculée
            // intensity = 0 → toujours 1.0 (pas d'AO)
            // intensity = 1 → valeur calculée (AO complète)
            let ao_value = 1.0 - (1.0 - ao.as_f32()) * intensity;
            ao_values[corner_idx] = ao_value;
        }

        ao_values
    }

    /// Vérifie si un voxel est opaque (bloque la lumière)
    fn is_opaque<F>(pos: IVec3, get_voxel: &mut F) -> bool
    where
        F: FnMut(i32, i32, i32) -> Option<GlobalVoxelId>,
    {
        get_voxel(pos.x, pos.y, pos.z)
            .map_or(false, |id| id != 0) // id=0 est l'air
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ao_level() {
        // Les deux côtés bloqués = très sombre
        let ao = AoLevel::from_neighbors(true, true, true);
        assert_eq!(ao.as_f32(), 0.0);

        // Aucun bloc = clair
        let ao = AoLevel::from_neighbors(false, false, false);
        assert_eq!(ao.as_f32(), 1.0);

        // Un seul côté bloqué = moyennement sombre
        let ao = AoLevel::from_neighbors(true, false, false);
        assert_eq!(ao.as_f32(), 2.0 / 3.0);
    }
}
