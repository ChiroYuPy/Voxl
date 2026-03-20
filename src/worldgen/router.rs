//! Noise Router - Route et combine les différentes densités
//!
//! Le NoiseRouter est responsable de:
//! - Calculer les paramètres climatiques (temperature, humidity, etc.)
//! - Calculer la densité finale du terrain

use serde::Deserialize;

use super::density::{DensityFunction, DensityContext};

/// Router de bruit - définit comment les différents bruits sont combinés
#[derive(Debug, Clone, Deserialize)]
pub struct NoiseRouter {
    // Paramètres climatiques pour les biomes
    pub temperature: DensityFunction,

    pub humidity: DensityFunction,

    pub continentalness: DensityFunction,

    pub erosion: DensityFunction,

    pub weirdness: DensityFunction,

    // Densité finale pour le terrain
    pub final_density: DensityFunction,
}

impl Default for NoiseRouter {
    fn default() -> Self {
        Self {
            temperature: DensityFunction::Constant(0.5),
            humidity: DensityFunction::Constant(0.5),
            continentalness: DensityFunction::Constant(0.5),
            erosion: DensityFunction::Constant(0.5),
            weirdness: DensityFunction::Constant(0.0),
            final_density: DensityFunction::Constant(0.0),
        }
    }
}

impl NoiseRouter {
    /// Crée un router depuis une chaîne RON
    pub fn from_ron(ron_str: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let result: Self = ron::from_str(ron_str)?;
        Ok(result)
    }

    /// Évalue les paramètres climatiques à une position
    pub fn sample_climate(&self, ctx: &mut DensityContext, x: f64, y: f64, z: f64) -> ClimateSample {
        ClimateSample {
            temperature: self.temperature.sample(x, y, z, ctx),
            humidity: self.humidity.sample(x, y, z, ctx),
            continentalness: self.continentalness.sample(x, y, z, ctx),
            erosion: self.erosion.sample(x, y, z, ctx),
            weirdness: self.weirdness.sample(x, y, z, ctx),
        }
    }

    /// Évalue la densité finale du terrain
    pub fn sample_density(&self, ctx: &mut DensityContext, x: f64, y: f64, z: f64) -> f64 {
        self.final_density.sample(x, y, z, ctx)
    }
}

/// Échantillon de paramètres climatiques
#[derive(Debug, Clone, Copy)]
pub struct ClimateSample {
    pub temperature: f64,
    pub humidity: f64,
    pub continentalness: f64,
    pub erosion: f64,
    pub weirdness: f64,
}
