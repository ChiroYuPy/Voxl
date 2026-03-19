use glam::{IVec3, Vec3, Vec3A};
use crate::raycast::dda::DdaState;
use crate::raycast::types::RaycastResult;
use crate::renderer::Camera;
use crate::voxel::GlobalVoxelId;

#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3,
}

impl Ray {
    pub fn new(origin: Vec3, direction: Vec3) -> Self {
        Self { origin, direction }
    }

    pub fn from_camera(camera: &Camera) -> Self {
        Self {
            origin: camera.position.into(),
            direction: camera.forward().into(),
        }
    }

    pub fn from_vec3a(origin: Vec3A, direction: Vec3A) -> Self {
        Self {
            origin: origin.into(),
            direction: direction.into(),
        }
    }

    pub fn origin_as_vec3a(&self) -> Vec3A {
        self.origin.into()
    }

    pub fn direction_as_vec3a(&self) -> Vec3A {
        self.direction.into()
    }

    pub fn at(&self, t: f32) -> Vec3 {
        self.origin + self.direction * t
    }
}

pub trait Raycast {
    fn cast_blocks<F>(&self, max_distance: f32, block_provider: F) -> Option<RaycastResult>
    where
        F: FnMut(IVec3) -> Option<GlobalVoxelId>;
}

impl Raycast for Ray {
    fn cast_blocks<F>(&self, max_distance: f32, mut block_provider: F) -> Option<RaycastResult>
    where
        F: FnMut(IVec3) -> Option<GlobalVoxelId>,
    {
        let mut dda = DdaState::new(self.origin, self.direction, max_distance);

        dda.exit_starting_block(|pos| {
            // Considérer comme solide seulement si c'est Some(id) et id != 0 (air)
            block_provider(pos).map_or(false, |id| id != 0)
        });

        dda
            .filter_map(|(pos, face, t)| {
                block_provider(pos).and_then(|block_type| {
                    if block_type != 0 {
                        // Seulement les blocs non-air (id != 0)
                        Some(RaycastResult {
                            block_pos: pos,
                            face,
                            block_type,
                            distance: t,
                        })
                    } else {
                        None
                    }
                })
            })
            .next()
    }
}
