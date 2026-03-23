//! World generation module
//!
//! Contains all terrain generation algorithms that can be shared
//! between client and server.

use crate::voxel::{VoxelChunk, SharedVoxelRegistry, CHUNK_SIZE, WORLD_HEIGHT, GlobalVoxelId};
use tracing::info;

/// World generator with simple heightmap-based terrain
#[derive(Clone, Copy)]
pub struct WorldGenerator {
    /// Block IDs pre-calculated for performance
    grass_id: GlobalVoxelId,
    dirt_id: GlobalVoxelId,
    stone_id: GlobalVoxelId,
    bedrock_id: GlobalVoxelId,
}

impl WorldGenerator {
    /// Creates a new world generator
    pub fn new() -> Self {
        Self {
            grass_id: 1,
            dirt_id: 2,
            stone_id: 3,
            bedrock_id: 4,
        }
    }

    /// Initializes block IDs from the registry
    pub fn init_block_ids(&mut self, registry: &SharedVoxelRegistry) {
        self.grass_id = registry.get_id_by_string("grass").unwrap_or(1);
        self.dirt_id = registry.get_id_by_string("dirt").unwrap_or(2);
        self.stone_id = registry.get_id_by_string("stone").unwrap_or(3);
        self.bedrock_id = registry.get_id_by_string("bedrock").unwrap_or(4);

        info!("[WorldGen] Initialized block IDs: grass={}, dirt={}, stone={}, bedrock={}",
            self.grass_id, self.dirt_id, self.stone_id, self.bedrock_id);
        info!("[WorldGen] Total blocks in registry: {}", registry.len());
    }

    /// Calculates terrain height at a given world position
    /// Uses terraced elevation levels with moderate height variation
    #[inline]
    pub fn terrain_height(&self, wx: i32, wz: i32) -> i32 {
        // Base height (lower than before for more accessible terrain)
        let base = 48.0;

        // Large rolling hills - reduced amplitude for lower mountains
        let scale_x = wx as f32 * 0.03;
        let scale_z = wz as f32 * 0.03;
        let large_hills = (scale_x.sin() * 3.5 + scale_z.cos() * 3.5);

        // Medium frequency terrain features
        let med_scale_x = wx as f32 * 0.08;
        let med_scale_z = wz as f32 * 0.08;
        let med_hills = (med_scale_x.sin() * 2.0 + med_scale_z.cos() * 2.0);

        // Small detail noise
        let detail_scale_x = wx as f32 * 0.2;
        let detail_scale_z = wz as f32 * 0.2;
        let detail = (detail_scale_x.sin() * 0.8 + detail_scale_z.cos() * 0.8);

        // Combine all variations
        let raw_height = base + large_hills + med_hills + detail;

        // Create terraces/levels by quantizing to 2-block steps
        // This gives a more "layered" Minecraft-like appearance
        let terraced = (raw_height / 2.0).floor() * 2.0;

        // Add subtle smoothing between terraces (optional)
        let smoothed = terraced + (raw_height % 2.0) * 0.3;

        // Clamp to reasonable range [38, 62] - lower than before
        smoothed.clamp(38.0, 62.0) as i32
    }

    /// Generates a complete chunk with detailed logging
    ///
    /// Returns statistics about the generation:
    /// - number of blocks placed
    /// - time taken in nanoseconds
    pub fn generate_chunk_logged(&self, chunk: &mut VoxelChunk, _registry: &SharedVoxelRegistry,
                                   cx: i32, cy: i32, cz: i32) -> ChunkGenStats {
        let start = std::time::Instant::now();

        let base_y = cy * CHUNK_SIZE as i32;

        // Early exit conditions
        if base_y > WORLD_HEIGHT as i32 {
            return ChunkGenStats::empty(start.elapsed());
        }

        // Pre-calculate heights for the entire chunk (16x16 = 256 values)
        let mut heights = [[0i32; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];
        let mut max_h = 0i32;

        let wx_base = cx * CHUNK_SIZE as i32;
        let wz_base = cz * CHUNK_SIZE as i32;

        for z in 0..CHUNK_SIZE as usize {
            for x in 0..CHUNK_SIZE as usize {
                let wx = wx_base + x as i32;
                let wz = wz_base + z as i32;
                let h = self.terrain_height(wx, wz);
                heights[z][x] = h;
                if h > max_h {
                    max_h = h;
                }
            }
        }

        // If chunk is entirely above terrain, nothing to do
        if base_y > max_h {
            return ChunkGenStats::empty(start.elapsed());
        }

        // Generate terrain
        let mut blocks_placed = 0u32;

        for z in 0..CHUNK_SIZE as usize {
            for x in 0..CHUNK_SIZE as usize {
                let h = heights[z][x];

                // If height is below chunk, skip
                if h < base_y {
                    continue;
                }

                for y in 0..CHUNK_SIZE as usize {
                    let gy = base_y + y as i32;

                    // If above terrain height, skip
                    if gy > h {
                        continue;
                    }

                    // Bedrock at Y=0 (layer 0 exactly)
                    if gy == 0 {
                        chunk.set(x as u32, y as u32, z as u32, Some(self.bedrock_id));
                        blocks_placed += 1;
                        continue;
                    }

                    // Determine block based on position relative to surface
                    // gy == h: surface (grass)
                    // h-1 >= gy >= h-3: dirt (3 layers below surface)
                    // gy < h-3: stone
                    let block_id = if gy == h {
                        self.grass_id  // Surface = grass
                    } else if gy > h - 4 {
                        // 3 layers below surface: h-1, h-2, h-3
                        self.dirt_id
                    } else {
                        // Everything else: stone
                        self.stone_id
                    };

                    chunk.set(x as u32, y as u32, z as u32, Some(block_id));
                    blocks_placed += 1;
                }
            }
        }

        ChunkGenStats {
            blocks_placed,
            duration_ns: start.elapsed().as_nanos() as u64,
        }
    }

    /// Generates a chunk without logging (for performance-critical paths)
    #[inline]
    pub fn generate_chunk(&self, chunk: &mut VoxelChunk, registry: &SharedVoxelRegistry,
                          cx: i32, cy: i32, cz: i32) {
        self.generate_chunk_logged(chunk, registry, cx, cy, cz);
    }
}

impl Default for WorldGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics about chunk generation
#[derive(Debug, Clone, Copy)]
pub struct ChunkGenStats {
    /// Number of blocks placed in the chunk
    pub blocks_placed: u32,
    /// Time taken to generate in nanoseconds
    pub duration_ns: u64,
}

impl ChunkGenStats {
    fn empty(duration: std::time::Duration) -> Self {
        Self {
            blocks_placed: 0,
            duration_ns: duration.as_nanos() as u64,
        }
    }

    /// Returns the duration in milliseconds (for display)
    pub fn duration_ms(&self) -> f64 {
        self.duration_ns as f64 / 1_000_000.0
    }

    /// Returns the duration in microseconds (for display)
    pub fn duration_us(&self) -> f64 {
        self.duration_ns as f64 / 1_000.0
    }
}
