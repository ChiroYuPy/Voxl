use glam::Vec3;
use voxl_common::entities::GameMode;
use voxl_common::chat::{ChatMessage, ChatComponent, ChatColor};

/// Result of executing a command
pub enum CommandResult {
    /// Command executed successfully, message to display
    Success(ChatMessage),
    /// Error in command
    Error(ChatMessage),
    /// Teleport to a position
    Teleport(Vec3),
    /// Relative teleport
    TeleportRelative(Vec3),
    /// No result (silent command)
    None,
    /// Clear chat history
    ClearChat,
    /// Change game mode
    SetGameMode(GameMode),
    /// Toggle fly mode
    ToggleFly,
}

/// Parse and execute a command
pub fn execute_command(command: &str, current_pos: Vec3) -> CommandResult {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return CommandResult::Error(ChatMessage::single(ChatComponent::text("Empty command").color(ChatColor::Red)));
    }

    let cmd = parts[0].to_lowercase();

    match cmd.as_str() {
        "/tp" => handle_tp(&parts, current_pos),
        "/help" => handle_help(),
        "/clear" => handle_clear(),
        "/pos" => handle_pos(current_pos),
        "/gamemode" | "/gm" => handle_gamemode(&parts),
        "/fly" => handle_fly(),
        _ => CommandResult::Error(ChatMessage::multiple(vec![
            ChatComponent::text("Unknown command: ").color(ChatColor::Red),
            ChatComponent::text(&cmd).color(ChatColor::White),
            ChatComponent::text(". Type /help").color(ChatColor::Gray),
        ])),
    }
}

fn handle_tp(parts: &[&str], current_pos: Vec3) -> CommandResult {
    if parts.len() < 4 {
        return CommandResult::Error(ChatMessage::single(ChatComponent::text("Usage: /tp <x> <y> <z> or /tp ~<x> ~<y> ~<z>").color(ChatColor::Red)));
    }

    let parse_coord = |s: &str| -> Option<f32> {
        if s.starts_with('~') {
            let rest = &s[1..];
            if rest.is_empty() {
                return Some(0.0); // ~ alone means relative offset of 0
            }
            rest.parse::<f32>().ok().map(|v| -v) // Negative because we want ~5 to add 5
        } else {
            s.parse::<f32>().ok()
        }
    };

    let x = match parse_coord(parts[1]) {
        Some(v) if parts[1].starts_with('~') => current_pos.x + v,
        Some(v) => v,
        None => return CommandResult::Error(ChatMessage::multiple(vec![
            ChatComponent::text("Invalid X coordinate: ").color(ChatColor::Red),
            ChatComponent::text(parts[1]).color(ChatColor::White),
        ])),
    };

    let y = match parse_coord(parts[2]) {
        Some(v) if parts[2].starts_with('~') => current_pos.y + v,
        Some(v) => v,
        None => return CommandResult::Error(ChatMessage::multiple(vec![
            ChatComponent::text("Invalid Y coordinate: ").color(ChatColor::Red),
            ChatComponent::text(parts[2]).color(ChatColor::White),
        ])),
    };

    let z = match parse_coord(parts[3]) {
        Some(v) if parts[3].starts_with('~') => current_pos.z + v,
        Some(v) => v,
        None => return CommandResult::Error(ChatMessage::multiple(vec![
            ChatComponent::text("Invalid Z coordinate: ").color(ChatColor::Red),
            ChatComponent::text(parts[3]).color(ChatColor::White),
        ])),
    };

    if parts[1].starts_with('~') || parts[2].starts_with('~') || parts[3].starts_with('~') {
        CommandResult::TeleportRelative(Vec3::new(x, y, z))
    } else {
        CommandResult::Teleport(Vec3::new(x, y, z))
    }
}

fn handle_help() -> CommandResult {
    CommandResult::Success(ChatMessage::multiple(vec![
        ChatComponent::text("Available commands:\n\n").color(ChatColor::Gold),
        ChatComponent::text("/tp <x> <y> <z>").color(ChatColor::Gold),
        ChatComponent::text(" - Teleport to absolute coordinates\n").color(ChatColor::Gray),
        ChatComponent::text("/tp ~<x> ~<y> ~<z>").color(ChatColor::Gold),
        ChatComponent::text(" - Teleport relatively (ex: /tp ~ ~5 ~)\n").color(ChatColor::Gray),
        ChatComponent::text("/pos").color(ChatColor::Gold),
        ChatComponent::text(" - Show your current position\n").color(ChatColor::Gray),
        ChatComponent::text("/gamemode <creative|spectator>").color(ChatColor::Gold),
        ChatComponent::text(" - Change game mode\n").color(ChatColor::Gray),
        ChatComponent::text("/gm <c|s>").color(ChatColor::Gold),
        ChatComponent::text(" - Shortcut for gamemode\n").color(ChatColor::Gray),
        ChatComponent::text("/fly").color(ChatColor::Gold),
        ChatComponent::text(" - Toggle fly mode (creative only)\n").color(ChatColor::Gray),
        ChatComponent::text("/clear").color(ChatColor::Gold),
        ChatComponent::text(" - Clear chat history\n").color(ChatColor::Gray),
        ChatComponent::text("/help").color(ChatColor::Gold),
        ChatComponent::text(" - Show this help\n\n").color(ChatColor::Gray),
        ChatComponent::text("Examples:\n").color(ChatColor::DarkGray),
        ChatComponent::text("/tp 100 64 200\n").color(ChatColor::White),
        ChatComponent::text("/tp ~ ~10 ~").color(ChatColor::White),
        ChatComponent::text(" (go up 10 blocks)\n").color(ChatColor::Gray),
        ChatComponent::text("/tp ~5 ~ ~").color(ChatColor::White),
        ChatComponent::text(" (move forward 5 blocks)\n").color(ChatColor::Gray),
        ChatComponent::text("/gamemode creative\n").color(ChatColor::White),
        ChatComponent::text("/gm spectator\n").color(ChatColor::White),
        ChatComponent::text("/fly\n").color(ChatColor::White),
    ]))
}

fn handle_clear() -> CommandResult {
    CommandResult::ClearChat
}

fn handle_pos(current_pos: Vec3) -> CommandResult {
    CommandResult::Success(ChatMessage::multiple(vec![
        ChatComponent::text("Position: ").color(ChatColor::Green),
        ChatComponent::text(&format!("X: {:.1}, ", current_pos.x)).color(ChatColor::White),
        ChatComponent::text(&format!("Y: {:.1}, ", current_pos.y)).color(ChatColor::White),
        ChatComponent::text(&format!("Z: {:.1}", current_pos.z)).color(ChatColor::White),
    ]))
}

fn handle_gamemode(parts: &[&str]) -> CommandResult {
    if parts.len() < 2 {
        return CommandResult::Error(ChatMessage::single(ChatComponent::text("Usage: /gamemode <creative|spectator> or /gm <c|s>").color(ChatColor::Red)));
    }

    let (mode, mode_name) = match parts[1].to_lowercase().as_str() {
        "creative" | "c" => (GameMode::Creative { fly_enabled: true }, "creative"),
        "spectator" | "s" => (GameMode::Spectator, "spectator"),
        _ => {
            return CommandResult::Error(ChatMessage::multiple(vec![
                ChatComponent::text("Unknown mode: ").color(ChatColor::Red),
                ChatComponent::text(parts[1]).color(ChatColor::White),
                ChatComponent::text(". Available modes: creative, spectator").color(ChatColor::Gray),
            ]))
        }
    };

    CommandResult::Success(ChatMessage::multiple(vec![
        ChatComponent::text("Game mode changed to ").color(ChatColor::Green),
        ChatComponent::text(mode_name).color(ChatColor::Gold),
    ]))
}

fn handle_fly() -> CommandResult {
    CommandResult::ToggleFly
}
