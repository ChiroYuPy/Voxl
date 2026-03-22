pub mod components;
pub mod systems;

pub use components::*;
pub use systems::*;

use hecs::World;
use glam::Vec3;
use tracing::info;

/// Entity world (ECS) - shared between client and server
pub struct EntityWorld {
    pub ecs_world: World,
    pub player_entity: Option<hecs::Entity>,
}

impl EntityWorld {
    /// Creates a new entity world
    pub fn new() -> Self {
        Self {
            ecs_world: World::new(),
            player_entity: None,
        }
    }

    /// Spawns the player entity at the given position
    pub fn spawn_player(&mut self, position: Vec3) -> hecs::Entity {
        let mut controlled = PlayerControlled::default();
        controlled.set_game_mode(GameMode::Spectator);

        let entity = self.ecs_world.spawn((
            Position::new(position),
            Velocity::default(),
            controlled,
            PhysicsAffected::default(),
            AABB::player_size(),
            LookDirection::default(),
        ));
        self.player_entity = Some(entity);
        entity
    }

    /// Returns the player entity if it exists
    pub fn player_entity(&self) -> Option<hecs::Entity> {
        self.player_entity
    }

    /// Returns mutable access to the ECS world
    pub fn world(&mut self) -> &mut World {
        &mut self.ecs_world
    }

    /// Returns immutable access to the ECS world
    pub fn world_read(&self) -> &World {
        &self.ecs_world
    }

    /// Changes the player's game mode
    pub fn set_game_mode(&mut self, mode: GameMode) -> bool {
        if let Some(player_entity) = self.player_entity {
            if let Ok(controlled) = self.ecs_world.query_one_mut::<&mut PlayerControlled>(player_entity) {
                info!("[GameMode] Changed: {} -> {}", controlled.get_game_mode().name(), mode.name());
                controlled.set_game_mode(mode);
                return true;
            }
        }
        false
    }

    /// Toggles fly mode (only works in creative mode)
    pub fn toggle_fly(&mut self) -> bool {
        if let Some(player_entity) = self.player_entity {
            if let Ok(controlled) = self.ecs_world.query_one_mut::<&mut PlayerControlled>(player_entity) {
                let old_fly = controlled.is_flying();
                controlled.toggle_fly();
                let new_fly = controlled.is_flying();
                info!("[Fly] Toggle: {} -> {}", old_fly, new_fly);
                return true;
            }
        }
        false
    }

    /// Returns the current game mode of the player
    pub fn get_game_mode(&self) -> Option<GameMode> {
        if let Some(player_entity) = self.player_entity {
            let entity_ref = self.ecs_world.entity(player_entity).ok()?;
            let controlled = entity_ref.get::<&PlayerControlled>()?;
            return Some(*controlled.get_game_mode());
        }
        None
    }
}

impl Default for EntityWorld {
    fn default() -> Self {
        Self::new()
    }
}
