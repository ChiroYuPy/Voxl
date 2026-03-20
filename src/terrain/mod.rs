use crate::voxel::{VoxelChunk, VoxelWorld, SharedVoxelRegistry, CHUNK_SIZE, WORLD_HEIGHT, VERTICAL_CHUNKS, GlobalVoxelId};
use crate::worldgen::WorldGenerator;
use noise::{Perlin, Fbm, NoiseFn, MultiFractal};

/// IDs des blocs communs mis en cache pour éviter les lookup par string
#[derive(Debug, Clone, Copy)]
struct BlockIds {
    stone: GlobalVoxelId,
    grass: GlobalVoxelId,
    dirt: GlobalVoxelId,
    bedrock: GlobalVoxelId,
    coal_ore: GlobalVoxelId,
    iron_ore: GlobalVoxelId,
    copper_ore: GlobalVoxelId,
    gold_ore: GlobalVoxelId,
    lapis_ore: GlobalVoxelId,
    redstone_ore: GlobalVoxelId,
    diamond_ore: GlobalVoxelId,
    emerald_ore: GlobalVoxelId,
}

/// Mode de génération du terrain
#[derive(Debug, Clone, Copy)]
pub enum TerrainGenMode {
    /// Utilise le générateur legacy (hardcodé)
    Legacy,
    /// Utilise le générateur data-driven depuis un fichier RON
    DataDriven(&'static str),
    /// Utilise la config data-driven par défaut
    DefaultData,
}

/// Générateur de terrain
pub struct TerrainGenerator {
    terrain_noise: Fbm<Perlin>,
    cave_noise: Perlin,
    ore_noise: Perlin,
    _seed: u32,
    block_ids: Option<BlockIds>,
    /// Générateur data-driven (optionnel)
    worldgen: Option<WorldGenerator>,
}

impl TerrainGenerator {
    pub fn new() -> Self {
        Self::with_mode(TerrainGenMode::Legacy)
    }

    /// Crée un générateur avec un mode spécifique
    pub fn with_mode(mode: TerrainGenMode) -> Self {
        let seed = 12345u32;
        let worldgen = match mode {
            TerrainGenMode::Legacy => None,
            TerrainGenMode::DataDriven(path) => {
                println!("[WorldGen] Loading config from: {}", path);
                match WorldGenerator::load_from_file(path) {
                    Ok(generator) => {
                        println!("[WorldGen] Config loaded successfully");
                        Some(generator)
                    }
                    Err(e) => {
                        eprintln!("[WorldGen] Failed to load config: {}, using legacy", e);
                        None
                    }
                }
            }
            TerrainGenMode::DefaultData => {
                println!("[WorldGen] Using default data-driven config");
                Some(WorldGenerator::default())
            }
        };

        Self {
            terrain_noise: Fbm::new(seed)
                .set_octaves(4)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            cave_noise: Perlin::new(seed + 100),
            ore_noise: Perlin::new(seed + 200),
            _seed: seed,
            block_ids: None,
            worldgen,
        }
    }

    /// Indique si ce générateur utilise le système data-driven
    pub fn is_data_driven(&self) -> bool {
        self.worldgen.is_some()
    }

    /// Initialise les IDs de blocs depuis le registry (à appeler une fois au démarrage)
    fn init_block_ids(&mut self, registry: &SharedVoxelRegistry) {
        if self.block_ids.is_some() {
            return;
        }

        let get_id = |name: &str| -> GlobalVoxelId {
            registry.get_id_by_string(name).unwrap_or(0)
        };

        self.block_ids = Some(BlockIds {
            stone: get_id("stone"),
            grass: get_id("grass"),
            dirt: get_id("dirt"),
            bedrock: get_id("bedrock"),
            coal_ore: get_id("coal_ore"),
            iron_ore: get_id("iron_ore"),
            copper_ore: get_id("copper_ore"),
            gold_ore: get_id("gold_ore"),
            lapis_ore: get_id("lapis_ore"),
            redstone_ore: get_id("redstone_ore"),
            diamond_ore: get_id("diamond_ore"),
            emerald_ore: get_id("emerald_ore"),
        });

        // Initialiser aussi le worldgen data-driven
        if let Some(ref mut worldgen) = self.worldgen {
            worldgen.init_block_ids(registry);
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

    fn get_ore(&self, n: f64, depth: i32) -> Option<GlobalVoxelId> {
        let ids = self.block_ids.as_ref()?;

        // Charbon: Y=5-60
        if depth >= 5 && depth <= 60 && n > 0.94 {
            return Some(ids.coal_ore);
        }
        // Fer: Y=5-40
        if depth >= 5 && depth <= 40 && n > 0.96 {
            return Some(ids.iron_ore);
        }
        // Cuivre: Y=5-50
        if depth >= 5 && depth <= 50 && n > 0.965 {
            return Some(ids.copper_ore);
        }
        // Or: Y=10-30
        if depth >= 10 && depth <= 30 && n > 0.975 {
            return Some(ids.gold_ore);
        }
        // Lapis: Y=5-25
        if depth >= 5 && depth <= 25 && n > 0.975 {
            return Some(ids.lapis_ore);
        }
        // Redstone: Y=5+
        if depth >= 5 && n > 0.97 {
            return Some(ids.redstone_ore);
        }
        // Diamant: Y=5-20
        if depth >= 5 && depth <= 20 && n > 0.985 {
            return Some(ids.diamond_ore);
        }
        // Émeraude: Y=5-30
        if depth >= 5 && depth <= 30 && n > 0.988 {
            return Some(ids.emerald_ore);
        }
        None
    }

    pub fn generate_chunk(&self, chunk: &mut VoxelChunk, _registry: &SharedVoxelRegistry,
                          cx: i32, cy: i32, cz: i32) {
        let ids = self.block_ids.expect("Block IDs not initialized! Call init_block_ids first.");

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
                        chunk.set(x, y, z, Some(ids.bedrock));
                        continue;
                    }

                    if self.is_cave(wx, wy, wz) {
                        continue;
                    }

                    let ore_noise = self.ore_noise.get([wx * 0.03, wy * 0.03, wz * 0.03]);
                    let block = if gy == h {
                        ids.grass
                    } else if gy >= h - 3 {
                        ids.dirt
                    } else {
                        self.get_ore(ore_noise, gy).unwrap_or(ids.stone)
                    };

                    chunk.set(x, y, z, Some(block));
                }
            }
        }
    }

    pub fn generate_test_world(&self, world: &mut VoxelWorld) {
        let registry = world.registry().clone();

        // Utiliser le générateur data-driven si disponible
        if self.worldgen.is_some() {
            println!("[WorldGen] Generating world with data-driven system...");
            // Pour le moment on utilise un nouveau générateur avec la config par défaut
            // car on ne peut pas cloner le worldgen (Arc<dyn NoiseGenerator> n'est pas Clone)
            // Une solution propre serait d'avoir un generate_chunk qui prend registry
            let mut wg_temp = WorldGenerator::default();
            wg_temp.init_block_ids(&registry);

            for cx in -3..=3 {
                for cz in -3..=3 {
                    for cy in 0..VERTICAL_CHUNKS as i32 {
                        let chunk = world.get_or_create_chunk(cx, cy, cz);
                        wg_temp.generate_chunk(chunk, cx, cy, cz);
                        println!("[WorldGen] Generated chunk ({}, {}, {})", cx, cy, cz);
                    }
                }
            }
            return;
        }

        // Sinon utiliser le générateur legacy
        println!("[WorldGen] Generating world with legacy system...");
        let mut gen_temp = Self {
            terrain_noise: self.terrain_noise.clone(),
            cave_noise: self.cave_noise.clone(),
            ore_noise: self.ore_noise.clone(),
            _seed: self._seed,
            block_ids: None,
            worldgen: None,
        };
        gen_temp.init_block_ids(&registry);

        for cx in -5..=4 {
            for cz in -5..=4 {
                for cy in 0..VERTICAL_CHUNKS as i32 {
                    let chunk = world.get_or_create_chunk(cx, cy, cz);
                    gen_temp.generate_chunk(chunk, &registry, cx, cy, cz);
                }
            }
        }
    }

    pub fn fill_solid(&self, chunk: &mut VoxelChunk, height: u32, registry: &SharedVoxelRegistry) {
        // Pour fill_solid, on fait le lookup car c'est rarement utilisé
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
