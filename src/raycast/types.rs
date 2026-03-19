use crate::voxel::GlobalVoxelId;
use crate::renderer::voxel_map::VoxelFace;
use glam::IVec3;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis { X, Y, Z, }

impl Axis {
    pub const ALL: [Axis; 3] = [Axis::X, Axis::Y, Axis::Z];
}

#[derive(Debug, Clone)]
pub struct RaycastResult {
    pub block_pos: IVec3,
    pub face: VoxelFace,
    pub block_type: GlobalVoxelId,
    pub distance: f32,
}

impl RaycastResult {
    pub fn new(block_pos: IVec3, face: VoxelFace, block_type: GlobalVoxelId, distance: f32) -> Self {
        Self {
            block_pos,
            face,
            block_type,
            distance,
        }
    }

    pub fn adjacent_pos(&self) -> IVec3 {
        self.block_pos + self.face.normal()
    }
}
