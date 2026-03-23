use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tracing::info;

const USER_CONFIG_FILE: &str = "assets/config/user_config.toml";
const DEFAULT_CONFIG_FILE: &str = "assets/config/config.toml";

/// Graphics settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicsSettings {
    /// Render distance in chunks (radius around player)
    pub render_distance: u32,
    /// Ambient occlusion intensity (0.0 = disabled, 1.0 = maximum)
    pub ao_intensity: f32,
    /// Enable/disable VSync
    pub vsync: bool,
    /// Maximum FPS (0 = unlimited, negative values are clamped to 0). Only used when vsync is disabled.
    pub max_fps: i32,
}

impl Default for GraphicsSettings {
    fn default() -> Self {
        Self {
            render_distance: 8,
            ao_intensity: 0.7,
            vsync: true,
            max_fps: 144,
        }
    }
}

impl GraphicsSettings {
    /// Get effective max FPS (None for unlimited, Some(n) for limited)
    pub fn effective_max_fps(&self) -> Option<u32> {
        if self.vsync {
            None // VSync controls FPS
        } else if self.max_fps <= 0 {
            None // 0 or negative means unlimited
        } else {
            Some(self.max_fps as u32)
        }
    }
}

/// Keybindings configuration
/// Stores bindings as strings for serialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyBindingsConfig {
    /// Mapping from action names to key representations
    /// Format: "MoveForward" -> ["KeyZ", "KeyW"]
    pub bindings: std::collections::HashMap<String, Vec<String>>,
}

impl Default for KeyBindingsConfig {
    fn default() -> Self {
        let mut bindings = std::collections::HashMap::new();

        // Movement
        bindings.insert("MoveForward".to_string(), vec!["KeyZ".to_string(), "KeyW".to_string()]);
        bindings.insert("MoveBackward".to_string(), vec!["KeyS".to_string()]);
        bindings.insert("MoveLeft".to_string(), vec!["KeyQ".to_string(), "KeyA".to_string()]);
        bindings.insert("MoveRight".to_string(), vec!["KeyD".to_string()]);
        bindings.insert("MoveUp".to_string(), vec!["NamedSpace".to_string()]);
        bindings.insert("MoveDown".to_string(), vec!["NamedShift".to_string()]);

        // Mouse - view
        bindings.insert("LookUp".to_string(), vec!["MouseMoveY".to_string()]);
        bindings.insert("LookDown".to_string(), vec!["MouseMoveY".to_string()]);
        bindings.insert("LookLeft".to_string(), vec!["MouseMoveX".to_string()]);
        bindings.insert("LookRight".to_string(), vec!["MouseMoveX".to_string()]);

        // Block interactions
        bindings.insert("BreakBlock".to_string(), vec!["Mouse1".to_string()]);
        bindings.insert("PlaceBlock".to_string(), vec!["Mouse3".to_string()]);
        bindings.insert("PickBlock".to_string(), vec!["Mouse2".to_string()]);
        bindings.insert("NextBlockType".to_string(), vec!["Mouse4".to_string()]);
        bindings.insert("PreviousBlockType".to_string(), vec!["Mouse5".to_string()]);

        // Controls
        bindings.insert("ToggleMouseCapture".to_string(), vec!["NamedEnter".to_string()]);
        bindings.insert("ReleaseMouse".to_string(), vec!["NamedEscape".to_string()]);

        // Speed
        bindings.insert("IncreaseSpeed".to_string(), vec!["NamedControl".to_string()]);
        bindings.insert("DecreaseSpeed".to_string(), vec!["Key-".to_string()]);

        // UI and debug
        bindings.insert("ToggleDebugUI".to_string(), vec!["NamedF3".to_string()]);
        bindings.insert("OpenChat".to_string(), vec!["KeyT".to_string()]);

        // Game modes
        bindings.insert("ToggleFly".to_string(), vec!["KeyF".to_string()]);
        bindings.insert("CycleGameMode".to_string(), vec!["KeyG".to_string()]);
        bindings.insert("ToggleChunkBorders".to_string(), vec!["NamedF6".to_string()]);

        Self { bindings }
    }
}

/// Main game configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameConfig {
    pub graphics: GraphicsSettings,
    pub keybindings: KeyBindingsConfig,
    pub server: ServerModeConfig,
}

/// Server mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerModeConfig {
    /// Server mode: "embedded" (local server in background) or "remote" (connect to remote server)
    pub mode: ServerMode,
    /// Remote server address (only used in remote mode)
    pub address: String,
    /// Server port (used in both modes - embedded uses this as its port)
    pub port: u16,
}

/// Server mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerMode {
    /// Local embedded server (single player / LAN)
    Embedded,
    /// Connect to remote server
    Remote,
}

impl Default for ServerModeConfig {
    fn default() -> Self {
        Self {
            mode: ServerMode::Embedded,
            address: "127.0.0.1".to_string(),
            port: 25565,
        }
    }
}

/// Server settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerSettings {
    /// World generation distance in chunks (radius around spawn)
    pub world_gen_distance: u32,
    /// Port for the server
    pub port: u16,
    /// Maximum number of players
    pub max_players: usize,
    /// Server name displayed in client
    pub server_name: String,
    /// MOTD (Message of the Day)
    pub motd: String,
    /// Enable detailed worldgen logging
    pub verbose_worldgen: bool,
    /// Number of world generation worker threads
    pub worldgen_threads: usize,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            world_gen_distance: 16,
            port: 25565,
            max_players: 10,
            server_name: "Voxl Server".to_string(),
            motd: "Welcome to Voxl Server!".to_string(),
            verbose_worldgen: true,
            worldgen_threads: 4,
        }
    }
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            graphics: GraphicsSettings::default(),
            keybindings: KeyBindingsConfig::default(),
            server: ServerModeConfig::default(),
        }
    }
}

impl GameConfig {
    /// Loads configuration from config files
    /// Priority: user_config.toml > config.toml > default values
    pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
        // Try user_config.toml first (user config, not committed)
        if PathBuf::from(USER_CONFIG_FILE).exists() {
            let contents = fs::read_to_string(USER_CONFIG_FILE)?;
            let config: GameConfig = toml::from_str(&contents)?;
            info!("Configuration loaded from {}", USER_CONFIG_FILE);
            return Ok(config);
        }

        // Try config.toml (default config)
        if PathBuf::from(DEFAULT_CONFIG_FILE).exists() {
            let contents = fs::read_to_string(DEFAULT_CONFIG_FILE)?;
            let config: GameConfig = toml::from_str(&contents)?;
            info!("Configuration loaded from {}", DEFAULT_CONFIG_FILE);
            return Ok(config);
        }

        // No config found, create with default values
        info!("No configuration file found, creating with default values");
        let config = GameConfig::default();

        // Create assets/config/ directory if needed
        if let Some(parent) = PathBuf::from(USER_CONFIG_FILE).parent() {
            fs::create_dir_all(parent)?;
        }

        config.save()?;
        Ok(config)
    }

    /// Saves configuration to user_config.toml
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let toml_string = toml::to_string_pretty(self)?;

        // Create assets/config/ directory if needed
        if let Some(parent) = PathBuf::from(USER_CONFIG_FILE).parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(USER_CONFIG_FILE, toml_string)?;
        info!("Configuration saved to {}", USER_CONFIG_FILE);
        Ok(())
    }

    /// Returns the path to the user config file
    pub fn config_path() -> PathBuf {
        PathBuf::from(USER_CONFIG_FILE)
    }
}
