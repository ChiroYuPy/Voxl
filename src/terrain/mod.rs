use crate::voxel::{VoxelChunk, VoxelWorld, SharedVoxelRegistry, CHUNK_SIZE, WORLD_HEIGHT, VERTICAL_CHUNKS};
use noise::{Perlin, OpenSimplex, Fbm, NoiseFn, MultiFractal};

/// Types de biomes
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Biome {
    Plains,     // Plaine - herbe, arbres épars, plat
    Forest,     // Forêt - herbe, beaucoup d'arbres
    Desert,     // Désert - sable, cactus, pas d'arbres
    Mountains,  // Montagne - pierre, neige en haut, pics
    Snow,       // Toundra - neige, herbe froide, sapins
}

impl Biome {
    /// Retourne le biome basé sur la température et l'humidité
    pub fn from_temp_humidity(temp: f64, humidity: f64) -> Self {
        // Température: -1.0 (froid) à 1.0 (chaud)
        // Humidité: -1.0 (sec) à 1.0 (humide)

        if temp < -0.3 {
            Biome::Snow  // Toundra
        } else if temp > 0.5 && humidity < 0.0 {
            Biome::Desert  // Désert: chaud et sec
        } else if temp > 0.3 {
            if humidity > 0.3 {
                Biome::Forest  // Forêt: chaud et humide
            } else {
                Biome::Plains  // Plaine: chaud et modéré
            }
        } else {
            if humidity < -0.2 {
                Biome::Mountains  // Montagne: tempéré et sec
            } else if humidity > 0.2 {
                Biome::Forest  // Forêt de montagne
            } else {
                Biome::Plains
            }
        }
    }

    /// Hauteur de base pour ce biome
    pub fn base_height(&self) -> f64 {
        match self {
            Biome::Plains => 32.0,
            Biome::Forest => 35.0,
            Biome::Desert => 30.0,
            Biome::Mountains => 45.0,
            Biome::Snow => 40.0,
        }
    }

    /// Variation de hauteur pour ce biome
    pub fn height_variation(&self) -> f64 {
        match self {
            Biome::Plains => 8.0,
            Biome::Forest => 12.0,
            Biome::Desert => 5.0,
            Biome::Mountains => 30.0,
            Biome::Snow => 15.0,
        }
    }

    /// Bloc de surface pour ce biome
    pub fn surface_block(&self, registry: &SharedVoxelRegistry) -> Option<usize> {
        match self {
            Biome::Plains => registry.get_id_by_string("grass"),
            Biome::Forest => registry.get_id_by_string("grass"),
            Biome::Desert => registry.get_id_by_string("sand"),
            Biome::Mountains => registry.get_id_by_string("stone"),
            Biome::Snow => registry.get_id_by_string("stone"), // Sera remplacé par snow
        }
    }

    /// Bloc sous la surface pour ce biome
    pub fn subsurface_block(&self, registry: &SharedVoxelRegistry) -> Option<usize> {
        match self {
            Biome::Plains => registry.get_id_by_string("dirt"),
            Biome::Forest => registry.get_id_by_string("dirt"),
            Biome::Desert => registry.get_id_by_string("sand"),
            Biome::Mountains => registry.get_id_by_string("stone"),
            Biome::Snow => registry.get_id_by_string("dirt"),
        }
    }

    /// Chance d'avoir un arbre par colonne (0.0 à 1.0)
    pub fn tree_chance(&self) -> f64 {
        match self {
            Biome::Plains => 0.015,    // Quelques arbres dans les plaines
            Biome::Forest => 0.08,     // Plus en forêt
            Biome::Desert => 0.0,      // Pas d'arbres dans le désert
            Biome::Mountains => 0.01,  // Rares en montagne
            Biome::Snow => 0.02,       // Quelques arbres dans la neige
        }
    }
}

/// Générateur de terrain amélioré avec biomes et bruit
pub struct TerrainGenerator {
    // Noise pour le terrain (hauteur)
    terrain_noise: Fbm<Perlin>,
    // Noise pour la température
    temp_noise: Fbm<OpenSimplex>,
    // Noise pour l'humidité
    humidity_noise: Fbm<OpenSimplex>,
    // Noise pour les grottes (3D)
    cave_noise: Fbm<Perlin>,
    // Noise pour les structures (arbres, minerai)
    structure_noise: Perlin,
    // Seed
    seed: u32,
}

impl TerrainGenerator {
    pub fn new() -> Self {
        let seed = 12345u32;

        Self {
            terrain_noise: Fbm::new(seed + 1)
                .set_octaves(4)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            temp_noise: Fbm::new(seed + 10)
                .set_octaves(2)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            humidity_noise: Fbm::new(seed + 20)
                .set_octaves(2)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            cave_noise: Fbm::new(seed + 100)
                .set_octaves(3)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            structure_noise: Perlin::new(seed + 200),
            seed,
        }
    }

    pub fn with_seed(seed: u32) -> Self {
        Self {
            terrain_noise: Fbm::new(seed + 1)
                .set_octaves(4)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            temp_noise: Fbm::new(seed + 10)
                .set_octaves(2)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            humidity_noise: Fbm::new(seed + 20)
                .set_octaves(2)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            cave_noise: Fbm::new(seed + 100)
                .set_octaves(3)
                .set_persistence(0.5)
                .set_lacunarity(2.0),
            structure_noise: Perlin::new(seed + 200),
            seed,
        }
    }

    /// Obtenir le biome à une position mondiale
    fn get_biome(&self, wx: f64, wz: f64) -> Biome {
        let scale = 0.003;
        let temp = self.temp_noise.get([wx * scale, wz * scale]);
        let humidity = self.humidity_noise.get([wx * scale * 0.7 + 1000.0, wz * scale * 0.7 + 1000.0]);
        Biome::from_temp_humidity(temp, humidity)
    }

    /// Obtenir la hauteur du terrain à une position mondiale
    fn get_terrain_height(&self, wx: f64, wz: f64) -> f64 {
        let biome = self.get_biome(wx, wz);
        let base = biome.base_height();
        let variation = biome.height_variation();

        // Terrain principal avec grandes ondulations
        let scale1 = 0.005;
        let noise1 = self.terrain_noise.get([wx * scale1, wz * scale1]);

        // Détails plus fins
        let scale2 = 0.02;
        let noise2 = self.terrain_noise.get([wx * scale2 + 500.0, wz * scale2 + 500.0]) * 0.3;

        // Encore plus de détails
        let scale3 = 0.08;
        let noise3 = self.terrain_noise.get([wx * scale3 + 1000.0, wz * scale3 + 1000.0]) * 0.1;

        base + (noise1 + noise2 + noise3) * variation
    }

    /// Vérifier si une position est dans une grotte
    fn is_in_cave(&self, wx: f64, wy: f64, wz: f64) -> bool {
        // Pas de grottes près de la surface
        if wy < 5.0 || wy > 50.0 {
            return false;
        }

        let scale = 0.04;
        let cave = self.cave_noise.get([wx * scale, wy * scale, wz * scale]);

        // Seuil pour créer des grottes (valeurs positives = vide)
        cave > 0.4
    }

    /// Vérifier si on peut placer un arbre à cette position
    fn can_place_tree(&self, wx: f64, wz: f64) -> bool {
        let tree_noise = self.structure_noise.get([wx * 0.5, wz * 0.5]);
        tree_noise > 0.3
    }

    /// Génère les minerais selon la profondeur
    fn get_ore_at_depth(&self, wx: f64, wy: f64, wz: f64, registry: &SharedVoxelRegistry) -> Option<usize> {
        // Utiliser un bruit différent pour les minerais
        let ore_noise = self.structure_noise.get([wx * 0.1, wy * 0.1, wz * 0.1]);

        // Probabilités selon la profondeur
        let depth = wy as u32;

        // Charbon : commun, de 5 à 60
        if depth >= 5 && depth <= 60 {
            let coal_chance = 0.08 - (depth as f64 / 1000.0);
            if ore_noise > (1.0 - coal_chance) {
                return registry.get_id_by_string("coal_ore");
            }
        }

        // Fer : de 5 à 40
        if depth >= 5 && depth <= 40 {
            let iron_chance = 0.05 - (depth as f64 / 1500.0);
            let iron_noise = self.structure_noise.get([wx * 0.15, wy * 0.15, wz * 0.15]);
            if iron_noise > (1.0 - iron_chance) {
                return registry.get_id_by_string("iron_ore");
            }
        }

        // Or : rare, de 10 à 30
        if depth >= 10 && depth <= 30 {
            let gold_chance = 0.02;
            let gold_noise = self.structure_noise.get([wx * 0.2, wy * 0.2, wz * 0.2]);
            if gold_noise > (1.0 - gold_chance) {
                return registry.get_id_by_string("gold_ore");
            }
        }

        // Diamant : très rare, de 5 à 20
        if depth >= 5 && depth <= 20 {
            let diamond_chance = 0.008;
            let diamond_noise = self.structure_noise.get([wx * 0.25, wy * 0.25, wz * 0.25]);
            if diamond_noise > (1.0 - diamond_chance) {
                return registry.get_id_by_string("diamond_ore");
            }
        }

        None
    }

    /// Génère un arbre à la position donnée (dans le chunk)
    fn generate_tree(&self, chunk: &mut VoxelChunk, registry: &SharedVoxelRegistry,
                     lx: u32, ly: u32, lz: u32, _biome: Biome) {
        // IDs des blocs
        let log_id = registry.get_id_by_string("oak_log");
        let leaves_id = registry.get_id_by_string("oak_leaves");

        let (Some(log_id), Some(leaves_id)) = (log_id, leaves_id) else { return };

        // Hauteur de l'arbre (5 à 7 blocs)
        let height = 5 + (self.structure_noise.get([lx as f64, lz as f64]).abs() * 2.0) as u32;

        // Tronc
        for dy in 0..height {
            if ly + dy < CHUNK_SIZE {
                chunk.set(lx, ly + dy, lz, Some(log_id));
            }
        }

        // Feuillage - forme plus naturelle
        let top = ly + height;
        let leaf_radius = 2 + (height / 3).min(2);

        for dy in -2i32..=1 {
            for dx in -(leaf_radius as i32)..=leaf_radius as i32 {
                for dz in -(leaf_radius as i32)..=leaf_radius as i32 {
                    let nx = lx as i32 + dx;
                    let ny = top as i32 + dy;
                    let nz = lz as i32 + dz;

                    if nx >= 0 && nx < CHUNK_SIZE as i32 &&
                       ny >= 0 && ny < CHUNK_SIZE as i32 &&
                       nz >= 0 && nz < CHUNK_SIZE as i32 {
                        // Distance pour forme arrondie
                        let dist = ((dx * dx + dz * dz) as f64).sqrt();
                        let max_dist = leaf_radius as f64;

                        // Plus large en bas, plus étroit en haut
                        let layer_radius = if dy < 0 {
                            max_dist * 1.2
                        } else if dy == 0 {
                            max_dist
                        } else {
                            max_dist * 0.6
                        };

                        if dist < layer_radius && (dx != 0 || dz != 0 || dy >= 0) {
                            chunk.set(nx as u32, ny as u32, nz as u32, Some(leaves_id));
                        }
                    }
                }
            }
        }

        // Bloc de feuillage tout en haut
        if top + 1 < CHUNK_SIZE {
            chunk.set(lx, top + 1, lz, Some(leaves_id));
        }
    }

    /// Génère le terrain d'un chunk
    pub fn generate_chunk(&self, chunk: &mut VoxelChunk, registry: &SharedVoxelRegistry,
                          cx: i32, cy: i32, cz: i32) {
        // Récupérer les ids globaux
        let stone_id = registry.get_id_by_string("stone").unwrap_or(0);
        let bedrock_id = registry.get_id_by_string("bedrock").unwrap_or(0);

        let chunk_base_y = cy * CHUNK_SIZE as i32;
        let chunk_top_y = (chunk_base_y + CHUNK_SIZE as i32).min(WORLD_HEIGHT as i32);

        // Pour stocker les positions d'arbres à générer (après le terrain)
        let mut tree_positions: Vec<(u32, u32, u32, Biome)> = Vec::new();

        // Grille pour espacer les arbres (éviter les arbres collés)
        // Chaque cellule de 4x4 peut avoir au plus 1 arbre
        let mut tree_grid = [[false; 4]; 4];

        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                let wx = (cx * CHUNK_SIZE as i32 + x as i32) as f64;
                let wz = (cz * CHUNK_SIZE as i32 + z as i32) as f64;

                // Obtenir le biome
                let biome = self.get_biome(wx, wz);

                // Obtenir la hauteur du terrain
                let terrain_height = self.get_terrain_height(wx, wz);
                let terrain_height = terrain_height.clamp(5.0, WORLD_HEIGHT as f64) as u32;

                // Calculer les limites Y pour ce chunk
                let y_start = 0u32;
                let y_end = if terrain_height >= chunk_top_y as u32 {
                    CHUNK_SIZE
                } else if terrain_height > chunk_base_y as u32 {
                    terrain_height - chunk_base_y as u32
                } else {
                    0
                };

                for y in y_start..y_end {
                    let global_y = chunk_base_y + y as i32;
                    let wy = global_y as f64;

                    // Vérifier les grottes
                    if self.is_in_cave(wx, wy, wz) {
                        continue; // Laisser vide (grotte)
                    }

                    let block_id = if global_y == 0 {
                        Some(bedrock_id)
                    } else if global_y as u32 >= terrain_height {
                        // Au-dessus du terrain - vide
                        continue;
                    } else if global_y as u32 == terrain_height - 1 {
                        // Surface
                        biome.surface_block(registry)
                    } else if global_y as u32 >= terrain_height - 4 {
                        // Juste sous la surface
                        biome.subsurface_block(registry)
                    } else {
                        // Profondeur - vérifier les minerais d'abord
                        if let Some(ore_id) = self.get_ore_at_depth(wx, wy, wz, registry) {
                            Some(ore_id)
                        } else {
                            Some(stone_id)
                        }
                    };

                    chunk.set(x, y, z, block_id);
                }

                // Vérifier si on doit placer un arbre
                // Uniquement à la surface du terrain
                if terrain_height >= chunk_base_y as u32 && terrain_height < chunk_top_y as u32 {
                    let surface_y = terrain_height - chunk_base_y as u32;

                    // Vérifier qu'il y a un bloc solide sous la surface (pas de grotte)
                    let block_below = if surface_y > 0 {
                        chunk.get(x, surface_y - 1, z)
                    } else {
                        None
                    };

                    // Si le bloc en dessous existe (pas vide/grotte), on peut placer un arbre
                    if block_below.is_some() && surface_y < CHUNK_SIZE - 6 && biome.tree_chance() > 0.0 {
                        let tree_roll = self.structure_noise.get([wx * 0.5, wz * 0.5]);
                        if tree_roll > 0.7 { // Environ 30% des colonnes éligibles
                            // Vérifier l'espacement avec les autres arbres (grille 4x4)
                            let grid_x = (x / 4) as usize;
                            let grid_z = (z / 4) as usize;
                            if !tree_grid[grid_x][grid_z] {
                                tree_grid[grid_x][grid_z] = true;
                                tree_positions.push((x, surface_y, z, biome));
                            }
                        }
                    }
                }
            }
        }

        // Générer les arbres (après avoir rempli le terrain)
        for (x, y, z, biome) in tree_positions {
            // Générer même si ça dépasse le chunk (partie visible sera générée)
            self.generate_tree(chunk, registry, x, y + 1, z, biome);
        }
    }

    /// Génère un monde de test
    pub fn generate_test_world(&self, world: &mut VoxelWorld) {
        let registry = world.registry().clone();

        // Générer 10x10 chunks
        for cx in -5..=4 {
            for cz in -5..=4 {
                for cy in 0..VERTICAL_CHUNKS as i32 {
                    let chunk = world.get_or_create_chunk(cx, cy, cz);
                    self.generate_chunk(chunk, &registry, cx, cy, cz);
                }
            }
        }
    }

    /// Remplit un chunk de solide jusqu'à une hauteur
    pub fn fill_solid(&self, chunk: &mut VoxelChunk, height: u32, registry: &SharedVoxelRegistry) {
        let stone_id = registry.get_id_by_string("stone").unwrap_or(0);
        for x in 0..CHUNK_SIZE {
            for z in 0..CHUNK_SIZE {
                for y in 0..height.min(CHUNK_SIZE) {
                    chunk.set(x, y, z, Some(stone_id));
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
