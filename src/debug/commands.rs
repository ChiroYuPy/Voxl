use glam::Vec3;
use crate::entities::GameMode;

/// Résultat de l'exécution d'une commande
pub enum CommandResult {
    /// Commande exécutée avec succès, message à afficher
    Success(String),
    /// Erreur dans la commande
    Error(String),
    /// Téléportation vers une position
    Teleport(Vec3),
    /// Téléportation relative
    TeleportRelative(Vec3),
    /// Aucun résultat (commande silencieuse)
    None,
    /// Effacer l'historique du chat
    ClearChat,
    /// Changer le mode de jeu
    SetGameMode(GameMode),
    /// Toggle le fly mode
    ToggleFly,
}

/// Parse et exécute une commande
pub fn execute_command(command: &str, current_pos: Vec3) -> CommandResult {
    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        return CommandResult::Error("Commande vide".to_string());
    }

    let cmd = parts[0].to_lowercase();

    match cmd.as_str() {
        "/tp" => handle_tp(&parts, current_pos),
        "/help" => handle_help(),
        "/clear" => handle_clear(),
        "/pos" => handle_pos(current_pos),
        "/gamemode" | "/gm" => handle_gamemode(&parts),
        "/fly" => handle_fly(),
        _ => CommandResult::Error(format!("Commande inconnue: {}. Tapez /help", cmd)),
    }
}

fn handle_tp(parts: &[&str], current_pos: Vec3) -> CommandResult {
    if parts.len() < 4 {
        return CommandResult::Error("Usage: /tp <x> <y> <z> ou /tp ~<x> ~<y> ~<z>".to_string());
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
        None => return CommandResult::Error(format!("Coordonnée X invalide: {}", parts[1])),
    };

    let y = match parse_coord(parts[2]) {
        Some(v) if parts[2].starts_with('~') => current_pos.y + v,
        Some(v) => v,
        None => return CommandResult::Error(format!("Coordonnée Y invalide: {}", parts[2])),
    };

    let z = match parse_coord(parts[3]) {
        Some(v) if parts[3].starts_with('~') => current_pos.z + v,
        Some(v) => v,
        None => return CommandResult::Error(format!("Coordonnée Z invalide: {}", parts[3])),
    };

    if parts[1].starts_with('~') || parts[2].starts_with('~') || parts[3].starts_with('~') {
        CommandResult::TeleportRelative(Vec3::new(x, y, z))
    } else {
        CommandResult::Teleport(Vec3::new(x, y, z))
    }
}

fn handle_help() -> CommandResult {
    let help_text = r#"
§cCommandes disponibles:

§6/tp <x> <y> <z>§r - Téléporte aux coordonnées absolues
§6/tp ~<x> ~<y> ~<z>§r - Téléporte relativement (ex: /tp ~ ~5 ~)
§6/pos§r - Affiche votre position actuelle
§6/gamemode <creative|spectator>§r - Change le mode de jeu
§6/gm <c|s>§r - Raccourci pour gamemode
§6/fly§r - Toggle le mode vol (créatif seulement)
§6/clear§r - Efface l'historique du chat
§6/help§r - Affiche cette aide

§7Exemples:§r
/tp 100 64 200
/tp ~ ~10 ~ (monte de 10 blocs)
/tp ~5 ~ ~ (avance de 5 blocs)
/gamemode creative
/gm spectator
/fly
"#;
    CommandResult::Success(help_text.to_string())
}

fn handle_clear() -> CommandResult {
    CommandResult::ClearChat
}

fn handle_pos(current_pos: Vec3) -> CommandResult {
    CommandResult::Success(format!("Position: X: {:.1}, Y: {:.1}, Z: {:.1}", current_pos.x, current_pos.y, current_pos.z))
}

fn handle_gamemode(parts: &[&str]) -> CommandResult {
    if parts.len() < 2 {
        return CommandResult::Error("Usage: /gamemode <creative|spectator> ou /gm <c|s>".to_string());
    }

    let mode = match parts[1].to_lowercase().as_str() {
        "creative" | "c" => GameMode::Creative { fly_enabled: true },
        "spectator" | "s" => GameMode::Spectator,
        _ => return CommandResult::Error(format!("Mode inconnu: {}. Modes disponibles: creative, spectator", parts[1])),
    };

    CommandResult::SetGameMode(mode)
}

fn handle_fly() -> CommandResult {
    CommandResult::ToggleFly
}
