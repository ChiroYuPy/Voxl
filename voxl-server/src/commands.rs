//! Built-in server commands
//!
//! Commands are executed server-side. The client sends raw command strings
//! and receives responses with rich formatting.

use voxl_common::{
    Command, CommandContext, CommandResult, TabCompleteSuggestion,
    chat::{ChatMessage, ChatComponent, ChatColor},
    entities::{GameMode, PlayerControlled, Position},
    args,
};
use glam::Vec3;

// ============================================================================
// Built-in Commands
// ============================================================================

/// Displays help for all commands
pub struct HelpCommand;

impl Command for HelpCommand {
    fn name(&self) -> &'static str {
        "help"
    }

    fn description(&self) -> &'static str {
        "Shows available commands"
    }

    fn execute(&self, _args: &[&str], _ctx: &CommandContext) -> CommandResult {
        // Build a rich chat message with multiple components
        let components = vec![
            ChatComponent::text("Available commands:\n").color(ChatColor::Gold).bold(),
            ChatComponent::text("/help").color(ChatColor::Yellow),
            ChatComponent::text(" - Shows this help message\n").color(ChatColor::White),
            ChatComponent::text("/tp <x> <y> <z>").color(ChatColor::Yellow),
            ChatComponent::text(" - Teleport to coordinates\n").color(ChatColor::White),
            ChatComponent::text("/tp ~<x> ~<y> ~<z>").color(ChatColor::Yellow),
            ChatComponent::text(" - Teleport relatively\n").color(ChatColor::White),
            ChatComponent::text("/pos").color(ChatColor::Yellow),
            ChatComponent::text(" - Show your position\n").color(ChatColor::White),
            ChatComponent::text("/gamemode <creative|spectator>").color(ChatColor::Yellow),
            ChatComponent::text(" - Change game mode\n").color(ChatColor::White),
            ChatComponent::text("/gm <c|s>").color(ChatColor::Yellow),
            ChatComponent::text(" - Game mode shortcut\n").color(ChatColor::White),
            ChatComponent::text("/fly").color(ChatColor::Yellow),
            ChatComponent::text(" - Toggle fly mode (creative only)").color(ChatColor::White),
        ];
        CommandResult::Success(ChatMessage::multiple(components))
    }
}

/// Teleport command
pub struct TpCommand;

impl Command for TpCommand {
    fn name(&self) -> &'static str {
        "tp"
    }

    fn description(&self) -> &'static str {
        "Teleport to coordinates"
    }

    fn usage(&self) -> &'static str {
        "<x> <y> <z> | ~<x> ~<y> ~<z>"
    }

    fn execute(&self, args: &[&str], ctx: &CommandContext) -> CommandResult {
        let current_pos = ctx.get_executor_position().unwrap_or(Vec3::ZERO);

        let target_pos = match args::parse_coords(args, current_pos) {
            Ok(pos) => pos,
            Err(e) => {
                return CommandResult::Error(
                    ChatMessage::multiple(vec![
                        ChatComponent::text("Invalid coordinates: ").color(ChatColor::Red),
                        ChatComponent::text(&e).color(ChatColor::White),
                    ])
                )
            }
        };

        // Update player entity position
        if let Some(entity) = ctx.executor_entity {
            if let Ok(mut entities) = ctx.entities.write() {
                if let Ok(mut pos) = entities.ecs_world.query_one_mut::<&mut Position>(entity) {
                    pos.set(target_pos);
                    return CommandResult::Success(
                        ChatMessage::multiple(vec![
                            ChatComponent::text("Teleported to ").color(ChatColor::Green),
                            ChatComponent::text(format!("({:.1}, {:.1}, {:.1})",
                                target_pos.x, target_pos.y, target_pos.z)
                            ).color(ChatColor::White),
                        ])
                    );
                }
            }
        }

        CommandResult::Error(
            ChatMessage::Single(
                ChatComponent::text("Failed to teleport - entity not found")
                    .color(ChatColor::Red)
            )
        )
    }

    fn tab_complete(&self, args: &[&str], ctx: &CommandContext) -> Vec<TabCompleteSuggestion> {
        if args.len() <= 3 {
            // Suggest coordinate formats
            let _current = ctx.get_executor_position().unwrap_or(Vec3::ZERO);
            match args.len() {
                0 => vec![TabCompleteSuggestion::new("~")],
                1 => vec![TabCompleteSuggestion::new("~")],
                2 => vec![TabCompleteSuggestion::new("~")],
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        }
    }
}

/// Show current position
pub struct PosCommand;

impl Command for PosCommand {
    fn name(&self) -> &'static str {
        "pos"
    }

    fn description(&self) -> &'static str {
        "Show your current position"
    }

    fn execute(&self, _args: &[&str], ctx: &CommandContext) -> CommandResult {
        match ctx.get_executor_position() {
            Some(pos) => CommandResult::Success(
                ChatMessage::multiple(vec![
                    ChatComponent::text("Position: ").color(ChatColor::Yellow),
                    ChatComponent::text(format!("X: {:.1}, ", pos.x)).color(ChatColor::White),
                    ChatComponent::text(format!("Y: {:.1}, ", pos.y)).color(ChatColor::White),
                    ChatComponent::text(format!("Z: {:.1}", pos.z)).color(ChatColor::White),
                ])
            ),
            None => CommandResult::Error(
                ChatMessage::Single(
                    ChatComponent::text("Position unknown").color(ChatColor::Red)
                )
            ),
        }
    }
}

/// Game mode command
pub struct GamemodeCommand;

impl Command for GamemodeCommand {
    fn name(&self) -> &'static str {
        "gamemode"
    }

    fn description(&self) -> &'static str {
        "Change game mode"
    }

    fn aliases(&self) -> &'static [&'static str] {
        &["gm"]
    }

    fn usage(&self) -> &'static str {
        "<creative|spectator> | <c|s>"
    }

    fn execute(&self, args: &[&str], ctx: &CommandContext) -> CommandResult {
        if args.is_empty() {
            return CommandResult::Error(
                ChatMessage::multiple(vec![
                    ChatComponent::text("Usage: ").color(ChatColor::Red),
                    ChatComponent::text("/gamemode <creative|spectator>").color(ChatColor::Yellow),
                ])
            );
        }

        let mode = match args[0].to_lowercase().as_str() {
            "creative" | "c" => GameMode::Creative { fly_enabled: true },
            "spectator" | "s" => GameMode::Spectator,
            _ => {
                return CommandResult::Error(
                    ChatMessage::multiple(vec![
                        ChatComponent::text("Unknown game mode: ").color(ChatColor::Red),
                        ChatComponent::text(args[0]).color(ChatColor::White),
                        ChatComponent::text(". Available: ").color(ChatColor::Gray),
                        ChatComponent::text("creative").color(ChatColor::Green),
                        ChatComponent::text(", ").color(ChatColor::Gray),
                        ChatComponent::text("spectator").color(ChatColor::Green),
                    ])
                )
            }
        };

        if let Some(entity) = ctx.executor_entity {
            if let Ok(mut entities) = ctx.entities.write() {
                if let Ok(mut controlled) = entities.ecs_world.query_one_mut::<&mut PlayerControlled>(entity) {
                    let old_mode = *controlled.get_game_mode();
                    controlled.set_game_mode(mode);
                    return CommandResult::Success(
                        ChatMessage::multiple(vec![
                            ChatComponent::text("Game mode changed from ").color(ChatColor::Green),
                            ChatComponent::text(old_mode.name()).color(ChatColor::Yellow),
                            ChatComponent::text(" to ").color(ChatColor::Green),
                            ChatComponent::text(mode.name()).color(ChatColor::Yellow),
                        ])
                    );
                }
            }
        }

        CommandResult::Error(
            ChatMessage::Single(
                ChatComponent::text("Failed to change game mode - entity not found")
                    .color(ChatColor::Red)
            )
        )
    }

    fn tab_complete(&self, args: &[&str], _ctx: &CommandContext) -> Vec<TabCompleteSuggestion> {
        if args.len() == 1 {
            vec![
                TabCompleteSuggestion::with_tooltip("creative", "Creative mode with flying"),
                TabCompleteSuggestion::with_tooltip("spectator", "Spectator mode (fly through walls)"),
                TabCompleteSuggestion::with_tooltip("c", "Creative mode shortcut"),
                TabCompleteSuggestion::with_tooltip("s", "Spectator mode shortcut"),
            ]
        } else {
            Vec::new()
        }
    }
}

/// Toggle fly command
pub struct FlyCommand;

impl Command for FlyCommand {
    fn name(&self) -> &'static str {
        "fly"
    }

    fn description(&self) -> &'static str {
        "Toggle fly mode"
    }

    fn execute(&self, _args: &[&str], ctx: &CommandContext) -> CommandResult {
        if let Some(entity) = ctx.executor_entity {
            if let Ok(mut entities) = ctx.entities.write() {
                if let Ok(mut controlled) = entities.ecs_world.query_one_mut::<&mut PlayerControlled>(entity) {
                    // Only allow flying in creative mode
                    if !matches!(controlled.get_game_mode(), GameMode::Creative { .. }) {
                        return CommandResult::Error(
                            ChatMessage::Single(
                                ChatComponent::text("Fly mode is only available in creative mode")
                                    .color(ChatColor::Red)
                            )
                        );
                    }

                    let now_flying = !controlled.is_flying();
                    controlled.toggle_fly();

                    let (status, color) = if now_flying {
                        ("enabled", ChatColor::Green)
                    } else {
                        ("disabled", ChatColor::Yellow)
                    };

                    return CommandResult::Success(
                        ChatMessage::multiple(vec![
                            ChatComponent::text("Fly mode ").color(ChatColor::White),
                            ChatComponent::text(status).color(color),
                        ])
                    );
                }
            }
        }

        CommandResult::Error(
            ChatMessage::Single(
                ChatComponent::text("Failed to toggle fly - entity not found")
                    .color(ChatColor::Red)
            )
        )
    }
}
