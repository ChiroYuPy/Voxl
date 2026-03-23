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
        apply_physics_with_collisions(pos, vel, physics, aabb, voxel_world, delta_time, false);
    }
}

/// Applies physics with collision detection (Minecraft-style)
pub fn apply_physics_with_collisions(
    pos: &mut Position,
    vel: &mut Velocity,
    physics: &mut PhysicsAffected,
    aabb: &AABB,
    voxel_world: &VoxelWorld,
    delta_time: f32,
    is_sneaking: bool,
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

    // Apply air/ground drag to horizontal velocity
    let drag = if physics.on_ground {
        physics.ground_drag
    } else {
        physics.air_drag
    };

    // Apply drag: v = v * (1 - drag * dt)
    let drag_factor = 1.0 - (drag * delta_time).min(1.0);
    vel.x *= drag_factor;
    vel.z *= drag_factor;

    // Very small velocities become zero
    if vel.x.abs() < 0.001 {
        vel.x = 0.0;
    }
    if vel.z.abs() < 0.001 {
        vel.z = 0.0;
    }

    // Check if entity is stuck inside a block and push out if possible
    let current_pos = pos.as_vec3();
    if check_voxel_collision(pos, aabb, voxel_world) {
        // Entity is inside a block - check if we can push up
        let mut push_up_distance = 0.0;
        const MAX_PUSH_UP: f32 = 2.0;  // Maximum distance to push up

        // Find how much we need to push up to get out of collision
        for test_y in 1..=(MAX_PUSH_UP as i32) {
            let test_pos = Vec3::new(current_pos.x, current_pos.y + test_y as f32, current_pos.z);
            pos.set(test_pos);
            if !check_voxel_collision(pos, aabb, voxel_world) {
                push_up_distance = test_y as f32;
                break;
            }
        }

        if push_up_distance > 0.0 {
            // Push the entity up to the free position
            pos.set(Vec3::new(current_pos.x, current_pos.y + push_up_distance, current_pos.z));
            vel.y = 0.0;
        } else {
            // No free space above, leave entity where it is
            pos.set(current_pos);
        }
    }

    let old_pos = pos.as_vec3();
    let mut new_pos = old_pos + vel.as_vec3() * delta_time;

    // Sneaking edge detection: prevent falling off blocks when sneaking
    if is_sneaking && physics.on_ground {
        // Check X movement for edge
        if vel.x != 0.0 {
            let test_pos_x = Vec3::new(new_pos.x, old_pos.y, old_pos.z);
            pos.set(test_pos_x);
            let has_support_x = has_solid_block_below(pos, aabb, voxel_world);

            if !has_support_x && !check_voxel_collision(pos, aabb, voxel_world) {
                // No solid block below when moving in X - don't move
                new_pos.x = old_pos.x;
                vel.x = 0.0;
            }
        }

        // Check Z movement for edge
        if vel.z != 0.0 {
            let test_pos_z = Vec3::new(new_pos.x, old_pos.y, new_pos.z);
            pos.set(test_pos_z);
            let has_support_z = has_solid_block_below(pos, aabb, voxel_world);

            if !has_support_z && !check_voxel_collision(pos, aabb, voxel_world) {
                // No solid block below when moving in Z - don't move
                new_pos.z = old_pos.z;
                vel.z = 0.0;
            }
        }
    }

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

/// Checks if there's a solid block below the given position
/// Used for sneaking edge detection
pub fn has_solid_block_below(pos: &Position, aabb: &AABB, voxel_world: &VoxelWorld) -> bool {
    let (aabb_min, aabb_max) = aabb.bounds(pos);

    // Check the block directly below the player's feet
    let feet_y = (aabb_min.y - 0.01).floor() as i32;

    // Check a 3x3 area below the player for edge detection
    let min_x = aabb_min.x.floor() as i32;
    let max_x = aabb_max.x.floor() as i32;
    let min_z = aabb_min.z.floor() as i32;
    let max_z = aabb_max.z.floor() as i32;

    for x in min_x..=max_x {
        for z in min_z..=max_z {
            if let Some(vid) = voxel_world.get_voxel_opt(x, feet_y, z) {
                if vid != 0 {
                    let registry = voxel_world.registry();
                    if let Some(v) = registry.get(vid) {
                        if v.collidable {
                            return true;
                        }
                    }
                }
            }
        }
    }

    false
}
