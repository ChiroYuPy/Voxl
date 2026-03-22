// Client-specific modules
pub mod app;
pub mod chunk_tracker_compat;
pub mod debug;
pub mod client_systems;
pub mod dirty_chunks;
pub mod embedded_server;
pub mod game_state;
pub mod input;
pub mod networking;
pub mod performance;
pub mod raycast;
pub mod renderer;
pub mod server_chunk_tracker;
pub mod server_integration;
pub mod ui;
pub mod worldgen;

// Re-export common types from voxl-common
pub use voxl_common::{
    VoxelChunk, VoxelWorld, VoxelRegistry, SharedVoxelRegistry,
    GameConfig, EntityWorld, GameMode,
    voxel::{BlockConfig, VoxelFace},
    entities::{
        Position, Velocity, PlayerControlled, LookDirection,
        PhysicsAffected, AABB, Name,
    },
    ChunkPos, SetResult,
};
pub use voxl_common::config::GraphicsSettings;

pub use app::run;
