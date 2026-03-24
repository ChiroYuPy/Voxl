//! World generation module
//!
//! Contains all terrain generation algorithms that can be shared
//! between client and server.

use crate::voxel::{VoxelChunk, SharedVoxelRegistry, CHUNK_SIZE, WORLD_HEIGHT, GlobalVoxelId};
use tracing::info;
use noise::{Perlin, Simplex, NoiseFn};

/// Biome types for world generation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Biome {
    Plains,
    Desert,
    Mountains,
    River,
}

/// World generator with heightmap-based terrain, caves, and biomes
#[derive(Clone)]
pub struct WorldGenerator {
    /// Block IDs pre-calculated for performance
    grass_id: GlobalVoxelId,
    dirt_id: GlobalVoxelId,
    stone_id: GlobalVoxelId,
    bedrock_id: GlobalVoxelId,
    sand_id: GlobalVoxelId,
    water_id: GlobalVoxelId,

    /// Noise for terrain heightmap (base shape)
    terrain_noise: Perlin,

    /// Noise for detail (local variations)
    detail_noise: Perlin,

    /// Noise for biome selection
    biome_noise: Perlin,

    /// Noise for rivers (2D, creates river paths)
    river_noise: Perlin,

    /// 3D noise for cave generation
    cave_noise: Simplex,

    /// Seed for reproducible generation
    seed: u32,
}

impl WorldGenerator {
    /// Creates a new world generator with a default seed
    pub fn new() -> Self {
        Self::with_seed(12345)
    }

    /// Creates a new world generator with a specific seed
    pub fn with_seed(seed: u32) -> Self {
        Self {
            grass_id: 1,
            dirt_id: 2,
            stone_id: 3,
            bedrock_id: 4,
            sand_id: 5,
            water_id: 6,

            terrain_noise: Perlin::new(seed),
            detail_noise: Perlin::new(seed + 1),
            biome_noise: Perlin::new(seed + 2),
            river_noise: Perlin::new(seed + 3),
            cave_noise: Simplex::new(seed + 100),

            seed,
        }
    }

    /// Returns the current seed
    pub fn seed(&self) -> u32 {
        self.seed
    }

    /// Initializes block IDs from the registry
    pub fn init_block_ids(&mut self, registry: &SharedVoxelRegistry) {
        self.grass_id = registry.get_id_by_string("grass").unwrap_or(1);
        self.dirt_id = registry.get_id_by_string("dirt").unwrap_or(2);
        self.stone_id = registry.get_id_by_string("stone").unwrap_or(3);
        self.bedrock_id = registry.get_id_by_string("bedrock").unwrap_or(4);
        self.sand_id = registry.get_id_by_string("sand").unwrap_or(5);
        self.water_id = registry.get_id_by_string("cherry_log").unwrap_or(6);

        info!("[WorldGen] Initialized block IDs: grass={}, dirt={}, stone={}, bedrock={}, sand={}, water={}",
            self.grass_id, self.dirt_id, self.stone_id, self.bedrock_id, self.sand_id, self.water_id);
        info!("[WorldGen] Total blocks in registry: {}", registry.len());
        info!("[WorldGen] Using seed: {}", self.seed);
    }

    /// Calculates terrain height at a given world position
    /// The detail noise participates in local height variations
    #[inline]
    pub fn terrain_height(&self, wx: i32, wz: i32) -> i32 {
        // Main terrain noise - large scale features
        let terrain = self.terrain_noise.get([wx as f64 * 0.01, wz as f64 * 0.01]);

        // Detail noise - local variations (more visible now)
        let detail = self.detail_noise.get([wx as f64 * 0.08, wz as f64 * 0.08]) * 6.0;

        // Base height at Y=127, with amplitude of +/- 30 from terrain
        // Detail adds local bumps and dips
        let height = 127.0 + (terrain * 30.0) + detail;

        // Clamp to reasonable range [70, 180]
        height.clamp(70.0, 180.0) as i32
    }

    /// Gets the biome at a given world position
    #[inline]
    pub fn get_biome(&self, wx: i32, wz: i32) -> Biome {
        // River noise: higher values create rivers
        let river_val = self.river_noise.get([wx as f64 * 0.02, wz as f64 * 0.02]);

        // If river noise is above threshold, it's a river
        if river_val > 0.4 {
            return Biome::River;
        }

        // Biome noise - higher frequency for smaller biomes
        let n = self.biome_noise.get([wx as f64 * 0.02, wz as f64 * 0.02]);

        // Divide into biome zones
        if n < -0.33 {
            Biome::Desert
        } else if n < 0.33 {
            Biome::Plains
        } else {
            Biome::Mountains
        }
    }

    /// Returns the surface block ID for a given biome
    #[inline]
    fn surface_block_for_biome(&self, biome: Biome) -> GlobalVoxelId {
        match biome {
            Biome::Plains => self.grass_id,
            Biome::Desert => self.sand_id,
            Biome::Mountains => self.stone_id,
            Biome::River => self.dirt_id, // River bottom is dirt/sand
        }
    }

    /// Returns the subsurface block ID for a given biome (dirt layer)
    #[inline]
    fn subsurface_block_for_biome(&self, biome: Biome) -> GlobalVoxelId {
        match biome {
            Biome::Plains => self.dirt_id,
            Biome::Desert => self.sand_id,
            Biome::Mountains => self.stone_id,
            Biome::River => self.dirt_id,
        }
    }

    /// Checks if a position should be a cave (empty space)
    #[inline]
    fn is_cave(&self, wx: i32, wy: i32, wz: i32) -> bool {
        // Caves between Y=10 and Y=110 (below surface, above deep)
        if wy > 110 || wy < 10 {
            return false;
        }

        let n = self.cave_noise.get([wx as f64 * 0.05, wy as f64 * 0.05, wz as f64 * 0.05]);
        n > 0.5
    }

    /// Checks if a position is near the surface
    #[inline]
    fn is_near_surface(&self, _wx: i32, wy: i32, _wz: i32, surface_height: i32) -> bool {
        (surface_height - wy) < 5
    }

    /// Generates a complete chunk with detailed logging
    pub fn generate_chunk_logged(&self, chunk: &mut VoxelChunk, _registry: &SharedVoxelRegistry,
                                   cx: i32, cy: i32, cz: i32) -> ChunkGenStats {
        let start = std::time::Instant::now();

        let base_y = cy * CHUNK_SIZE as i32;

        if base_y > WORLD_HEIGHT as i32 {
            return ChunkGenStats::empty(start.elapsed());
        }

        // Pre-calculate heights and biomes for the entire chunk
        let mut heights = [[0i32; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];
        let mut biomes = [[Biome::Plains; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];
        let mut max_h = 0i32;

        let wx_base = cx * CHUNK_SIZE as i32;
        let wz_base = cz * CHUNK_SIZE as i32;

        for z in 0..CHUNK_SIZE as usize {
            for x in 0..CHUNK_SIZE as usize {
                let wx = wx_base + x as i32;
                let wz = wz_base + z as i32;
                let h = self.terrain_height(wx, wz);
                let biome = self.get_biome(wx, wz);
                heights[z][x] = h;
                biomes[z][x] = biome;
                if h > max_h {
                    max_h = h;
                }
            }
        }

        // Water level (below normal terrain height)
        let water_level = 100;

        if base_y > max_h && base_y > water_level {
            return ChunkGenStats::empty(start.elapsed());
        }

        let mut blocks_placed = 0u32;

        for z in 0..CHUNK_SIZE as usize {
            for x in 0..CHUNK_SIZE as usize {
                let wx = wx_base + x as i32;
                let wz = wz_base + z as i32;
                let h = heights[z][x];
                let biome = biomes[z][x];

                // Skip if below chunk and not in water area
                if h < base_y && base_y > water_level {
                    continue;
                }

                for y in 0..CHUNK_SIZE as usize {
                    let gy = base_y + y as i32;

                    // Above terrain and above water level = air
                    if gy > h && gy > water_level {
                        continue;
                    }

                    // Bedrock at Y=0
                    if gy == 0 {
                        chunk.set(x as u32, y as u32, z as u32, Some(self.bedrock_id));
                        blocks_placed += 1;
                        continue;
                    }

                    // Check for caves
                    if gy > 0 && !self.is_near_surface(wx, gy, wz, h) {
                        if self.is_cave(wx, gy, wz) {
                            continue;
                        }
                    }

                    // Water: fill below water_level and above terrain
                    if gy > h && gy <= water_level {
                        chunk.set(x as u32, y as u32, z as u32, Some(self.water_id));
                        blocks_placed += 1;
                        continue;
                    }

                    // Below terrain - place solid blocks
                    let block_id = if gy == h {
                        self.surface_block_for_biome(biome)
                    } else if gy > h - 4 {
                        self.subsurface_block_for_biome(biome)
                    } else {
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
