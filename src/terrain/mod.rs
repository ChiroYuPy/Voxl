use crate::voxel::{VoxelChunk, VoxelWorld, SharedVoxelRegistry, CHUNK_SIZE, WORLD_HEIGHT, VERTICAL_CHUNKS};
use noise::{Perlin, Fbm, NoiseFn, MultiFractal};

/// Générateur de terrain
pub struct TerrainGenerator {
    terrain_noise: Fbm<Perlin>,
    cave_noise: Perlin,
    ore_noise: Perlin,
    _seed: u32,
}

impl TerrainGenerator {
    pub fn new() -> Self {
        let seed = 12345u32;
        Self {
            terrain_noise: Fbm::new(seed)
                .set_octaves(4)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            cave_noise: Perlin::new(seed + 100),
            ore_noise: Perlin::new(seed + 200),
            _seed: seed,
        }
    }

    fn terrain_height(&self, wx: f64, wz: f64) -> i32 {
        let n = self.terrain_noise.get([wx * 0.008, wz * 0.008]);
        (128.0 + n * 40.0).clamp(10.0, WORLD_HEIGHT as f64 - 5.0) as i32
    }

    fn is_cave(&self, wx: f64, wy: f64, wz: f64) -> bool {
        if wy < 15.0 || wy > (WORLD_HEIGHT - 3) as f64 {
            return false;
        }
        self.cave_noise.get([wx * 0.04, wy * 0.04, wz * 0.04]) > 0.4
    }

    fn get_ore(&self, n: f64, depth: i32, registry: &SharedVoxelRegistry) -> Option<usize> {
        // Charbon: Y=5-60
        if depth >= 5 && depth <= 60 && n > 0.94 {
            return registry.get_id_by_string("coal_ore");
        }
        // Fer: Y=5-40
        if depth >= 5 && depth <= 40 && n > 0.96 {
            return registry.get_id_by_string("iron_ore");
        }
        // Cuivre: Y=5-50
        if depth >= 5 && depth <= 50 && n > 0.965 {
            return registry.get_id_by_string("copper_ore");
        }
        // Or: Y=10-30
        if depth >= 10 && depth <= 30 && n > 0.975 {
            return registry.get_id_by_string("gold_ore");
        }
        // Lapis: Y=5-25
        if depth >= 5 && depth <= 25 && n > 0.975 {
            return registry.get_id_by_string("lapis_ore");
        }
        // Redstone: Y=5+
        if depth >= 5 && n > 0.97 {
            return registry.get_id_by_string("redstone_ore");
        }
        // Diamant: Y=5-20
        if depth >= 5 && depth <= 20 && n > 0.985 {
            return registry.get_id_by_string("diamond_ore");
        }
        // Émeraude: Y=5-30
        if depth >= 5 && depth <= 30 && n > 0.988 {
            return registry.get_id_by_string("emerald_ore");
        }
        None
    }

    pub fn generate_chunk(&self, chunk: &mut VoxelChunk, registry: &SharedVoxelRegistry,
                          cx: i32, cy: i32, cz: i32) {
        let stone = registry.get_id_by_string("stone").unwrap_or(0);
        let grass = registry.get_id_by_string("grass").unwrap_or(1);
        let dirt = registry.get_id_by_string("dirt").unwrap_or(2);
        let bedrock = registry.get_id_by_string("bedrock").unwrap_or(3);

        let base_y = cy * CHUNK_SIZE as i32;

        // Chunk au-dessus du terrain max? Skip
        if base_y > (WORLD_HEIGHT - 5) as i32 {
            return;
        }

        // Calculer toutes les hauteurs du chunk
        let mut heights = [0i32; CHUNK_SIZE as usize * CHUNK_SIZE as usize];
        let mut max_h = 0i32;
        let mut min_h = WORLD_HEIGHT as i32;
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let wx = (cx * CHUNK_SIZE as i32 + x as i32) as f64;
                let wz = (cz * CHUNK_SIZE as i32 + z as i32) as f64;
                let h = self.terrain_height(wx, wz);
                heights[(z * CHUNK_SIZE + x) as usize] = h;
                max_h = max_h.max(h);
                min_h = min_h.min(h);
            }
        }

        // Chunk complètement vide? Skip
        if base_y > max_h {
            return;
        }

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let h = heights[(z * CHUNK_SIZE + x) as usize];
                let wx = (cx * CHUNK_SIZE as i32 + x as i32) as f64;
                let wz = (cz * CHUNK_SIZE as i32 + z as i32) as f64;

                // Colonne vide dans ce chunk
                if h < base_y {
                    continue;
                }

                for y in 0..CHUNK_SIZE {
                    let gy = base_y + y as i32;

                    if gy > h {
                        continue;
                    }

                    let wy = gy as f64;

                    if gy == 0 {
                        chunk.set(x, y, z, Some(bedrock));
                        continue;
                    }

                    if self.is_cave(wx, wy, wz) {
                        continue;
                    }

                    let ore_noise = self.ore_noise.get([wx * 0.03, wy * 0.03, wz * 0.03]);
                    let block = if gy == h {
                        grass
                    } else if gy >= h - 3 {
                        dirt
                    } else {
                        self.get_ore(ore_noise, gy, registry).unwrap_or(stone)
                    };

                    chunk.set(x, y, z, Some(block));
                }
            }
        }
    }

    pub fn generate_test_world(&self, world: &mut VoxelWorld) {
        let registry = world.registry().clone();
        for cx in -5..=4 {
            for cz in -5..=4 {
                for cy in 0..VERTICAL_CHUNKS as i32 {
                    let chunk = world.get_or_create_chunk(cx, cy, cz);
                    self.generate_chunk(chunk, &registry, cx, cy, cz);
                }
            }
        }
    }

    pub fn fill_solid(&self, chunk: &mut VoxelChunk, height: u32, registry: &SharedVoxelRegistry) {
        let stone = registry.get_id_by_string("stone").unwrap_or(0);
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 0..height.min(CHUNK_SIZE) {
                    chunk.set(x, y, z, Some(stone));
                }
            }
        }
    }
}

impl Default for TerrainGenerator {
    fn default() -> Self {
        Self::new()
    }
}
