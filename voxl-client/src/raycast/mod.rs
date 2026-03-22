pub mod dda;
pub mod ray;
pub mod types;

pub use ray::{Ray, Raycast};
pub use types::{RaycastResult, Axis};

pub const RAYCAST_DISTANCE: f32 = 100.0;