//! Système de biomes - Multi-noise biome source
//!
//! Les biomes sont placés selon les paramètres climatiques calculés par le NoiseRouter.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

use super::router::ClimateSample;

/// Définition d'un biome
#[derive(Debug, Clone, Deserialize)]
pub struct Biome {
    /// ID unique du biome
    pub id: String,

    /// Température de base (0.0 = froid, 1.0 = chaud)
    #[serde(default)]
    pub temperature: f32,

    /// Humidité de base (0.0 = sec, 1.0 = humide)
    #[serde(default)]
    pub humidity: f32,

    /// Le biome a des précipitations (pluie/neige)
    #[serde(default = "default_true")]
    pub has_precipitation: bool,

    /// Features par couche de profondeur
    /// 0 = surface, 1 = underground top, 2 = deep underground, 3 = bedrock level
    #[serde(default)]
    pub features: Vec<Vec<FeatureEntry>>,

    /// Règles de surface (blocs à utiliser)
    #[serde(default)]
    pub surface: SurfaceRule,
}

fn default_true() -> bool { true }

/// Entrée de feature (nom et probabilité)
#[derive(Debug, Clone, Deserialize)]
pub struct FeatureEntry {
    /// Nom de la feature (ex: "trees_oak", "ore_coal")
    pub name: String,

    /// Probabilité/chance de spawn
    pub chance: f32,
}

/// Règles de surface pour un biome
#[derive(Debug, Clone, Deserialize, Default)]
pub struct SurfaceRule {
    /// Bloc de surface (ex: "grass_block", "sand", "snow_block")
    pub top_material: String,

    /// Bloc sous la surface (ex: "dirt", "sandstone", "stone")
    #[serde(default)]
    pub under_material: Option<String>,

    /// Profondeur de la sous-couche
    #[serde(default = "default_depth")]
    pub depth: u32,
}

fn default_depth() -> u32 { 4 }

/// Source multi-noise pour le placement des biomes
#[derive(Debug, Clone, Deserialize)]
pub struct MultiNoiseBiomeSource {
    /// Liste des entrées de biomes avec leurs paramètres climatiques
    pub biomes: Vec<BiomeEntry>,
}

/// Entrée de biome avec ses paramètres climatiques
#[derive(Debug, Clone, Deserialize)]
pub struct BiomeEntry {
    /// ID du biome
    pub biome: String,

    /// Plage de température
    #[serde(default)]
    pub temperature: ParameterRange,

    /// Plage d'humidité
    #[serde(default)]
    pub humidity: ParameterRange,

    /// Plage de continentalness
    #[serde(default)]
    pub continentalness: ParameterRange,

    /// Plage d'érosion
    #[serde(default)]
    pub erosion: ParameterRange,

    /// Plage de weirdness
    #[serde(default)]
    pub weirdness: ParameterRange,
}

/// Plage de paramètre (min/max)
#[derive(Debug, Clone, Deserialize)]
pub struct ParameterRange {
    #[serde(default = "param_min")]
    pub min: f32,

    #[serde(default = "param_max")]
    pub max: f32,
}

fn param_min() -> f32 { -2.0 }

fn param_max() -> f32 { 2.0 }

impl Default for ParameterRange {
    fn default() -> Self {
        Self {
            min: param_min(),
            max: param_max(),
        }
    }
}

impl MultiNoiseBiomeSource {
    /// Crée une source depuis une chaîne RON
    pub fn from_ron(ron_str: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let result: Self = ron::from_str(ron_str)?;
        Ok(result)
    }

    /// Trouve le biome correspondant aux paramètres climatiques
    pub fn get_biome(&self, climate: &ClimateSample) -> Option<&str> {
        let temp = climate.temperature as f32;
        let humidity = climate.humidity as f32;
        let cont = climate.continentalness as f32;
        let eros = climate.erosion as f32;
        let weird = climate.weirdness as f32;

        // Trouver le premier biome qui correspond
        for entry in &self.biomes {
            if temp >= entry.temperature.min && temp <= entry.temperature.max
                && humidity >= entry.humidity.min && humidity <= entry.humidity.max
                && cont >= entry.continentalness.min && cont <= entry.continentalness.max
                && eros >= entry.erosion.min && eros <= entry.erosion.max
                && weird >= entry.weirdness.min && weird <= entry.weirdness.max
            {
                return Some(entry.biome.as_str());
            }
        }
        None
    }
}

impl Default for MultiNoiseBiomeSource {
    fn default() -> Self {
        Self {
            biomes: vec![BiomeEntry {
                biome: "plains".to_string(),
                temperature: ParameterRange::default(),
                humidity: ParameterRange::default(),
                continentalness: ParameterRange::default(),
                erosion: ParameterRange::default(),
                weirdness: ParameterRange::default(),
            }]
        }
    }
}

/// Registre des biomes chargés
#[derive(Clone)]
pub struct BiomeRegistry {
    pub biomes: HashMap<String, Biome>,
}

impl BiomeRegistry {
    pub fn new() -> Self {
        Self {
            biomes: HashMap::new(),
        }
    }

    pub fn register(&mut self, biome: Biome) {
        self.biomes.insert(biome.id.clone(), biome);
    }

    pub fn get(&self, id: &str) -> Option<&Biome> {
        self.biomes.get(id)
    }

    /// Charge tous les biomes depuis un dossier
    pub fn load_from_directory(&mut self, path: &Path) -> Result<(), Box<dyn std::error::Error>> {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            if entry.path().extension().map_or(false, |e| e == "ron") {
                let content = std::fs::read_to_string(entry.path())?;
                let biome: Biome = ron::from_str(&content)?;
                self.register(biome);
            }
        }
        Ok(())
    }

    /// Retourne le nombre de biomes enregistrés
    pub fn len(&self) -> usize {
        self.biomes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.biomes.is_empty()
    }
}

impl Default for BiomeRegistry {
    fn default() -> Self {
        Self::new()
    }
}
