//! Temporary compatibility layer for ChunkTracker
//!
//! This bridges the old LocalChunkTracker API with the new server-based ChunkTracker

use crate::renderer::chunk_tracker::LocalSharedChunkTracker;
use std::sync::{Arc, RwLock};
use voxl_common::ChunkPos;

/// Temporary compatibility wrapper
pub struct ChunkTrackerCompat {
    old: LocalSharedChunkTracker,
}

impl ChunkTrackerCompat {
    pub fn new() -> Self {
        Self {
            old: LocalSharedChunkTracker::new(),
        }
    }

    // Delegate to old tracker for now - all methods take &self since LocalSharedChunkTracker handles locking internally
    pub fn mark_pending_generation(&self, pos: ChunkPos) -> bool {
        self.old.mark_pending_generation(pos)
    }

    pub fn mark_generated(&self, pos: ChunkPos) {
        self.old.mark_generated(pos);
    }

    pub fn mark_meshed(&self, pos: ChunkPos) {
        self.old.mark_meshed(pos);
    }

    pub fn is_generated(&self, pos: &ChunkPos) -> bool {
        self.old.is_generated(pos)
    }

    pub fn get_stats(&self) -> (usize, usize, usize) {
        self.old.get_stats()
    }

    pub fn cleanup_pending_generation_verify<F>(&self, exists_fn: F)
    where
        F: Fn(ChunkPos) -> bool + Send + Sync,
    {
        self.old.cleanup_pending_generation_verify(exists_fn);
    }

    pub fn clear_mesh_state(&self, pos: ChunkPos) {
        self.old.clear_mesh_state(pos);
    }

    pub fn mark_pending_mesh_direct(&self, pos: ChunkPos) {
        self.old.mark_pending_mesh_direct(pos);
    }

    pub fn is_generating(&self, pos: &ChunkPos) -> bool {
        self.old.is_generating(pos)
    }

    pub fn is_meshing_or_pending(&self, pos: &ChunkPos) -> bool {
        self.old.is_meshing_or_pending(pos)
    }
}

pub type SharedChunkTracker = Arc<RwLock<ChunkTrackerCompat>>;
