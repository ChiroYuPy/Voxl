//! Système de génération de monde data-driven
//!
//! Inspiré de Minecraft 1.18+, ce système utilise :
//! - Heightmap pour la forme du terrain
//! - Biomes pour la variation de surface
//! - Surface Rules pour les blocs

pub mod config;
pub mod density;
pub mod noise;
pub mod router;
pub mod biome;

use crate::voxel::{VoxelChunk, SharedVoxelRegistry, CHUNK_SIZE, GlobalVoxelId};
use config::WorldGenConfig;
use noise::NoiseGenerator;
use biome::{BiomeRegistry, MultiNoiseBiomeSource};
use std::collections::HashMap;
use std::sync::Arc;

/// Générateur de monde data-driven complet
#[derive(Clone)]
pub struct WorldGenerator {
    /// Configuration principale
    config: WorldGenConfig,

    /// Générateur de bruit
    noise: Arc<dyn NoiseGenerator + Send + Sync>,

    /// Source de biomes multi-noise
    biome_source: MultiNoiseBiomeSource,

    /// Registre des biomes chargés
    biomes: BiomeRegistry,

    /// Cache des IDs de blocs
    block_ids: HashMap<String, GlobalVoxelId>,
}

impl WorldGenerator {
    /// Crée un nouveau générateur depuis une configuration
    pub fn from_config(config: WorldGenConfig) -> Self {
        let seed = config.seed;
        let noise = noise::create_perlin_noise(seed);

        // Créer la source de biomes par défaut
        let biome_source = Self::create_biome_source();

        // Charger les biomes
        let mut biomes = BiomeRegistry::new();
        if let Err(e) = biomes.load_from_directory(std::path::Path::new(&config.biomes_path)) {
            eprintln!("[WorldGen] Failed to load biomes: {}", e);
        }

        Self {
            config,
            noise,
            biome_source,
            biomes,
            block_ids: HashMap::new(),
        }
    }

    /// Crée la source de biomes par défaut
    fn create_biome_source() -> MultiNoiseBiomeSource {
        use biome::{BiomeEntry, ParameterRange};

        MultiNoiseBiomeSource {
            biomes: vec![
                // Plaines - biome par défaut
                BiomeEntry {
                    biome: "plains".to_string(),
                    temperature: ParameterRange::default(),
                    humidity: ParameterRange::default(),
                    continentalness: ParameterRange::default(),
                    erosion: ParameterRange::default(),
                    weirdness: ParameterRange::default(),
                },
                // Désert - chaud et sec
                BiomeEntry {
                    biome: "desert".to_string(),
                    temperature: ParameterRange { min: 0.3, max: 2.0 },
                    humidity: ParameterRange { min: -2.0, max: -0.2 },
                    continentalness: ParameterRange::default(),
                    erosion: ParameterRange::default(),
                    weirdness: ParameterRange::default(),
                },
                // Forêt - tempéré et humide
                BiomeEntry {
                    biome: "forest".to_string(),
                    temperature: ParameterRange { min: 0.2, max: 0.8 },
                    humidity: ParameterRange { min: 0.3, max: 2.0 },
                    continentalness: ParameterRange::default(),
                    erosion: ParameterRange::default(),
                    weirdness: ParameterRange::default(),
                },
            ],
        }
    }

    /// Charge un générateur depuis un fichier RON
    pub fn load_from_file(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: WorldGenConfig = ron::from_str(&content)?;
        Ok(Self::from_config(config))
    }

    /// Initialise les IDs de blocs depuis le registry
    pub fn init_block_ids(&mut self, registry: &SharedVoxelRegistry) {
        self.block_ids.clear();

        // Blocs de base
        let base_blocks = [
            "air", "stone", "grass", "dirt", "bedrock",
            "water", "sand", "sandstone",
            "coal_ore", "iron_ore", "copper_ore", "gold_ore", "diamond_ore",
        ];

        for name in base_blocks {
            if let Some(vid) = registry.get_id_by_string(name) {
                self.block_ids.insert(name.to_string(), vid);
            }
        }

        // Blocs depuis les biomes
        for biome in self.biomes.biomes.values() {
            if let Some(vid) = registry.get_id_by_string(&biome.surface.top_material) {
                self.block_ids.insert(biome.surface.top_material.clone(), vid);
            }
            if let Some(under) = &biome.surface.under_material {
                if let Some(vid) = registry.get_id_by_string(under) {
                    self.block_ids.insert(under.clone(), vid);
                }
            }
        }
    }

    /// Retourne l'ID d'un bloc depuis son nom, ou None si c'est de l'air
    fn get_block_id(&self, name: &str) -> Option<GlobalVoxelId> {
        if name == "air" {
            return Some(0); // Air est toujours ID 0
        }
        self.block_ids.get(name).copied()
    }

    /// Calcule la hauteur du terrain à une position XZ
    fn get_terrain_height(&self, wx: f64, wz: f64) -> i32 {
        // Base noise pour la forme générale du terrain
        let base_noise = self.noise.sample_2d(1, 0.008, wx, wz);

        // Détail de terrain
        let detail_noise = self.noise.sample_2d(2, 0.02, wx, wz) * 0.5;

        let height = 70.0 + base_noise * 20.0 + detail_noise * 5.0;
        height.clamp(50.0, 120.0) as i32
    }

    /// Vérifie si une position est dans une grotte
    fn is_cave(&self, wx: f64, wy: i32, wz: f64) -> bool {
        // Pas de grottes près de la surface
        if wy < 15 || wy > 55 {
            return false;
        }

        // Noise 3D pour les grottes
        let cave_noise = self.noise.sample_3d(1000, 0.05, 0.05, 0.05, wx, wy as f64, wz);
        cave_noise > 0.4
    }

    /// Génère un chunk complet
    pub fn generate_chunk(
        &self,
        chunk: &mut VoxelChunk,
        cx: i32,
        cy: i32,
        cz: i32,
    ) {
        let base_y = cy * CHUNK_SIZE as i32;

        // Précalculer les hauteurs de terrain pour ce chunk
        let mut terrain_heights = [[0i32; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];
        let mut biome_ids = [["plains"; CHUNK_SIZE as usize]; CHUNK_SIZE as usize];

        for lx in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let wx = (cx * CHUNK_SIZE as i32 + lx as i32) as f64;
                let wz = (cz * CHUNK_SIZE as i32 + lz as i32) as f64;

                terrain_heights[lx as usize][lz as usize] = self.get_terrain_height(wx, wz);

                // Température simple basée sur Z
                let temp = ((wz * 0.01).sin() * 2.0) as f32;
                // Humidité simple basée sur X
                let humidity = ((wx * 0.01).cos() * 2.0) as f32;

                // Trouver le biome
                for entry in &self.biome_source.biomes {
                    if temp >= entry.temperature.min && temp <= entry.temperature.max
                        && humidity >= entry.humidity.min && humidity <= entry.humidity.max
                    {
                        biome_ids[lx as usize][lz as usize] = &entry.biome;
                        break;
                    }
                }
            }
        }

        // Générer les blocs
        for lx in 0..CHUNK_SIZE {
            for lz in 0..CHUNK_SIZE {
                let wx = (cx * CHUNK_SIZE as i32 + lx as i32) as f64;
                let wz = (cz * CHUNK_SIZE as i32 + lz as i32) as f64;
                let terrain_height = terrain_heights[lx as usize][lz as usize];
                let biome_id = biome_ids[lx as usize][lz as usize];

                for ly in 0..CHUNK_SIZE {
                    let wy = base_y + ly as i32;

                    // Bedrock à Y=0
                    if wy == 0 {
                        chunk.set(lx, ly, lz, self.get_block_id("bedrock"));
                        continue;
                    }

                    // Skip si au-dessus du terrain et au-dessus du niveau de la mer
                    if wy > terrain_height && wy > self.config.sea_level {
                        continue;
                    }

                    // Grottes ?
                    if self.is_cave(wx, wy, wz) {
                        continue; // Laisser vide (air)
                    }

                    let block = if wy > terrain_height {
                        // Au-dessus du terrain mais sous le niveau de la mer = eau
                        if wy <= self.config.sea_level {
                            self.get_block_id("water")
                        } else {
                            self.get_block_id("air")
                        }
                    } else {
                        // Sous le terrain - déterminer le bloc
                        let depth = terrain_height - wy;

                        if let Some(biome) = self.biomes.get(biome_id) {
                            // Couche de surface
                            if depth == 0 {
                                self.get_block_id(&biome.surface.top_material)
                            } else if depth < biome.surface.depth as i32 {
                                if let Some(under) = &biome.surface.under_material {
                                    self.get_block_id(under)
                                } else {
                                    self.get_block_id("dirt")
                                }
                            } else {
                                self.get_block_id("stone")
                            }
                        } else {
                            // Pas de biome - valeurs par défaut
                            if depth == 0 {
                                self.get_block_id("grass")
                            } else if depth < 4 {
                                self.get_block_id("dirt")
                            } else {
                                self.get_block_id("stone")
                            }
                        }
                    };

                    // Ne set que si ce n'est pas None (air ID 0)
                    if let Some(block_id) = block {
                        chunk.set(lx, ly, lz, Some(block_id));
                    }
                }
            }
        }
    }
}

/// Configuration par défaut pour le surmonde
impl Default for WorldGenerator {
    fn default() -> Self {
        Self::from_config(WorldGenConfig::default())
    }
}
