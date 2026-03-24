//! Voxl Common - Shared code between client and server
//!
//! This crate contains all game logic that is not specific to
//! graphics rendering or user interface.

// Core modules
pub mod voxel;
pub mod entities;
pub mod config;
pub mod paths;
pub mod worldgen;
pub mod network;
pub mod commands;
pub mod chat;

// Re-exports for convenience
pub use voxel::{
    VoxelChunk, VoxelWorld, ChunkPos, SetResult,
    VoxelRegistry, SharedVoxelRegistry, VoxelDefinition,
    VoxelStringId, GlobalVoxelId, TextureUV,
    BlockConfig, BlockModel, VoxelFace,
    CHUNK_SIZE, WORLD_HEIGHT, VERTICAL_CHUNKS,
};

pub use entities::{EntityWorld, GameMode};
pub use config::{GameConfig, ServerSettings};
pub use worldgen::WorldGenerator;
pub use network::*;
pub use commands::{Command, CommandContext, CommandResult, TabCompleteSuggestion};
pub use commands::args;
pub use chat::{
    ChatMessage, ChatComponent, ChatColor, ChatFormat, ChatFormat as ChatStyle,
    ClickAction, ClickEvent, HoverAction, HoverEvent,
    // Helper functions
    text as chat_text, raw as chat_raw, error as chat_error, success as chat_success, info as chat_info,
};
