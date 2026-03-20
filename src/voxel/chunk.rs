use super::GlobalVoxelId;

pub const CHUNK_SIZE: u32 = 16;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE as usize * CHUNK_SIZE as usize * CHUNK_SIZE as usize;

/// Hauteur totale du monde en blocs (peut avoir plusieurs chunks verticaux)
pub const WORLD_HEIGHT: u32 = 128;

/// Nombre de chunks verticaux nécessaires pour WORLD_HEIGHT
pub const VERTICAL_CHUNKS: u32 = (WORLD_HEIGHT + CHUNK_SIZE - 1) / CHUNK_SIZE;

/// Id local dans le chunk (u16 permet 65536 types différents)
pub type LocalVoxelId = u16;

/// Chunk avec système de palette pour optimiser la mémoire
#[derive(Clone)]
pub struct VoxelChunk {
    /// Voxels stockés avec des ids locaux (0 = air)
    voxels: Box<[LocalVoxelId; CHUNK_VOLUME]>,
    /// Palette: local_id -> global_id
    /// La première entrée (index 0) est toujours l'air (global_id 0)
    palette: Vec<GlobalVoxelId>,
}

impl VoxelChunk {
    pub fn new() -> Self {
        Self {
            // Tous les voxels sont initialisés à 0 (air)
            voxels: Box::new([0; CHUNK_VOLUME]),
            // La palette contient au moins l'air (global_id 0 -> local_id 0)
            palette: vec![0],
        }
    }

    #[inline(always)]
    fn index(x: u32, y: u32, z: u32) -> usize {
        ((z * CHUNK_SIZE + y) * CHUNK_SIZE + x) as usize
    }

    /// Retourne le global_id d'un voxel, ou None si hors limites du monde
    #[inline(always)]
    pub fn get(&self, x: u32, y: u32, z: u32) -> Option<GlobalVoxelId> {
        // Vérifier les limites du monde
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE || y >= WORLD_HEIGHT {
            return None;
        }

        // Vérifier si y est dans ce chunk (0-15)
        if y >= CHUNK_SIZE {
            return None; // Ce chunk ne gère que y=0-15
        }

        let local_id = self.voxels[Self::index(x, y, z)];
        self.palette.get(local_id as usize).copied()
    }

    /// Définit le global_id d'un voxel
    #[inline(always)]
    pub fn set(&mut self, x: u32, y: u32, z: u32, global_id: Option<GlobalVoxelId>) {
        // Seuls les voxels dans ce chunk (y=0-15) peuvent être définis
        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            let global_id = global_id.unwrap_or(0); // 0 = air
            let local_id = self.get_or_create_local_id(global_id);
            self.voxels[Self::index(x, y, z)] = local_id;
        }
    }

    /// Retourne le local_id pour un global_id, en créant une nouvelle entrée dans la palette si nécessaire
    fn get_or_create_local_id(&mut self, global_id: GlobalVoxelId) -> LocalVoxelId {
        // Recherche linéaire - pour un chunk avec peu de types c'est très rapide
        if let Some(pos) = self.palette.iter().position(|&id| id == global_id) {
            pos as LocalVoxelId
        } else {
            let local_id = self.palette.len() as LocalVoxelId;
            self.palette.push(global_id);
            local_id
        }
    }

    /// Retourne le global_id à partir du local_id
    #[inline(always)]
    pub fn local_to_global(&self, local_id: LocalVoxelId) -> GlobalVoxelId {
        self.palette.get(local_id as usize).copied().unwrap_or(0)
    }

    /// Retourne une référence à la palette (local_id -> global_id)
    pub fn palette(&self) -> &[GlobalVoxelId] {
        &self.palette
    }

    /// Retourne le nombre de types de blocs différents dans ce chunk
    pub fn palette_size(&self) -> usize {
        self.palette.len()
    }

    #[inline(always)]
    pub fn get_unchecked(&self, x: u32, y: u32, z: u32) -> Option<GlobalVoxelId> {
        self.get(x, y, z)
    }

    #[inline(always)]
    pub fn set_unchecked(&mut self, x: u32, y: u32, z: u32, global_id: Option<GlobalVoxelId>) {
        self.set(x, y, z, global_id);
    }

    /// Réinitialise le chunk (tous les voxels deviennent air, palette réinitialisée)
    pub fn clear(&mut self) {
        self.voxels = Box::new([0; CHUNK_VOLUME]);
        self.palette = vec![0];
    }

    /// Crée un chunk vide (équivalent à new() mais explicite)
    pub fn empty() -> Self {
        Self::new()
    }

    /// Extrait les données du chunk pour transfert inter-thread
    pub fn extract_data(&self) -> (Box<[LocalVoxelId; CHUNK_VOLUME]>, Vec<GlobalVoxelId>) {
        (
            self.voxels.clone(),
            self.palette.clone(),
        )
    }

    /// Reconstruit un chunk depuis des données extraites
    pub fn from_data(voxels: Box<[LocalVoxelId; CHUNK_VOLUME]>, palette: Vec<GlobalVoxelId>) -> Self {
        Self {
            voxels,
            palette,
        }
    }
}

impl Default for VoxelChunk {
    fn default() -> Self {
        Self::new()
    }
}
