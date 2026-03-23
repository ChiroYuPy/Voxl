//! Chunk tracking system
//!
//! Manages chunk state based on server updates and local modifications.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use voxl_common::ChunkPos;
use crate::dirty_chunks::DirtyChunks;
use tracing::{debug, info};

/// Chunk state
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChunkState {
    /// Chunk not loaded
    Unloaded,
    /// Chunk requested from server
    Requested,
    /// Chunk loaded (data received)
    Loaded,
    /// Chunk needs mesh update
    NeedsMesh,
    /// Chunk meshed
    Meshed,
}

/// Chunk tracker - manages chunk lifecycle
pub struct ChunkTracker {
    /// Chunk states
    states: Arc<RwLock<HashMap<ChunkPos, ChunkState>>>,
    /// Dirty chunks
    dirty: DirtyChunks,
    /// Chunks currently pending mesh (to avoid re-requesting)
    pending_mesh: Arc<RwLock<HashSet<ChunkPos>>>,
}

impl ChunkTracker {
    /// Creates a new chunk tracker
    pub fn new() -> Self {
        Self {
            states: Arc::new(RwLock::new(HashMap::new())),
            dirty: DirtyChunks::new(),
            pending_mesh: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Called when a chunk is received from server
    pub fn on_chunk_loaded(&self, pos: ChunkPos) {
        let mut states = self.states.write().unwrap();
        states.insert(pos, ChunkState::Loaded);
        let total_loaded = states.len();
        drop(states);

        // Extra debug for y=0 chunks
        if pos.1 == 0 {
            info!("[ChunkTracker] LOADED Y=0 CHUNK {:?} (total: {} chunks)", pos, total_loaded);
        } else {
            debug!("[ChunkTracker] Loaded chunk {:?} (total: {} chunks)", pos, total_loaded);
        }

        // Mark for meshing
        self.dirty.mark_dirty(pos);
    }

    /// Called when a chunk is unloaded/removed
    pub fn on_chunk_unloaded(&self, pos: ChunkPos) {
        let mut states = self.states.write().unwrap();
        states.remove(&pos);
        drop(states);

        // Remove from dirty
        self.dirty.mark_clean(pos);

        debug!("[ChunkTracker] Chunk {:?} unloaded", pos);
    }

    /// Called when a local modification happens (optimistic)
    pub fn on_block_modified(&self, pos: ChunkPos) {
        let mut states = self.states.write().unwrap();
        states.insert(pos, ChunkState::NeedsMesh);
        drop(states);

        // Remove from pending to allow re-meshing
        self.pending_mesh.write().unwrap().remove(&pos);

        // Mark as priority dirty (urgent remesh)
        self.dirty.mark_priority(pos);

        debug!("[ChunkTracker] Chunk {:?} modified (optimistic)", pos);
    }

    /// Called when server updates/corrects a chunk
    pub fn on_server_update(&self, pos: ChunkPos) {
        let mut states = self.states.write().unwrap();
        states.insert(pos, ChunkState::NeedsMesh);
        drop(states);

        // Remove from pending to allow re-meshing
        self.pending_mesh.write().unwrap().remove(&pos);

        // Mark as priority dirty (server correction is urgent)
        self.dirty.mark_priority(pos);

        debug!("[ChunkTracker] Chunk {:?} updated by server", pos);
    }

    /// Marks chunk as meshed
    pub fn on_chunk_meshed(&self, pos: ChunkPos) {
        let mut states = self.states.write().unwrap();
        states.insert(pos, ChunkState::Meshed);
        drop(states);

        // Mark as clean
        self.dirty.mark_clean(pos);

        // Remove from pending
        self.pending_mesh.write().unwrap().remove(&pos);
    }

    /// Gets chunks that need meshing (removes them from dirty list and marks as pending)
    pub fn get_chunks_to_mesh(&self, limit: usize) -> Vec<ChunkPos> {
        let mut pending = self.pending_mesh.write().unwrap();

        // Get dirty chunks (without removing them yet)
        let dirty_chunks = self.dirty.get_dirty_chunks();

        // Filter out chunks that are already pending and take up to limit
        let mut chunks_to_mesh = Vec::new();
        for chunk in dirty_chunks.into_iter().take(limit) {
            if !pending.contains(&chunk) {
                chunks_to_mesh.push(chunk);
                pending.insert(chunk);
            }
        }

        // Remove selected chunks from dirty list
        for chunk in &chunks_to_mesh {
            self.dirty.remove(*chunk);
        }

        chunks_to_mesh
    }

    /// Gets the dirty chunks tracker
    pub fn dirty_chunks(&self) -> &DirtyChunks {
        &self.dirty
    }

    /// Gets chunk state
    pub fn get_state(&self, pos: ChunkPos) -> ChunkState {
        let states = self.states.read().unwrap();
        states.get(&pos).copied().unwrap_or(ChunkState::Unloaded)
    }

    /// Returns true if chunk is loaded
    pub fn is_loaded(&self, pos: ChunkPos) -> bool {
        matches!(self.get_state(pos), ChunkState::Loaded | ChunkState::NeedsMesh | ChunkState::Meshed)
    }

    /// Checks if chunks have meshes and marks them as clean
    /// This should be called periodically to clean up chunks that were successfully meshed
    pub fn check_and_mark_meshed<F>(&self, has_mesh: F) -> usize
    where
        F: Fn(ChunkPos) -> bool,
    {
        let states = self.states.read().unwrap();
        let mut to_mark = Vec::new();

        for (pos, state) in states.iter() {
            if *state == ChunkState::NeedsMesh {
                if has_mesh(*pos) {
                    to_mark.push(*pos);
                }
            }
        }
        drop(states);

        let count = to_mark.len();

        // Mark chunks that have meshes as meshed
        for pos in to_mark {
            self.on_chunk_meshed(pos);
        }

        count
    }
}

impl Default for ChunkTracker {
    fn default() -> Self {
        Self::new()
    }
}
