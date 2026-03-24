//! Command dispatcher type declaration
//!
//! This file only declares the CommandDispatcher type to avoid circular dependencies.
//! The implementation is in lib.rs.

use std::sync::{Arc, RwLock};
use voxl_common::{
    Command, CommandResult,
    PlayerId, VoxelWorld, EntityWorld, SharedVoxelRegistry, ServerSettings,
};
use hecs::Entity;

/// Command dispatcher - manages and executes commands
///
/// This type is declared separately to avoid circular dependencies.
pub struct CommandDispatcher {
    commands: Vec<Box<dyn Command>>,
}

impl CommandDispatcher {
    /// Creates a new command dispatcher
    ///
    /// Note: This creates an empty dispatcher. Use `with_defaults()` to get
    /// a dispatcher with all built-in commands registered.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Creates a new dispatcher with all built-in commands registered
    pub fn with_defaults() -> Self {
        use crate::commands::{HelpCommand, TpCommand, PosCommand, GamemodeCommand, FlyCommand};

        let mut dispatcher = Self {
            commands: Vec::new(),
        };
        dispatcher.register_command(HelpCommand);
        dispatcher.register_command(TpCommand);
        dispatcher.register_command(PosCommand);
        dispatcher.register_command(GamemodeCommand);
        dispatcher.register_command(FlyCommand);
        dispatcher
    }

    /// Registers a command
    pub fn register_command<C: Command + 'static>(&mut self, command: C) {
        self.commands.push(Box::new(command));
    }

    /// Dispatches a command string to the appropriate command handler
    pub fn dispatch(
        &self,
        command: &str,
        executor_id: PlayerId,
        executor_username: &str,
        executor_entity: Option<Entity>,
        world: &Arc<RwLock<VoxelWorld>>,
        entities: &Arc<RwLock<EntityWorld>>,
        registry: &SharedVoxelRegistry,
        settings: &ServerSettings,
        players: &[(PlayerId, String)],
    ) -> CommandResult {
        use voxl_common::CommandContext;
        use tracing::info;

        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() || !parts[0].starts_with('/') {
            return CommandResult::Error(
                voxl_common::ChatMessage::text("Commands must start with /")
            );
        }

        let cmd_name = &parts[0][1..]; // Remove the '/'
        let args = &parts[1..];

        // Find the command
        for cmd in &self.commands {
            if cmd.name() == cmd_name || cmd.aliases().contains(&cmd_name) {
                let ctx = CommandContext::new(
                    executor_id,
                    executor_username,
                    executor_entity,
                    world,
                    entities,
                    registry,
                    settings,
                    players,
                );

                info!("[Command] '{}' executed /{} by '{}'", cmd.name(), cmd_name, executor_username);
                return cmd.execute(args, &ctx);
            }
        }

        CommandResult::Error(
            voxl_common::ChatMessage::text(format!(
                "Unknown command: /{}. Type /help for available commands.",
                cmd_name
            ))
        )
    }
}
