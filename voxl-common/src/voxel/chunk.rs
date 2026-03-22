use super::GlobalVoxelId;

pub const CHUNK_SIZE: u32 = 16;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE as usize * CHUNK_SIZE as usize * CHUNK_SIZE as usize;

pub const WORLD_HEIGHT: u32 = 64;

pub const VERTICAL_CHUNKS: u32 = (WORLD_HEIGHT + CHUNK_SIZE - 1) / CHUNK_SIZE;

pub type LocalVoxelId = u16;

#[derive(Clone)]
pub struct VoxelChunk {
    voxels: Box<[LocalVoxelId; CHUNK_VOLUME]>,
    palette: Vec<GlobalVoxelId>,
}

impl VoxelChunk {
    pub fn new() -> Self {
        Self {
            voxels: Box::new([0; CHUNK_VOLUME]),
            palette: vec![0],
        }
    }

    #[inline(always)]
    fn index(x: u32, y: u32, z: u32) -> usize {
        ((z * CHUNK_SIZE + y) * CHUNK_SIZE + x) as usize
    }

    #[inline(always)]
    pub fn get(&self, x: u32, y: u32, z: u32) -> Option<GlobalVoxelId> {
        if x >= CHUNK_SIZE || z >= CHUNK_SIZE || y >= WORLD_HEIGHT {
            return None;
        }
        if y >= CHUNK_SIZE {
            return None;
        }

        let local_id = self.voxels[Self::index(x, y, z)];
        self.palette.get(local_id as usize).copied()
    }

    #[inline(always)]
    pub fn set(&mut self, x: u32, y: u32, z: u32, global_id: Option<GlobalVoxelId>) {
        // Seuls les voxels dans ce chunk (y=0-15) peuvent être définis
        if x < CHUNK_SIZE && y < CHUNK_SIZE && z < CHUNK_SIZE {
            let global_id = global_id.unwrap_or(0); // 0 = air
            let local_id = self.get_or_create_local_id(global_id);
            self.voxels[Self::index(x, y, z)] = local_id;
        }
    }

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

    /// Counts the number of non-air blocks in this chunk
    pub fn count_blocks(&self) -> u32 {
        let mut count = 0u32;
        for i in 0..CHUNK_VOLUME {
            let local_id = self.voxels[i];
            if local_id != 0 {
                count += 1;
            }
        }
        count
    }

    /// Sérialise le chunk en bytes pour le transfert réseau
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // Palette size (u32)
        let palette_size: u32 = self.palette.len() as u32;
        bytes.extend_from_slice(&palette_size.to_be_bytes());

        // Palette data (u32 each for platform-independent serialization)
        for global_id in &self.palette {
            bytes.extend_from_slice(&(*global_id as u32).to_be_bytes());
        }

        // Voxels data (u16 each)
        for i in 0..CHUNK_VOLUME {
            let local_id: u16 = self.voxels[i];
            bytes.extend_from_slice(&local_id.to_be_bytes());
        }

        bytes
    }

    /// Désérialise un chunk depuis des bytes reçus du réseau
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() < 4 {
            return Err("Data too short for palette size".to_string());
        }

        // Read palette size
        let palette_size = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;

        let expected_size = 4 + (palette_size * 4) + (CHUNK_VOLUME * 2);
        if bytes.len() != expected_size {
            return Err(format!("Invalid data size: expected {}, got {}", expected_size, bytes.len()));
        }

        let mut offset = 4;

        // Read palette
        let mut palette = Vec::with_capacity(palette_size);
        for _ in 0..palette_size {
            if offset + 4 > bytes.len() {
                return Err("Data too short for palette".to_string());
            }
            let global_id = u32::from_be_bytes([
                bytes[offset],
                bytes[offset + 1],
                bytes[offset + 2],
                bytes[offset + 3],
            ]);
            palette.push(global_id as usize);  // Convert u32 to usize
            offset += 4;
        }

        // Read voxels
        let mut voxels = Box::new([0u16; CHUNK_VOLUME]);
        for i in 0..CHUNK_VOLUME {
            if offset + 2 > bytes.len() {
                return Err("Data too short for voxels".to_string());
            }
            let local_id = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
            voxels[i] = local_id;
            offset += 2;
        }

        Ok(Self {
            voxels,
            palette,
        })
    }
}

impl Default for VoxelChunk {
    fn default() -> Self {
        Self::new()
    }
}
