//! Player management
//!
//! Handles player entities, spawning, and data.

use voxl_common::{
    entities::{EntityWorld, Position, Velocity, LookDirection, PlayerControlled, PhysicsAffected, AABB, Name},
    network::PlayerId,
};
use tracing::info;
use std::sync::{Arc, RwLock};
use glam::Vec3;
use hecs::Entity;

/// Player data with entity reference
#[derive(Clone)]
pub struct ServerPlayer {
    pub player_id: PlayerId,
    pub username: String,
    pub entity: Option<Entity>,
}

impl ServerPlayer {
    pub fn new(player_id: PlayerId, username: String) -> Self {
        Self {
            player_id,
            username,
            entity: None,
        }
    }

    /// Gets the player's current position
    pub fn get_position(&self, entities: &EntityWorld) -> Option<Vec3> {
        if let Some(entity) = self.entity {
            let entity_ref = entities.ecs_world.entity(entity).ok()?;
            entity_ref.get::<&Position>().map(|p| p.as_vec3())
        } else {
            None
        }
    }
}

/// Spawns a new player entity in the ECS
pub fn spawn_player_entity(
    entities: &Arc<RwLock<EntityWorld>>,
    player_id: PlayerId,
    username: &str,
) -> Entity {
    let mut entities = entities.write().unwrap();

    // Spawn at Y=80 (above terrain)
    let spawn_pos = Vec3::new(0.0, 80.0, 0.0);

    // Create entity with all required components
    let entity = entities.ecs_world.spawn((
        Position::new(spawn_pos),
        Velocity::new(Vec3::ZERO),
        LookDirection::new(),
        PlayerControlled::new(),
        PhysicsAffected::new(),
        AABB::player_size(),
        Name::new(username.to_string()),
    ));

    info!("[Player] Spawned entity for '{}' (ID: {}) at ({:.1}, {:.1}, {:.1})",
        username, player_id, spawn_pos.x, spawn_pos.y, spawn_pos.z);

    entity
}

/// Removes a player entity from the ECS
pub fn despawn_player_entity(
    entities: &Arc<RwLock<EntityWorld>>,
    entity: Entity,
    username: &str,
) {
    let mut entities = entities.write().unwrap();

    match entities.ecs_world.despawn(entity) {
        Ok(_) => {
            info!("[Player] Despawned entity {:?} for player '{}'", entity, username);
        }
        Err(e) => {
            tracing::warn!("[Player] Failed to despawn entity {:?} for '{}': {}",
                entity, username, e);
        }
    }
}
