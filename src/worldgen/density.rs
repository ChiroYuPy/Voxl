//! Fonctions de densité pour la génération de terrain
//!
//! Les density functions définissent la forme du terrain de manière composable.
//! Inspiré du système de Minecraft 1.18+.

use serde::Deserialize;
use std::collections::HashMap;

use super::noise::NoiseGenerator;

/// Fonction de densité composite
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum DensityFunction {
    Constant(f32),
    Add {
        arg1: Box<DensityFunction>,
        arg2: Box<DensityFunction>,
    },
    Mul {
        factor: f32,
        arg: Box<DensityFunction>,
    },
    MulBoth {
        arg1: Box<DensityFunction>,
        arg2: Box<DensityFunction>,
    },
    Min {
        arg1: Box<DensityFunction>,
        arg2: Box<DensityFunction>,
    },
    Max {
        arg1: Box<DensityFunction>,
        arg2: Box<DensityFunction>,
    },
    Clamp {
        min: f32,
        max: f32,
        input: Box<DensityFunction>,
    },
    Abs(Box<DensityFunction>),
    Square(Box<DensityFunction>),
    Cube(Box<DensityFunction>),
    Sqrt(Box<DensityFunction>),
    Noise2D {
        seed: u64,
        frequency: f32,
    },
    Noise3D {
        seed: u64,
        frequency: f32,
    },
    /// Terrain shift avec offset en XZ et Y (pour le router climatique)
    ShiftedNoise {
        noise: String,
        xz_scale: f32,
        y_scale: f32,
    },
    /// Gradient vertical clampé entre deux Y
    YClampedGradient {
        from_y: i32,
        to_y: i32,
        from_val: f32,
        to_val: f32,
    },
    /// Cache une valeur pour éviter de recalculer
    CacheOnce(Box<DensityFunction>),
    /// Référence à une fonction nommée
    Reference(String),
    /// Interpolation entre deux valeurs selon Y
    Lerp {
        from: f32,
        to: f32,
        from_val: Box<DensityFunction>,
        to_val: Box<DensityFunction>,
    },
    /// Density function pour les cavernes (noise 3D avec seuil)
    Cave {
        seed: u64,
        frequency: f32,
        threshold: f32,
    },
}

impl DensityFunction {
    /// Évalue la fonction à une position donnée
    pub fn sample(&self, x: f64, y: f64, z: f64, ctx: &mut DensityContext) -> f64 {
        match self {
            DensityFunction::Constant(v) => *v as f64,

            DensityFunction::Add { arg1, arg2 } => {
                arg1.sample(x, y, z, ctx) + arg2.sample(x, y, z, ctx)
            }

            DensityFunction::Mul { factor, arg } => {
                (*factor as f64) * arg.sample(x, y, z, ctx)
            }

            DensityFunction::MulBoth { arg1, arg2 } => {
                arg1.sample(x, y, z, ctx) * arg2.sample(x, y, z, ctx)
            }

            DensityFunction::Min { arg1, arg2 } => {
                arg1.sample(x, y, z, ctx).min(arg2.sample(x, y, z, ctx))
            }

            DensityFunction::Max { arg1, arg2 } => {
                arg1.sample(x, y, z, ctx).max(arg2.sample(x, y, z, ctx))
            }

            DensityFunction::Clamp { min, max, input } => {
                input.sample(x, y, z, ctx).clamp(*min as f64, *max as f64)
            }

            DensityFunction::Abs(arg) => {
                arg.sample(x, y, z, ctx).abs()
            }

            DensityFunction::Square(arg) => {
                let v = arg.sample(x, y, z, ctx);
                v * v
            }

            DensityFunction::Cube(arg) => {
                let v = arg.sample(x, y, z, ctx);
                v * v * v
            }

            DensityFunction::Sqrt(arg) => {
                arg.sample(x, y, z, ctx).sqrt().max(0.0)
            }

            DensityFunction::Noise2D { seed, frequency } => {
                ctx.noise.sample_2d(*seed, *frequency, x, z)
            }

            DensityFunction::Noise3D { seed, frequency } => {
                ctx.noise.sample_3d(*seed, *frequency, *frequency, *frequency, x, y, z)
            }

            DensityFunction::ShiftedNoise { noise, xz_scale, y_scale } => {
                // Applique l'échelle avant d'échantillonner
                let nx = x * *xz_scale as f64;
                let nz = z * *xz_scale as f64;
                let ny = if *y_scale > 0.0 { y * *y_scale as f64 } else { 0.0 };

                if let Some(func) = ctx.named_functions.get(noise) {
                    func.sample(nx, ny, nz, ctx)
                } else {
                    0.0
                }
            }

            DensityFunction::YClampedGradient { from_y, to_y, from_val, to_val } => {
                let range = (*to_y - *from_y) as f64;
                if range == 0.0 {
                    return *from_val as f64;
                }
                let t = (y - *from_y as f64) / range;
                let t = t.clamp(0.0, 1.0);
                *from_val as f64 + (*to_val - *from_val) as f64 * t
            }

            DensityFunction::CacheOnce(arg) => {
                // Pour l'instant, évalue directement (le vrai cache nécessite plus de travail)
                arg.sample(x, y, z, ctx)
            }

            DensityFunction::Lerp { from, to, from_val, to_val } => {
                let t = ((y - *from as f64) / (*to - *from) as f64).clamp(0.0, 1.0);
                let a = from_val.sample(x, y, z, ctx);
                let b = to_val.sample(x, y, z, ctx);
                a + t * (b - a)
            }

            DensityFunction::Cave { seed, frequency, threshold } => {
                let noise = ctx.noise.sample_3d(*seed, *frequency, *frequency, *frequency, x, y, z);
                if noise > *threshold as f64 { 1.0 } else { -1.0 }
            }

            DensityFunction::Reference(name) => {
                if let Some(func) = ctx.named_functions.get(name) {
                    func.sample(x, y, z, ctx)
                } else {
                    0.0
                }
            }
        }
    }
}

/// Contexte d'évaluation des fonctions de densité
pub struct DensityContext<'a> {
    pub noise: &'a dyn NoiseGenerator,
    pub named_functions: &'a HashMap<String, DensityFunction>,
}

impl<'a> DensityContext<'a> {
    pub fn new(
        noise: &'a dyn NoiseGenerator,
        named_functions: &'a HashMap<String, DensityFunction>,
    ) -> Self {
        Self {
            noise,
            named_functions,
        }
    }

    /// Échantillonne la densité finale
    pub fn sample_density(&mut self, x: f64, y: f64, z: f64) -> f64 {
        if let Some(func) = self.named_functions.get("final_density") {
            func.sample(x, y, z, self)
        } else {
            0.0
        }
    }
}
