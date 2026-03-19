use crate::voxel::chunk::CHUNK_SIZE;
use std::collections::HashSet;

#[derive(Default, Clone, Debug)]
pub struct DirtyChunkSet {
    dirty: HashSet<(i32, i32, i32)>,
}

impl DirtyChunkSet {
    pub fn new() -> Self {
        Self {
            dirty: HashSet::new(),
        }
    }

    pub fn mark_dirty(&mut self, chunk_x: i32, chunk_y: i32, chunk_z: i32) {
        self.dirty.insert((chunk_x, chunk_y, chunk_z));
    }

    pub fn mark_voxel_dirty(&mut self, world_x: i32, world_y: i32, world_z: i32) {
        let chunk_x = world_x.div_euclid(CHUNK_SIZE as i32);
        let chunk_y = world_y.div_euclid(CHUNK_SIZE as i32);
        let chunk_z = world_z.div_euclid(CHUNK_SIZE as i32);
        self.dirty.insert((chunk_x, chunk_y, chunk_z));
    }

    pub fn take_dirty(&mut self) -> Vec<(i32, i32, i32)> {
        let result: Vec<(i32, i32, i32)> = self.dirty.iter().copied().collect();
        self.dirty.clear();
        result
    }

    pub fn is_dirty(&self, chunk_x: i32, chunk_y: i32, chunk_z: i32) -> bool {
        self.dirty.contains(&(chunk_x, chunk_y, chunk_z))
    }

    pub fn len(&self) -> usize {
        self.dirty.len()
    }

    pub fn clear(&mut self) {
        self.dirty.clear();
    }
}
