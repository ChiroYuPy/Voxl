//! Client-specific ECS systems
//!
//! This module contains systems that depend on input (keyboard/mouse)
//! and display, which are specific to the client.

use crate::input::InputState;
use crate::input::keybinds::GameAction;
use voxl_common::voxel::VoxelWorld;
use voxl_common::entities::*;
use glam::Vec3;
use hecs::World;

/// Player input system: updates look direction and applies acceleration
/// based on keyboard/mouse input (Minecraft-style physics)
pub fn player_input_system(
    world: &mut World,
    input: &InputState,
    delta_time: f32,
) {
    for (controlled, look_dir, velocity, physics) in world.query_mut::<(&mut PlayerControlled, &mut LookDirection, &mut Velocity, &PhysicsAffected)>() {
        // Update sneaking state
        let sneak_held = input.is_held(GameAction::Sneak);
        let is_flying = controlled.is_flying();
        controlled.is_sneaking = sneak_held && !is_flying;

        // Smooth eye height transition (Minecraft-style: ~0.1 sec transition)
        let target_eye_height = if controlled.is_sneaking {
            0.47  // Sneaking: 1.27m - 0.8m (player center) = 0.47m
        } else {
            0.7   // Standing: 1.6m - 0.9m (player center) = 0.7m
        };

        // Interpolate with speed of 5.0 per second (smooth but noticeable transition)
        let lerp_speed = 5.0 * delta_time;
        let diff = target_eye_height - controlled.current_eye_height;
        if diff.abs() < 0.001 {
            controlled.current_eye_height = target_eye_height;
        } else {
            controlled.current_eye_height += diff * lerp_speed.min(1.0);
        }

        // Update sprinting state (can't sprint while sneaking)
        controlled.is_sprinting = input.is_held(GameAction::IncreaseSpeed) && !controlled.is_sneaking;

        // Mouse movement for rotation
        if input.is_mouse_captured() {
            let (dx, dy) = input.mouse_delta();
            look_dir.apply_mouse_delta(dx as f32, dy as f32, controlled.look_sensitivity, controlled.pitch_limits);
        }

        // Calculate movement direction
        let forward = look_dir.forward();
        let right = look_dir.right();

        // Horizontal forward (without pitch) for ground movement
        let forward_flat = Vec3::new(forward.x, 0.0, forward.z).normalize();

        let mut move_dir = Vec3::ZERO;

        if input.is_held(GameAction::MoveForward) {
            move_dir += forward_flat;
        }
        if input.is_held(GameAction::MoveBackward) {
            move_dir -= forward_flat;
        }
        if input.is_held(GameAction::MoveRight) {
            move_dir += right;
        }
        if input.is_held(GameAction::MoveLeft) {
            move_dir -= right;
        }

        // In fly mode, can move up and down
        if controlled.is_flying() {
            if input.is_held(GameAction::MoveUp) {
                move_dir.y += 1.0;
            }
            if input.is_held(GameAction::MoveDown) {
                move_dir.y -= 1.0;
            }
        }

        // Normalize if non-zero
        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize();
        }

        // Calculate speed multiplier
        let mut speed_mult = 1.0;
        if controlled.is_sprinting {
            // Sprint: x2 on ground, x10 in air
            if physics.on_ground {
                speed_mult = 2.0;
            } else {
                speed_mult = 10.0;
            }
        }
        if controlled.is_sneaking {
            speed_mult = controlled.sneak_multiplier;
        }

        // Determine target speed and acceleration
        if controlled.is_flying() {
            // Fly mode: direct velocity control for responsive flying
            let fly_speed = physics.move_speed * 2.0 * speed_mult;
            let target_vel = move_dir * fly_speed;

            // Smooth interpolation for flying
            let lerp_factor = 10.0 * delta_time;
            velocity.x += (target_vel.x - velocity.x) * lerp_factor.min(1.0);
            velocity.y += (target_vel.y - velocity.y) * lerp_factor.min(1.0);
            velocity.z += (target_vel.z - velocity.z) * lerp_factor.min(1.0);
        } else {
            // Ground mode: apply acceleration towards target speed
            let target_speed = physics.move_speed * speed_mult;

            if move_dir.length_squared() > 0.0 {
                // Apply acceleration in movement direction
                // When sprinting, use higher acceleration to overcome drag
                let base_accel = if physics.on_ground {
                    physics.ground_acceleration
                } else {
                    physics.air_acceleration
                };
                let accel = if controlled.is_sprinting && physics.on_ground {
                    base_accel * speed_mult  // Scale acceleration with sprint multiplier
                } else {
                    base_accel
                };

                // Accelerate towards target speed in movement direction
                let target_vel = move_dir * target_speed;
                let accel_delta = target_vel - Vec3::new(velocity.x, 0.0, velocity.z);

                // Apply acceleration
                let accel_amount = accel * delta_time;

                // If we're close to target velocity, just set it
                if accel_delta.length() < accel_amount {
                    velocity.x = target_vel.x;
                    velocity.z = target_vel.z;
                } else {
                    let accel_dir = accel_delta.normalize();
                    velocity.x += accel_dir.x * accel_amount;
                    velocity.z += accel_dir.z * accel_amount;
                }

                // Clamp to target speed
                let current_h_speed = (velocity.x * velocity.x + velocity.z * velocity.z).sqrt();
                if current_h_speed > target_speed {
                    let scale = target_speed / current_h_speed;
                    velocity.x *= scale;
                    velocity.z *= scale;
                }
            }
            // Y is handled by physics system (jump/gravity)
        }
    }
}

/// Physics system for the player (with PlayerControlled)
pub fn player_physics_system(
    world: &mut World,
    voxel_world: &VoxelWorld,
    delta_time: f32,
) {
    use voxl_common::entities::systems::{apply_physics_with_collisions, apply_fly_with_collisions};

    for (pos, vel, physics, aabb, controlled) in world.query_mut::<(
        &mut Position,
        &mut Velocity,
        &mut PhysicsAffected,
        &AABB,
        &PlayerControlled,
    )>() {
        let is_flying = controlled.is_flying();
        let has_collisions = controlled.has_collisions();
        let is_sneaking = controlled.is_sneaking;

        if !has_collisions {
            // Spectator mode: no collisions, no gravity
            let old_pos = pos.as_vec3();
            let new_pos = old_pos + vel.as_vec3() * delta_time;
            pos.set(new_pos);
        } else if is_flying {
            // Creative mode WITH fly: collisions, but no gravity
            apply_fly_with_collisions(pos, vel, aabb, voxel_world, delta_time);
        } else {
            // Creative mode WITHOUT fly: collisions and gravity
            apply_physics_with_collisions(pos, vel, physics, aabb, voxel_world, delta_time, is_sneaking);
        }
    }
}

/// Camera system: updates camera to follow the player
/// Returns (position, yaw, pitch) of the camera
pub fn camera_sync_system(
    world: &World,
    player_entity: hecs::Entity,
) -> Option<(Vec3, f32, f32)> {
    let entity_ref = world.entity(player_entity).ok()?;
    let pos = entity_ref.get::<&Position>()?;
    let look_dir = entity_ref.get::<&LookDirection>()?;
    let controlled = entity_ref.get::<&PlayerControlled>()?;

    // Use the smoothly interpolated eye height
    let camera_pos = pos.as_vec3() + Vec3::new(0.0, controlled.current_eye_height, 0.0);

    Some((camera_pos, look_dir.yaw, look_dir.pitch))
}

/// Jump system: handles jumping when player presses the key
pub fn jump_system(world: &mut World, input: &InputState, _voxel_world: &VoxelWorld) {
    for (vel, physics, controlled) in world.query_mut::<(&mut Velocity, &mut PhysicsAffected, &PlayerControlled)>() {
        // Only jump when on ground and pressing jump
        // In fly mode, MoveUp/MoveDown are handled by input system
        if controlled.is_flying() {
            continue;
        }

        if physics.on_ground && input.is_held(GameAction::MoveUp) {
            vel.y = physics.jump_velocity();
            physics.on_ground = false;
        }
    }
}
