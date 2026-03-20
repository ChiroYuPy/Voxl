pub mod components;
pub mod systems;

pub use components::*;
pub use systems::*;

use hecs::World;
use glam::Vec3;

/// Monde des entités (ECS)
pub struct EntityWorld {
    ecs_world: World,
    player_entity: Option<hecs::Entity>,
}

impl EntityWorld {
    pub fn new() -> Self {
        Self {
            ecs_world: World::new(),
            player_entity: None,
        }
    }

    /// Crée le joueur et retourne son entity
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

    pub fn player_entity(&self) -> Option<hecs::Entity> {
        self.player_entity
    }

    pub fn world(&mut self) -> &mut World {
        &mut self.ecs_world
    }

    pub fn world_read(&self) -> &World {
        &self.ecs_world
    }

    /// Change le mode de jeu du joueur
    pub fn set_game_mode(&mut self, mode: GameMode) -> bool {
        if let Some(player_entity) = self.player_entity {
            if let Ok(controlled) = self.ecs_world.query_one_mut::<&mut PlayerControlled>(player_entity) {
                println!("[GameMode] Changement: {} -> {}", controlled.get_game_mode().name(), mode.name());
                controlled.set_game_mode(mode);
                return true;
            }
        }
        false
    }

    /// Toggle le fly mode du joueur (seulement en mode créatif)
    pub fn toggle_fly(&mut self) -> bool {
        if let Some(player_entity) = self.player_entity {
            if let Ok(controlled) = self.ecs_world.query_one_mut::<&mut PlayerControlled>(player_entity) {
                let old_fly = controlled.is_flying();
                controlled.toggle_fly();
                let new_fly = controlled.is_flying();
                println!("[Fly] Toggle: {} -> {}", old_fly, new_fly);
                return true;
            }
        }
        false
    }

    /// Retourne le mode de jeu actuel du joueur
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
