//! Project file paths

use std::fs;
use std::path::PathBuf;

/// Returns the path to the assets directory
pub fn assets_dir() -> PathBuf {
    // For client running from voxl-client/, assets are in ../assets
    // For server running from voxl-reborn/, assets are in ./assets
    let relative = PathBuf::from("assets");
    let parent = PathBuf::from("../assets");

    // Prefer parent path (../assets) for client compatibility
    if parent.exists() {
        parent
    } else if relative.exists() {
        relative
    } else {
        // Default to parent path for client
        parent
    }
}

/// Returns the path to the config directory
pub fn config_dir() -> PathBuf {
    assets_dir().join("config")
}

/// Returns the path to the user config file
pub fn user_config_path() -> PathBuf {
    config_dir().join("user_config.toml")
}

/// Returns the path to the default config file
pub fn default_config_path() -> PathBuf {
    config_dir().join("config.toml")
}

/// Returns the path to the textures directory
pub fn textures_dir() -> PathBuf {
    assets_dir().join("textures")
}

/// Returns the path to the models directory
pub fn models_dir() -> PathBuf {
    assets_dir().join("models")
}

/// Returns the path to the data directory (generated data)
pub fn data_dir() -> PathBuf {
    PathBuf::from("data")
}

/// Returns the path to the logs directory
pub fn logs_dir() -> PathBuf {
    data_dir().join("logs")
}

/// Returns the path to the debug directory
pub fn debug_dir() -> PathBuf {
    data_dir().join("debug")
}

/// Returns the path to the screenshots directory
pub fn screenshots_dir() -> PathBuf {
    data_dir().join("screenshots")
}

/// Initializes all required directories
pub fn init_directories() -> Result<(), std::io::Error> {
    fs::create_dir_all(config_dir())?;
    fs::create_dir_all(textures_dir())?;
    fs::create_dir_all(models_dir())?;
    fs::create_dir_all(logs_dir())?;
    fs::create_dir_all(debug_dir())?;
    fs::create_dir_all(screenshots_dir())?;
    Ok(())
}
