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

/// Player input system: updates look direction and velocity
/// based on keyboard/mouse input
pub fn player_input_system(
    world: &mut World,
    input: &InputState,
    _delta_time: f32,
) {
    for (controlled, look_dir, velocity) in world.query_mut::<(&PlayerControlled, &mut LookDirection, &mut Velocity)>() {
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

        // Handle sprint
        let sprint = input.is_held(GameAction::IncreaseSpeed);
        let speed_mult = if sprint { controlled.sprint_multiplier } else { 1.0 };

        // In fly mode, apply speed directly to velocity
        // In non-fly mode, only apply horizontal velocity (gravity handles Y)
        if controlled.is_flying() {
            let fly_speed = 8.0 * speed_mult;
            velocity.set(move_dir * fly_speed);
        } else {
            let ground_speed = 8.0 * speed_mult;
            velocity.x = move_dir.x * ground_speed;
            velocity.z = move_dir.z * ground_speed;
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
            apply_physics_with_collisions(pos, vel, physics, aabb, voxel_world, delta_time);
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

    // Camera position: slightly above the entity center
    // For a player, we place "eyes" at 1.6m from ground
    // Player center is at y = pos.y, so eyes are at pos.y + 0.7
    let eye_height = 0.7;
    let camera_pos = pos.as_vec3() + Vec3::new(0.0, eye_height, 0.0);

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
