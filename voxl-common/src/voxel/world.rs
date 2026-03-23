use super::{VoxelChunk, GlobalVoxelId, SharedVoxelRegistry, CHUNK_SIZE, WORLD_HEIGHT};
use glam::IVec3;
use std::collections::HashMap;

pub trait VoxelGrid {
    fn get_voxel_opt(&self, x: i32, y: i32, z: i32) -> Option<GlobalVoxelId>;

    fn get_voxel(&self, x: i32, y: i32, z: i32) -> Option<GlobalVoxelId> {
        self.get_voxel_opt(x, y, z)
    }

    fn get_block(&self, x: i32, y: i32, z: i32) -> bool {
        self.get_voxel(x, y, z).map_or(false, |id| id != 0)
    }

    fn min_bounds(&self) -> IVec3;
    fn max_bounds(&self) -> IVec3;
}

impl<T: VoxelGrid + ?Sized> VoxelGrid for &T {
    fn get_voxel_opt(&self, x: i32, y: i32, z: i32) -> Option<GlobalVoxelId> {
        (**self).get_voxel_opt(x, y, z)
    }

    fn min_bounds(&self) -> IVec3 {
        (**self).min_bounds()
    }

    fn max_bounds(&self) -> IVec3 {
        (**self).max_bounds()
    }
}

pub type ChunkPos = (i32, i32, i32);  // (cx, cy, cz)

#[derive(Clone, Debug)]
pub struct SetResult {
    pub modified_chunk: ChunkPos,
    pub neighbor_chunks: Vec<ChunkPos>,
}

/// Résultat de l'insertion d'un chunk
#[derive(Clone, Debug)]
pub struct InsertResult {
    pub chunk_pos: ChunkPos,
    pub existing_neighbors: Vec<ChunkPos>,  // Voisins qui existaient déjà (besoin de remesh)
}

pub struct VoxelWorld {
    chunks: HashMap<ChunkPos, VoxelChunk>,
    registry: SharedVoxelRegistry,
}

impl VoxelWorld {
    pub fn new(registry: SharedVoxelRegistry) -> Self {
        Self {
            chunks: HashMap::new(),
            registry,
        }
    }

    pub fn registry(&self) -> &SharedVoxelRegistry {
        &self.registry
    }

    pub fn create_chunk(&mut self, cx: i32, cy: i32, cz: i32) {
        self.chunks.insert((cx, cy, cz), VoxelChunk::new());
    }

    pub fn get_or_create_chunk(&mut self, cx: i32, cy: i32, cz: i32) -> &mut VoxelChunk {
        if !self.chunks.contains_key(&(cx, cy, cz)) {
            self.chunks.insert((cx, cy, cz), VoxelChunk::new());
        }
        self.chunks.get_mut(&(cx, cy, cz)).unwrap()
    }

    pub fn get_chunk_existing(&self, cx: i32, cy: i32, cz: i32) -> Option<&VoxelChunk> {
        self.chunks.get(&(cx, cy, cz))
    }

    pub fn get_chunk_mut(&mut self, cx: i32, cy: i32, cz: i32) -> Option<&mut VoxelChunk> {
        self.chunks.get_mut(&(cx, cy, cz))
    }

    /// Insère un chunk complet directement (utile pour le chargement asynchrone)
    /// Retourne les voisins qui existaient déjà (pour remesh)
    pub fn insert_chunk(&mut self, cx: i32, cy: i32, cz: i32, chunk: VoxelChunk) -> InsertResult {
        // Vérifier quels voisins existent déjà avant l'insertion
        let potential_neighbors = [
            (cx + 1, cy, cz), (cx - 1, cy, cz),
            (cx, cy + 1, cz), (cx, cy - 1, cz),
            (cx, cy, cz + 1), (cx, cy, cz - 1),
        ];

        let existing_neighbors: Vec<ChunkPos> = potential_neighbors.iter()
            .filter(|(nx, ny, nz)| {
                *ny >= 0 && self.chunks.contains_key(&(*nx, *ny, *nz))
            })
            .copied()
            .collect();

        // Insérer le chunk
        self.chunks.insert((cx, cy, cz), chunk);

        InsertResult {
            chunk_pos: (cx, cy, cz),
            existing_neighbors,
        }
    }

    pub fn set_voxel(&mut self, x: i32, y: i32, z: i32, global_id: Option<GlobalVoxelId>) -> SetResult {
        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cy = y.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);

        let lx = x.rem_euclid(CHUNK_SIZE as i32) as u32;
        let ly = y.rem_euclid(CHUNK_SIZE as i32) as u32;
        let lz = z.rem_euclid(CHUNK_SIZE as i32) as u32;

        if y < 0 || y >= WORLD_HEIGHT as i32 {
            return SetResult {
                modified_chunk: (cx, cy, cz),
                neighbor_chunks: Vec::new(),
            };
        }

        if !self.chunks.contains_key(&(cx, cy, cz)) {
            self.chunks.insert((cx, cy, cz), VoxelChunk::new());
        }

        let chunk = self.chunks.get_mut(&(cx, cy, cz)).unwrap();
        chunk.set(lx, ly, lz, global_id);

        let mut neighbor_chunks = Vec::new();

        // Marquer les chunks voisins comme dirty si on est à la limite du chunk
        // (ils doivent être rebuild car la visibilité des faces change)
        if lx == 0 {
            neighbor_chunks.push((cx - 1, cy, cz));
        }
        if lx == CHUNK_SIZE as u32 - 1 {
            neighbor_chunks.push((cx + 1, cy, cz));
        }
        if ly == 0 && cy > 0 {
            neighbor_chunks.push((cx, cy - 1, cz));  // Chunk en-dessous
        }
        if ly == CHUNK_SIZE as u32 - 1 {
            neighbor_chunks.push((cx, cy + 1, cz));  // Chunk au-dessus
        }
        if lz == 0 {
            neighbor_chunks.push((cx, cy, cz - 1));
        }
        if lz == CHUNK_SIZE as u32 - 1 {
            neighbor_chunks.push((cx, cy, cz + 1));
        }

        SetResult {
            modified_chunk: (cx, cy, cz),
            neighbor_chunks,
        }
    }

    pub fn get_voxel_opt(&self, x: i32, y: i32, z: i32) -> Option<GlobalVoxelId> {
        if y < 0 || y >= WORLD_HEIGHT as i32 {
            return None;
        }

        let cx = x.div_euclid(CHUNK_SIZE as i32);
        let cy = y.div_euclid(CHUNK_SIZE as i32);
        let cz = z.div_euclid(CHUNK_SIZE as i32);

        if let Some(chunk) = self.get_chunk_existing(cx, cy, cz) {
            let lx = x.rem_euclid(CHUNK_SIZE as i32) as u32;
            let ly = y.rem_euclid(CHUNK_SIZE as i32) as u32;
            let lz = z.rem_euclid(CHUNK_SIZE as i32) as u32;
            chunk.get(lx, ly, lz)
        } else {
            None
        }
    }

    pub fn get_voxel(&self, x: i32, y: i32, z: i32) -> Option<GlobalVoxelId> {
        self.get_voxel_opt(x, y, z)
    }

    pub fn chunks_iter(&self) -> impl Iterator<Item = (&ChunkPos, &VoxelChunk)> {
        self.chunks.iter()
    }

    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

impl VoxelGrid for VoxelWorld {
    fn get_voxel_opt(&self, x: i32, y: i32, z: i32) -> Option<GlobalVoxelId> {
        self.get_voxel_opt(x, y, z)
    }

    fn min_bounds(&self) -> IVec3 {
        if self.chunks.is_empty() {
            IVec3::ZERO
        } else {
            let (min_cx, _min_cy, min_cz) = self.chunks.keys().map(|(cx, cy, cz)| (*cx, *cy, *cz))
                .min_by_key(|(cx, _cy, cz)| (*cx, *cz))
                .unwrap();
            IVec3::new(min_cx * CHUNK_SIZE as i32, 0, min_cz * CHUNK_SIZE as i32)
        }
    }

    fn max_bounds(&self) -> IVec3 {
        if self.chunks.is_empty() {
            IVec3::ZERO
        } else {
            let (max_cx, _max_cy, max_cz) = self.chunks.keys().map(|(cx, cy, cz)| (*cx, *cy, *cz))
                .max_by_key(|(cx, _cy, cz)| (*cx, *cz))
                .unwrap();
            IVec3::new(
                (max_cx + 1) * CHUNK_SIZE as i32,
                WORLD_HEIGHT as i32,
                (max_cz + 1) * CHUNK_SIZE as i32,
            )
        }
    }
}
