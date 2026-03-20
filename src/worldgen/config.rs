//! Structures de configuration pour la génération de monde data-driven

use serde::Deserialize;

/// Configuration principale de génération de monde
#[derive(Debug, Clone, Deserialize)]
pub struct WorldGenConfig {
    /// Seed pour la génération aléatoire
    pub seed: u64,

    /// Dimensions du monde
    #[serde(default = "default_height")]
    pub height: u32,

    /// Niveau de la mer
    #[serde(default = "default_sea_level")]
    pub sea_level: i32,

    /// Chemin vers le fichier de configuration du router
    #[serde(default = "default_noise_router_path")]
    pub noise_router_path: String,

    /// Chemin vers le fichier de configuration de la source de biomes
    #[serde(default = "default_biome_source_path")]
    pub biome_source_path: String,

    /// Chemin vers le dossier contenant les définitions de biomes
    #[serde(default = "default_biomes_path")]
    pub biomes_path: String,

    /// Chemin vers le fichier de fonctions de densité
    #[serde(default = "default_density_functions_path")]
    pub density_functions_path: String,
}

// Fonctions par défaut pour les valeurs

fn default_height() -> u32 { 256 }

fn default_sea_level() -> i32 { 62 }

fn default_noise_router_path() -> String {
    "data/noise_settings/overworld_router.ron".to_string()
}

fn default_biome_source_path() -> String {
    "data/biome_source.ron".to_string()
}

fn default_biomes_path() -> String {
    "data/biomes".to_string()
}

fn default_density_functions_path() -> String {
    "data/density_functions/noise.ron".to_string()
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self {
            seed: 12345,
            height: default_height(),
            sea_level: default_sea_level(),
            noise_router_path: default_noise_router_path(),
            biome_source_path: default_biome_source_path(),
            biomes_path: default_biomes_path(),
            density_functions_path: default_density_functions_path(),
        }
    }
}
