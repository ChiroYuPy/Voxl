//! Shared ECS systems for the game
//!
//! This module contains systems that can be used by both
//! client and server (mainly physics).

use glam::Vec3;
use hecs::World;

use super::components::{Position, Velocity, PhysicsAffected, AABB};
use crate::voxel::VoxelWorld;

/// Physics system for generic entities (without PlayerControlled)
pub fn entity_physics_system(
    world: &mut World,
    voxel_world: &VoxelWorld,
    delta_time: f32,
) {
    for (pos, vel, physics, aabb) in world.query_mut::<(
        &mut Position,
        &mut Velocity,
        &mut PhysicsAffected,
        &AABB,
    )>() {
        apply_physics_with_collisions(pos, vel, physics, aabb, voxel_world, delta_time);
    }
}

/// Applies physics with collision detection
pub fn apply_physics_with_collisions(
    pos: &mut Position,
    vel: &mut Velocity,
    physics: &mut PhysicsAffected,
    aabb: &AABB,
    voxel_world: &VoxelWorld,
    delta_time: f32,
) {
    // Apply gravity
    vel.y -= physics.gravity * delta_time;

    // Clamp terminal velocity
    if vel.y < -physics.terminal_velocity {
        vel.y = -physics.terminal_velocity;
    }
    if vel.y > physics.terminal_velocity {
        vel.y = physics.terminal_velocity;
    }

    let old_pos = pos.as_vec3();
    let mut new_pos = old_pos + vel.as_vec3() * delta_time;

    // Collision system with voxels
    // Process each axis separately to allow sliding along walls

    // X axis
    let test_pos_x = Vec3::new(new_pos.x, old_pos.y, old_pos.z);
    pos.set(test_pos_x);
    if check_voxel_collision(pos, aabb, voxel_world) {
        new_pos.x = old_pos.x;
        vel.x = 0.0;
    }

    // Z axis
    let test_pos_z = Vec3::new(new_pos.x, old_pos.y, new_pos.z);
    pos.set(test_pos_z);
    if check_voxel_collision(pos, aabb, voxel_world) {
        new_pos.z = old_pos.z;
        vel.z = 0.0;
    }

    // Y axis (last to allow jumping against walls)
    pos.set(new_pos);
    if check_voxel_collision(pos, aabb, voxel_world) {
        if vel.y < 0.0 {
            physics.on_ground = true;
        }
        new_pos.y = old_pos.y;
        vel.y = 0.0;
    } else {
        physics.on_ground = false;
    }

    pos.set(new_pos);
}

/// Flying movement with collisions (no gravity)
pub fn apply_fly_with_collisions(
    pos: &mut Position,
    vel: &mut Velocity,
    aabb: &AABB,
    voxel_world: &VoxelWorld,
    delta_time: f32,
) {
    let old_pos = pos.as_vec3();
    let mut new_pos = old_pos + vel.as_vec3() * delta_time;

    // Check each axis separately
    let test_pos_x = Vec3::new(new_pos.x, old_pos.y, old_pos.z);
    pos.set(test_pos_x);
    if check_voxel_collision(pos, aabb, voxel_world) {
        new_pos.x = old_pos.x;
        vel.x = 0.0;
    }

    let test_pos_z = Vec3::new(new_pos.x, old_pos.y, new_pos.z);
    pos.set(test_pos_z);
    if check_voxel_collision(pos, aabb, voxel_world) {
        new_pos.z = old_pos.z;
        vel.z = 0.0;
    }

    pos.set(new_pos);
    if check_voxel_collision(pos, aabb, voxel_world) {
        new_pos.y = old_pos.y;
        vel.y = 0.0;
    }

    pos.set(new_pos);
}

/// Checks if an entity's AABB collides with any solid voxel
pub fn check_voxel_collision(pos: &Position, aabb: &AABB, voxel_world: &VoxelWorld) -> bool {
    let potential_voxels = aabb.get_potential_collisions(pos);
    let (aabb_min, aabb_max) = aabb.bounds(pos);

    for voxel_pos in potential_voxels {
        let voxel_id = voxel_world.get_voxel_opt(voxel_pos.x, voxel_pos.y, voxel_pos.z);

        if let Some(vid) = voxel_id {
            if vid == 0 {
                continue;
            }

            let registry = voxel_world.registry();
            let is_solid = match registry.get(vid) {
                Some(v) => v.collidable,
                None => false,
            };

            if is_solid {
                let block_min = Vec3::new(
                    voxel_pos.x as f32,
                    voxel_pos.y as f32,
                    voxel_pos.z as f32,
                );
                let block_max = block_min + Vec3::new(1.0, 1.0, 1.0);

                if aabb_min.x < block_max.x && aabb_max.x > block_min.x
                    && aabb_min.y < block_max.y && aabb_max.y > block_min.y
                    && aabb_min.z < block_max.z && aabb_max.z > block_min.z
                {
                    return true;
                }
            }
        }
    }

    false
}
