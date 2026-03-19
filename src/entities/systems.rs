//! Systèmes ECS pour mettre à jour les entités

use crate::input::InputState;
use crate::input::keybinds::GameAction;
use crate::voxel::VoxelWorld;
use glam::Vec3;
use hecs::World;

use super::components::{
    Position, Velocity, PlayerControlled, LookDirection, PhysicsAffected, AABB,
};

/// Système d'input joueur: met à jour la direction du regard et la vélocité
/// basée sur les entrées clavier/souris
pub fn player_input_system(
    world: &mut World,
    input: &InputState,
    _delta_time: f32,
) {
    // Pour le moment, on suppose qu'il y a un seul joueur
    // On va itérer sur toutes les entités avec PlayerControlled
    for (controlled, look_dir, velocity) in world.query_mut::<(&PlayerControlled, &mut LookDirection, &mut Velocity)>() {
        // Mouvement de souris pour la rotation
        if input.is_mouse_captured() {
            let (dx, dy) = input.mouse_delta();
            look_dir.apply_mouse_delta(dx as f32, dy as f32, controlled.look_sensitivity, controlled.pitch_limits);
        }

        // Calculer la direction de mouvement
        let forward = look_dir.forward();
        let right = look_dir.right();

        // Forward horizontal (sans le pitch) pour les déplacements au sol
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

        // En mode vol, on peut monter et descendre
        if controlled.is_flying() {
            if input.is_held(GameAction::MoveUp) {
                move_dir.y += 1.0;
            }
            if input.is_held(GameAction::MoveDown) {
                move_dir.y -= 1.0;
            }
        }

        // Normaliser si non-nul
        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize();
        }

        // Gérer le sprint
        let sprint = input.is_held(GameAction::IncreaseSpeed);
        let speed_mult = if sprint { controlled.sprint_multiplier } else { 1.0 };

        // En mode fly, la vitesse est appliquée directement à la vélocité
        // En mode non-fly, on applique seulement la vélocité horizontale (la gravité gère le Y)
        if controlled.is_flying() {
            let fly_speed = 8.0 * speed_mult;
            velocity.set(move_dir * fly_speed);
        } else {
            let ground_speed = 8.0 * speed_mult;
            velocity.x = move_dir.x * ground_speed;
            velocity.z = move_dir.z * ground_speed;
            // Y est géré par le système de physique (saut/gravité)
        }
    }
}

/// Système de physique pour le joueur (avec PlayerControlled)
pub fn player_physics_system(
    world: &mut World,
    voxel_world: &VoxelWorld,
    delta_time: f32,
) {
    // Traiter les entités joueur (avec PlayerControlled)
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
            // Mode spectateur: pas de collisions, pas de gravité
            let old_pos = pos.as_vec3();
            let new_pos = old_pos + vel.as_vec3() * delta_time;
            pos.set(new_pos);
        } else if is_flying {
            // Mode créatif AVEC fly: collisions, mais pas de gravité
            apply_fly_with_collisions(pos, vel, aabb, voxel_world, delta_time);
        } else {
            // Mode créatif SANS fly: collisions et gravité
            apply_physics_with_collisions(pos, vel, physics, aabb, voxel_world, delta_time);
        }
    }
}

/// Système de physique pour les entités non-joueur (sans PlayerControlled)
pub fn entity_physics_system(
    world: &mut World,
    voxel_world: &VoxelWorld,
    delta_time: f32,
) {
    // Traiter les entités sans PlayerControlled (mobs, etc.)
    for (pos, vel, physics, aabb) in world.query_mut::<(
        &mut Position,
        &mut Velocity,
        &mut PhysicsAffected,
        &AABB,
    )>() {
        apply_physics_with_collisions(pos, vel, physics, aabb, voxel_world, delta_time);
    }
}

/// Fonction commune pour appliquer la physique avec collisions
fn apply_physics_with_collisions(
    pos: &mut Position,
    vel: &mut Velocity,
    physics: &mut PhysicsAffected,
    aabb: &AABB,
    voxel_world: &VoxelWorld,
    delta_time: f32,
) {
    // Appliquer la gravité
    vel.y -= physics.gravity * delta_time;
    // Limiter la vélocité terminale
    if vel.y < -physics.terminal_velocity {
        vel.y = -physics.terminal_velocity;
    }
    if vel.y > physics.terminal_velocity {
        vel.y = physics.terminal_velocity;
    }

    // Sauvegarder l'ancienne position
    let old_pos = pos.as_vec3();

    // Calculer la nouvelle position proposée
    let mut new_pos = old_pos + vel.as_vec3() * delta_time;

    // Système de collision avec les voxels
    // On traite chaque axe séparément pour permettre le sliding le long des murs

    // Axe X - tester seulement le mouvement en X
    let test_pos_x = Vec3::new(new_pos.x, old_pos.y, old_pos.z);
    pos.set(test_pos_x);
    if check_voxel_collision(pos, aabb, voxel_world) {
        // Collision en X: annuler le mouvement X
        new_pos.x = old_pos.x;
        vel.x = 0.0;
    }

    // Axe Z - tester avec le nouveau X mais l'ancien Y
    let test_pos_z = Vec3::new(new_pos.x, old_pos.y, new_pos.z);
    pos.set(test_pos_z);
    if check_voxel_collision(pos, aabb, voxel_world) {
        // Collision en Z: annuler le mouvement Z
        new_pos.z = old_pos.z;
        vel.z = 0.0;
    }

    // Axe Y (dernier pour permettre le saut contre un mur)
    pos.set(new_pos);
    if check_voxel_collision(pos, aabb, voxel_world) {
        // Collision en Y
        if vel.y < 0.0 {
            // En train de tomber -> on est au sol
            physics.on_ground = true;
        }
        new_pos.y = old_pos.y;
        vel.y = 0.0;
    } else {
        physics.on_ground = false;
    }

    // Appliquer la position finale
    pos.set(new_pos);
}

/// Mouvement en fly avec collisions (pas de gravité)
fn apply_fly_with_collisions(
    pos: &mut Position,
    vel: &mut Velocity,
    aabb: &AABB,
    voxel_world: &VoxelWorld,
    delta_time: f32,
) {
    let old_pos = pos.as_vec3();
    let mut new_pos = old_pos + vel.as_vec3() * delta_time;

    // Collision sur chaque axe séparément
    // Axe X
    let test_pos_x = Vec3::new(new_pos.x, old_pos.y, old_pos.z);
    pos.set(test_pos_x);
    if check_voxel_collision(pos, aabb, voxel_world) {
        new_pos.x = old_pos.x;
        vel.x = 0.0;
    }

    // Axe Z
    let test_pos_z = Vec3::new(new_pos.x, old_pos.y, new_pos.z);
    pos.set(test_pos_z);
    if check_voxel_collision(pos, aabb, voxel_world) {
        new_pos.z = old_pos.z;
        vel.z = 0.0;
    }

    // Axe Y
    pos.set(new_pos);
    if check_voxel_collision(pos, aabb, voxel_world) {
        new_pos.y = old_pos.y;
        vel.y = 0.0;
    }

    pos.set(new_pos);
}

/// Vérifie si l'AABB d'une entité entre en collision avec un voxel solide
fn check_voxel_collision(pos: &Position, aabb: &AABB, voxel_world: &VoxelWorld) -> bool {
    let potential_voxels = aabb.get_potential_collisions(pos);
    let (aabb_min, aabb_max) = aabb.bounds(pos);

    for voxel_pos in potential_voxels {
        // Récupérer le voxel à cette position
        let voxel_id = voxel_world.get_voxel_opt(voxel_pos.x, voxel_pos.y, voxel_pos.z);

        if let Some(vid) = voxel_id {
            if vid == 0 {
                continue; // Air, pas de collision
            }

            // Vérifier si ce bloc est solide
            let registry = voxel_world.registry();

            let is_solid = match registry.get(vid) {
                Some(v) => v.collidable,
                None => false,
            };

            if is_solid {
                // Il y a un bloc solide, vérifier l'intersection AABB
                let block_min = Vec3::new(
                    voxel_pos.x as f32,
                    voxel_pos.y as f32,
                    voxel_pos.z as f32,
                );
                let block_max = block_min + Vec3::new(1.0, 1.0, 1.0);

                // Test d'intersection AABB
                if aabb_min.x < block_max.x && aabb_max.x > block_min.x
                    && aabb_min.y < block_max.y && aabb_max.y > block_min.y
                    && aabb_min.z < block_max.z && aabb_max.z > block_min.z
                {
                    return true; // Collision détectée
                }
            }
        }
    }

    false
}

/// Système de caméra: met à jour la caméra pour suivre le joueur
/// Retourne (position, yaw, pitch) de la caméra
pub fn camera_sync_system(
    world: &World,
    player_entity: hecs::Entity,
) -> Option<(Vec3, f32, f32)> {
    let entity_ref = world.entity(player_entity).ok()?;
    let pos = entity_ref.get::<&Position>()?;
    let look_dir = entity_ref.get::<&LookDirection>()?;

    // Position de la caméra: légèrement au-dessus du centre de l'entité
    // Pour un joueur, on place les "yeux" à 1.6m du sol
    // Le centre du joueur est à y = pos.y, donc les yeux sont à pos.y + 0.7
    let eye_height = 0.7;
    let camera_pos = pos.as_vec3() + Vec3::new(0.0, eye_height, 0.0);

    Some((camera_pos, look_dir.yaw, look_dir.pitch))
}

/// Système de saut: gère le saut quand le joueur appuie sur la touche
pub fn jump_system(world: &mut World, input: &InputState, _voxel_world: &VoxelWorld) {
    for (vel, physics, controlled) in world.query_mut::<(&mut Velocity, &mut PhysicsAffected, &PlayerControlled)>() {
        // Ne sauter que si on est au sol et qu'on appuie sur saut
        // En mode fly, MoveUp/MoveDown sont gérés par le système d'input
        if controlled.is_flying() {
            continue;
        }

        if physics.on_ground && input.is_held(GameAction::MoveUp) {
            vel.y = physics.jump_velocity();
            physics.on_ground = false;
        }
    }
}
