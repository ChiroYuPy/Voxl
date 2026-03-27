use voxl_common::{
    Command, CommandContext, CommandResult, TabCompleteSuggestion,
    chat::{ChatMessage, ChatComponent},
    entities::{GameMode, PlayerControlled, Position},
    network::ClientAction,
    args,
};
use glam::Vec3;

pub struct HelpCommand;

impl Command for HelpCommand {
    fn name(&self) -> &'static str {
        "help"
    }

    fn description(&self) -> &'static str {
        "Shows available commands"
    }

    fn execute(&self, _args: &[&str], _ctx: &CommandContext) -> CommandResult {
        CommandResult::Success(ChatMessage {
            components: vec![
                ChatComponent::text("Available commands:\n").color("#FFAA00"),
                ChatComponent::text("/help").color("#FFFF55"),
                ChatComponent::text(" - Shows this help message\n").color("#FFFFFF"),
                ChatComponent::text("/tp <x> <y> <z>").color("#FFFF55"),
                ChatComponent::text(" - Teleport to coordinates\n").color("#FFFFFF"),
                ChatComponent::text("/tp ~<x> ~<y> ~<z>").color("#FFFF55"),
                ChatComponent::text(" - Teleport relatively\n").color("#FFFFFF"),
                ChatComponent::text("/pos").color("#FFFF55"),
                ChatComponent::text(" - Show your position\n").color("#FFFFFF"),
                ChatComponent::text("/gamemode <creative|spectator>").color("#FFFF55"),
                ChatComponent::text(" - Change game mode\n").color("#FFFFFF"),
                ChatComponent::text("/gm <c|s>").color("#FFFF55"),
                ChatComponent::text(" - Game mode shortcut\n").color("#FFFFFF"),
                ChatComponent::text("/fly").color("#FFFF55"),
                ChatComponent::text(" - Toggle fly mode (creative only)").color("#FFFFFF"),
            ],
        })
    }
}

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
                return CommandResult::Error(ChatMessage {
                    components: vec![
                        ChatComponent::text("Invalid coordinates: ").color("#FF5555"),
                        ChatComponent::text(&e).color("#FFFFFF"),
                    ],
                })
            }
        };

        let is_relative = args.iter().take(3).any(|s| s.starts_with('~'));

        if is_relative {
            let msg = ChatMessage {
                components: vec![
                    ChatComponent::text("Teleported relatively to (").color("#55FF55"),
                    ChatComponent::text(format!("{:.1}, {:.1}, {:.1})",
                        target_pos.x, target_pos.y, target_pos.z)
                    ).color("#FFFFFF"),
                ],
            };
            CommandResult::with_action_msg(msg, ClientAction::TeleportRelative {
                dx: target_pos.x - current_pos.x,
                dy: target_pos.y - current_pos.y,
                dz: target_pos.z - current_pos.z,
            })
        } else {
            let msg = ChatMessage {
                components: vec![
                    ChatComponent::text("Teleported to ").color("#55FF55"),
                    ChatComponent::text(format!("({:.1}, {:.1}, {:.1})",
                        target_pos.x, target_pos.y, target_pos.z)
                    ).color("#FFFFFF"),
                ],
            };
            CommandResult::with_action_msg(msg, ClientAction::Teleport {
                x: target_pos.x,
                y: target_pos.y,
                z: target_pos.z,
            })
        }
    }

    fn tab_complete(&self, args: &[&str], ctx: &CommandContext) -> Vec<TabCompleteSuggestion> {
        if args.len() <= 3 {
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
            Some(pos) => CommandResult::Success(ChatMessage {
                components: vec![
                    ChatComponent::text("Position: ").color("#FFFF55"),
                    ChatComponent::text(format!("X: {:.1}, ", pos.x)).color("#FFFFFF"),
                    ChatComponent::text(format!("Y: {:.1}, ", pos.y)).color("#FFFFFF"),
                    ChatComponent::text(format!("Z: {:.1}", pos.z)).color("#FFFFFF"),
                ],
            }),
            None => CommandResult::Error(ChatMessage {
                components: vec![
                    ChatComponent::text("Position unknown").color("#FF5555")
                ],
            }),
        }
    }
}

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
            return CommandResult::Error(ChatMessage {
                components: vec![
                    ChatComponent::text("Usage: ").color("#FF5555"),
                    ChatComponent::text("/gamemode <creative|spectator>").color("#FFFF55"),
                ],
            });
        }

        let mode = match args[0].to_lowercase().as_str() {
            "creative" | "c" => GameMode::Creative { fly_enabled: true },
            "spectator" | "s" => GameMode::Spectator,
            _ => {
                return CommandResult::Error(ChatMessage {
                    components: vec![
                        ChatComponent::text("Unknown game mode: ").color("#FF5555"),
                        ChatComponent::text(args[0]).color("#FFFFFF"),
                        ChatComponent::text(". Available: ").color("#AAAAAA"),
                        ChatComponent::text("creative").color("#55FF55"),
                        ChatComponent::text(", ").color("#AAAAAA"),
                        ChatComponent::text("spectator").color("#55FF55"),
                    ],
                })
            }
        };

        let old_mode = if let Some(entity) = ctx.executor_entity {
            if let Ok(entities) = ctx.entities.read() {
                entities.ecs_world.query_one::<&PlayerControlled>(entity)
                    .get()
                    .map(|c| *c.get_game_mode())
                    .unwrap_or(GameMode::Spectator)
            } else {
                return CommandResult::Error(ChatMessage {
                    components: vec![
                        ChatComponent::text("Failed to access entity world").color("#FF5555")
                    ],
                });
            }
        } else {
            return CommandResult::Error(ChatMessage {
                components: vec![
                    ChatComponent::text("Failed to change game mode - entity not found").color("#FF5555")
                ],
            });
        };

        let msg = ChatMessage {
            components: vec![
                ChatComponent::text("Game mode changed from ").color("#55FF55"),
                ChatComponent::text(old_mode.name()).color("#FFFF55"),
                ChatComponent::text(" to ").color("#55FF55"),
                ChatComponent::text(mode.name()).color("#FFFF55"),
            ],
        };

        use voxl_common::network::GameModeData;
        CommandResult::with_action_msg(msg, ClientAction::SetGameMode {
            mode: GameModeData::from(&mode),
        })
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
            if let Ok(entities) = ctx.entities.read() {
                let can_fly = entities.ecs_world.query_one::<&PlayerControlled>(entity)
                    .get()
                    .map(|c| matches!(c.get_game_mode(), GameMode::Creative { .. }))
                    .unwrap_or(false);

                if !can_fly {
                    return CommandResult::Error(ChatMessage {
                        components: vec![
                            ChatComponent::text("Fly mode is only available in creative mode").color("#FF5555")
                        ],
                    });
                }

                return CommandResult::with_action("", ClientAction::ToggleFly);
            }
        }

        CommandResult::Error(ChatMessage {
            components: vec![
                ChatComponent::text("Failed to toggle fly - entity not found").color("#FF5555")
            ],
        })
    }
}
