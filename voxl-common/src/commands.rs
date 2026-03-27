//! Server-side command system
//!
//! This module provides a trait-based command system where commands are
//! executed on the server side. The client only sends the raw command string
//! and receives responses.

use std::sync::{Arc, RwLock};
use crate::{PlayerId, VoxelWorld, EntityWorld, SharedVoxelRegistry, ServerSettings, network::ClientAction};
use crate::chat::ChatMessage;
use glam::Vec3;
use hecs::Entity;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq)]
pub enum CommandResult {
    /// Command executed successfully with a message to display
    Success(ChatMessage),
    /// Command failed with an error message
    Error(ChatMessage),
    /// Silent command (no output)
    None,
    /// Success with a client action to perform (teleport, gamemode change, etc.)
    SuccessWithAction(ChatMessage, ClientAction),
}

impl CommandResult {
    /// Returns true if the command succeeded
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success(_) | Self::None | Self::SuccessWithAction(_, _))
    }

    /// Returns true if the command resulted in an error
    pub fn is_error(&self) -> bool {
        matches!(self, Self::Error(_))
    }

    /// Gets the message if any
    pub fn get_message(&self) -> Option<&ChatMessage> {
        match self {
            Self::Success(msg) => Some(msg),
            Self::Error(msg) => Some(msg),
            Self::SuccessWithAction(msg, _) => Some(msg),
            Self::None => None,
        }
    }

    /// Gets the client action if any
    pub fn get_action(&self) -> Option<&ClientAction> {
        match self {
            Self::SuccessWithAction(_, action) => Some(action),
            _ => None,
        }
    }

    /// Creates a success result from a plain string
    pub fn ok(msg: impl Into<String>) -> Self {
        Self::Success(ChatMessage::text(msg.into()))
    }

    /// Creates an error result from a plain string
    pub fn err(msg: impl Into<String>) -> Self {
        Self::Error(ChatMessage::text(msg.into()))
    }

    /// Creates a success result with a client action
    pub fn with_action(msg: impl Into<String>, action: ClientAction) -> Self {
        Self::SuccessWithAction(ChatMessage::text(msg.into()), action)
    }

    /// Creates a success result with a client action (using ChatMessage)
    pub fn with_action_msg(msg: ChatMessage, action: ClientAction) -> Self {
        Self::SuccessWithAction(msg, action)
    }
}

/// Suggestion for tab completion
#[derive(Debug, Clone, PartialEq)]
pub struct TabCompleteSuggestion {
    /// The suggested text to insert
    pub suggestion: String,
    /// Optional tooltip/hint text
    pub tooltip: Option<String>,
}

impl TabCompleteSuggestion {
    /// Creates a new suggestion without tooltip
    pub fn new(suggestion: impl Into<String>) -> Self {
        Self {
            suggestion: suggestion.into(),
            tooltip: None,
        }
    }

    /// Creates a new suggestion with tooltip
    pub fn with_tooltip(suggestion: impl Into<String>, tooltip: impl Into<String>) -> Self {
        Self {
            suggestion: suggestion.into(),
            tooltip: Some(tooltip.into()),
        }
    }
}

/// Context provided to commands when executing
///
/// This struct gives commands access to server resources they might need.
pub struct CommandContext<'a> {
    /// ID of the player executing the command
    pub executor_id: PlayerId,
    /// Username of the player executing the command
    pub executor_username: &'a str,
    /// Entity of the player executing the command (if available)
    pub executor_entity: Option<Entity>,
    /// The voxel world
    pub world: &'a Arc<RwLock<VoxelWorld>>,
    /// The entity world (ECS)
    pub entities: &'a Arc<RwLock<EntityWorld>>,
    /// The voxel registry (blocks)
    pub registry: &'a SharedVoxelRegistry,
    /// Server settings
    pub settings: &'a ServerSettings,
    /// List of all connected players (player_id, username)
    pub players: &'a [(PlayerId, String)],
}

impl<'a> CommandContext<'a> {
    /// Creates a new command context
    pub fn new(
        executor_id: PlayerId,
        executor_username: &'a str,
        executor_entity: Option<Entity>,
        world: &'a Arc<RwLock<VoxelWorld>>,
        entities: &'a Arc<RwLock<EntityWorld>>,
        registry: &'a SharedVoxelRegistry,
        settings: &'a ServerSettings,
        players: &'a [(PlayerId, String)],
    ) -> Self {
        Self {
            executor_id,
            executor_username,
            executor_entity,
            world,
            entities,
            registry,
            settings,
            players,
        }
    }

    /// Gets the position of the executor if available
    pub fn get_executor_position(&self) -> Option<Vec3> {
        if let Some(entity) = self.executor_entity {
            let entities = self.entities.read().ok()?;
            let entity_ref = entities.ecs_world.entity(entity).ok()?;
            let pos = entity_ref.get::<&crate::entities::Position>()?;
            Some(pos.as_vec3())
        } else {
            None
        }
    }

    /// Finds a player by username (partial match allowed)
    pub fn find_player(&self, name: &str) -> Option<(PlayerId, &str)> {
        let name_lower = name.to_lowercase();
        self.players
            .iter()
            .find(|(_, username)| {
                username.to_lowercase() == name_lower
                    || username.to_lowercase().starts_with(&name_lower)
            })
            .map(|(id, name)| (*id, name.as_str()))
    }

    /// Gets all player usernames
    pub fn get_player_names(&self) -> Vec<&str> {
        self.players.iter().map(|(_, name)| name.as_str()).collect()
    }

    /// Gets all block IDs/names from registry
    pub fn get_block_names(&self) -> Vec<String> {
        // This would require access to the registry's definitions
        // For now, return empty vec - can be implemented later
        Vec::new()
    }
}

/// Trait that all commands must implement
pub trait Command: Send + Sync {
    /// Returns the name of the command (without the '/')
    fn name(&self) -> &'static str;

    /// Returns a short description of the command
    fn description(&self) -> &'static str;

    /// Returns the usage syntax (e.g., "<target> <amount>")
    fn usage(&self) -> &'static str {
        ""
    }

    /// Returns all aliases for this command
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    /// Executes the command with the given arguments
    ///
    /// # Arguments
    /// * `args` - The command arguments (without the command name itself)
    /// * `ctx` - The command context providing access to server resources
    fn execute(&self, args: &[&str], ctx: &CommandContext) -> CommandResult;

    /// Provides tab complete suggestions for the given arguments
    ///
    /// The default implementation returns no suggestions.
    ///
    /// # Arguments
    /// * `args` - The current arguments (partial)
    /// * `ctx` - The command context
    fn tab_complete(&self, args: &[&str], ctx: &CommandContext) -> Vec<TabCompleteSuggestion> {
        let _ = args;
        let _ = ctx;
        Vec::new()
    }
}

/// Helper macro to parse common argument types
pub mod args {
    use super::*;

    /// Parses a player name/selector from an argument
    pub fn parse_player(arg: &str, ctx: &CommandContext) -> Result<PlayerId, String> {
        if arg == "@s" || arg == "@p" {
            // Self selector
            Ok(ctx.executor_id)
        } else if let Some((id, _)) = ctx.find_player(arg) {
            Ok(id)
        } else {
            Err(format!("Player not found: {}", arg))
        }
    }

    /// Parses a float coordinate (supports relative ~ notation)
    pub fn parse_coord(arg: &str, current: f32) -> Result<f32, String> {
        if arg.starts_with('~') {
            let rest = &arg[1..];
            if rest.is_empty() {
                Ok(current)
            } else {
                rest.parse::<f32>()
                    .map(|v| current + v)
                    .map_err(|_| format!("Invalid coordinate: {}", arg))
            }
        } else {
            arg.parse::<f32>()
                .map_err(|_| format!("Invalid coordinate: {}", arg))
        }
    }

    /// Parses a 3D coordinate set (supports relative ~ notation)
    pub fn parse_coords(args: &[&str], current: Vec3) -> Result<Vec3, String> {
        if args.len() < 3 {
            return Err("Expected 3 coordinates".to_string());
        }

        let x = parse_coord(args[0], current.x)?;
        let y = parse_coord(args[1], current.y)?;
        let z = parse_coord(args[2], current.z)?;

        Ok(Vec3::new(x, y, z))
    }

    /// Parses an integer
    pub fn parse_int(arg: &str) -> Result<i32, String> {
        arg.parse::<i32>()
            .map_err(|_| format!("Invalid integer: {}", arg))
    }

    /// Parses a float
    pub fn parse_float(arg: &str) -> Result<f32, String> {
        arg.parse::<f32>()
            .map_err(|_| format!("Invalid number: {}", arg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_coord() {
        assert_eq!(args::parse_coord("10", 0.0), Ok(10.0));
        assert_eq!(args::parse_coord("~", 5.0), Ok(5.0));
        assert_eq!(args::parse_coord("~3", 5.0), Ok(8.0));
        assert!(args::parse_coord("abc", 0.0).is_err());
    }
}
