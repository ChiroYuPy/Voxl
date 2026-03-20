//! Wrapper autour de la crate `noise` pour le générateur de bruit

use noise::{Perlin, Fbm, NoiseFn, MultiFractal};

/// Générateur de bruit (abstraction sur la crate noise)
pub trait NoiseGenerator: Send + Sync {
    fn sample_2d(&self, seed: u64, frequency: f32, x: f64, z: f64) -> f64;
    fn sample_3d(&self, seed: u64, freq_x: f32, freq_y: f32, freq_z: f32, x: f64, y: f64, z: f64) -> f64;
}

/// Implémentation utilisant Perlin + FBM pour plus de détail
pub struct PerlinNoiseGenerator {
    base_seed: u64,
}

impl PerlinNoiseGenerator {
    pub fn new(seed: u64) -> Self {
        Self { base_seed: seed }
    }

    fn get_fbm(&self, seed_offset: u64) -> Fbm<Perlin> {
        Fbm::new((self.base_seed + seed_offset) as u32)
            .set_octaves(4)
            .set_persistence(0.5)
            .set_lacunarity(2.0)
    }
}

impl NoiseGenerator for PerlinNoiseGenerator {
    fn sample_2d(&self, seed: u64, frequency: f32, x: f64, z: f64) -> f64 {
        let perlin = self.get_fbm(seed);
        perlin.get([x * frequency as f64, z * frequency as f64])
    }

    fn sample_3d(&self, seed: u64, freq_x: f32, freq_y: f32, freq_z: f32, x: f64, y: f64, z: f64) -> f64 {
        let perlin = self.get_fbm(seed);
        perlin.get([
            x * freq_x as f64,
            y * freq_y as f64,
            z * freq_z as f64,
        ])
    }
}

/// Crée un nouveau générateur de bruit Perlin (version simple sans FBM pour la performance)
pub fn create_perlin_noise(seed: u64) -> std::sync::Arc<dyn NoiseGenerator + Send + Sync> {
    std::sync::Arc::new(SimplePerlinGenerator::new(seed))
}

/// Implémentation simple utilisant directement Perlin (sans FBM)
pub struct SimplePerlinGenerator {
    _seed: u64,
    perlin: Perlin,
}

impl SimplePerlinGenerator {
    pub fn new(seed: u64) -> Self {
        Self {
            _seed: seed,
            perlin: Perlin::new(seed as u32),
        }
    }
}

impl NoiseGenerator for SimplePerlinGenerator {
    fn sample_2d(&self, _seed: u64, frequency: f32, x: f64, z: f64) -> f64 {
        self.perlin.get([x * frequency as f64, z * frequency as f64])
    }

    fn sample_3d(&self, _seed: u64, freq_x: f32, freq_y: f32, freq_z: f32, x: f64, y: f64, z: f64) -> f64 {
        self.perlin.get([
            x * freq_x as f64,
            y * freq_y as f64,
            z * freq_z as f64,
        ])
    }
}
