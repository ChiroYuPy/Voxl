use glam::{IVec3, Vec3};
use crate::renderer::voxel_map::VoxelFace;

#[derive(Debug, Clone, Copy)]
struct AxisState {
    step: i32,
    t_max: f32,
    t_delta: f32,
}

impl AxisState {
    fn new(ray_origin: f32, ray_dir: f32, voxel_coord: f32) -> Self {
        let step = if ray_dir > 0.0 {
            1
        } else if ray_dir < 0.0 {
            -1
        } else {
            0
        };

        let t_max = if step > 0 {
            (voxel_coord + 1.0 - ray_origin) / ray_dir
        } else if step < 0 {
            (voxel_coord - ray_origin) / ray_dir
        } else {
            f32::MAX
        };

        let t_delta = if ray_dir != 0.0 {
            (1.0 / ray_dir).abs()
        } else {
            f32::MAX
        };

        Self { step, t_max, t_delta }
    }
}

#[derive(Debug, Clone)]
pub struct DdaState {
    current: IVec3,
    axes: [AxisState; 3],
    entry_face: VoxelFace,
    max_distance: f32,
}

impl DdaState {
    pub fn new(origin: Vec3, direction: Vec3, max_distance: f32) -> Self {
        let x = origin.x.floor() as i32;
        let y = origin.y.floor() as i32;
        let z = origin.z.floor() as i32;

        let axes = [
            AxisState::new(origin.x, direction.x, x as f32),
            AxisState::new(origin.y, direction.y, y as f32),
            AxisState::new(origin.z, direction.z, z as f32),
        ];

        Self {
            current: IVec3::new(x, y, z),
            axes,
            entry_face: VoxelFace::Top,
            max_distance,
        }
    }

    pub fn exit_starting_block<F>(&mut self, mut is_solid: F) -> bool
    where
        F: FnMut(IVec3) -> bool,
    {
        if is_solid(self.current) {
            self.step();
            true
        } else {
            false
        }
    }

    fn current_t(&self) -> f32 {
        self.axes[0].t_max.min(self.axes[1].t_max).min(self.axes[2].t_max)
    }

    fn step(&mut self) -> bool {
        let axis = if self.axes[0].t_max < self.axes[1].t_max {
            if self.axes[0].t_max < self.axes[2].t_max { 0 } else { 2 }
        } else {
            if self.axes[1].t_max < self.axes[2].t_max { 1 } else { 2 }
        };

        match axis {
            0 => {
                self.current.x += self.axes[0].step;
                self.axes[0].t_max += self.axes[0].t_delta;
                self.entry_face = if self.axes[0].step > 0 { VoxelFace::West } else { VoxelFace::East };
            }
            1 => {
                self.current.y += self.axes[1].step;
                self.axes[1].t_max += self.axes[1].t_delta;
                self.entry_face = if self.axes[1].step > 0 { VoxelFace::Bottom } else { VoxelFace::Top };
            }
            2 => {
                self.current.z += self.axes[2].step;
                self.axes[2].t_max += self.axes[2].t_delta;
                self.entry_face = if self.axes[2].step > 0 { VoxelFace::South } else { VoxelFace::North };
            }
            _ => unreachable!(),
        }

        self.current_t() <= self.max_distance
    }
}

impl Iterator for DdaState {
    type Item = (IVec3, VoxelFace, f32);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_t() > self.max_distance {
            return None;
        }

        let result = (self.current, self.entry_face, self.current_t());

        if !self.step() {
            return None;
        }

        Some(result)
    }
}