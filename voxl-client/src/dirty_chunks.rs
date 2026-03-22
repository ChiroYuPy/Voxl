//! Dirty chunk tracking system
//!
//! Tracks which chunks need mesh regeneration based on:
//! - Local modifications (optimistic)
//! - Server updates/corrections
//! - Chunk load/unload

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use voxl_common::ChunkPos;
use tracing::debug;

/// Dirty chunk tracker
#[derive(Clone)]
pub struct DirtyChunks {
    /// Set of dirty chunk positions
    dirty: Arc<RwLock<HashSet<ChunkPos>>>,
    /// Priority chunks (urgent remesh needed)
    priority: Arc<RwLock<HashSet<ChunkPos>>>,
}

impl DirtyChunks {
    /// Creates a new dirty chunk tracker
    pub fn new() -> Self {
        Self {
            dirty: Arc::new(RwLock::new(HashSet::new())),
            priority: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// Marks a chunk as dirty (needs remesh)
    pub fn mark_dirty(&self, pos: ChunkPos) {
        let mut dirty = self.dirty.write().unwrap();
        if dirty.insert(pos) {
            debug!("[DirtyChunks] Marked chunk {:?} as dirty", pos);
        }
    }

    /// Marks a chunk as priority dirty (urgent remesh)
    pub fn mark_priority(&self, pos: ChunkPos) {
        let mut priority = self.priority.write().unwrap();
        if priority.insert(pos) {
            debug!("[DirtyChunks] Marked chunk {:?} as priority", pos);
        }
    }

    /// Marks a chunk as clean (meshed)
    pub fn mark_clean(&self, pos: ChunkPos) {
        let mut dirty = self.dirty.write().unwrap();
        dirty.remove(&pos);

        let mut priority = self.priority.write().unwrap();
        priority.remove(&pos);
    }

    /// Removes a chunk from dirty set (returns true if it was dirty)
    pub fn remove(&self, pos: ChunkPos) -> bool {
        let mut dirty = self.dirty.write().unwrap();
        let removed = dirty.remove(&pos);

        let mut priority = self.priority.write().unwrap();
        priority.remove(&pos) || removed
    }

    /// Gets all dirty chunks (priority first, then regular)
    pub fn get_dirty_chunks(&self) -> Vec<ChunkPos> {
        let priority = self.priority.read().unwrap();
        let dirty = self.dirty.read().unwrap();

        let mut result = priority.iter().copied().collect::<Vec<_>>();

        // Add regular dirty chunks (excluding priority ones)
        for pos in dirty.iter() {
            if !priority.contains(pos) {
                result.push(*pos);
            }
        }

        result
    }

    /// Returns true if a chunk is dirty
    pub fn is_dirty(&self, pos: ChunkPos) -> bool {
        let dirty = self.dirty.read().unwrap();
        dirty.contains(&pos)
    }

    /// Clears all dirty flags
    pub fn clear_all(&self) {
        let mut dirty = self.dirty.write().unwrap();
        dirty.clear();

        let mut priority = self.priority.write().unwrap();
        priority.clear();
    }

    /// Marks a range of chunks as dirty (for chunk load/unload)
    pub fn mark_range_dirty(&self, chunks: &[ChunkPos]) {
        let mut dirty = self.dirty.write().unwrap();
        for &pos in chunks {
            dirty.insert(pos);
        }
        debug!("[DirtyChunks] Marked {} chunks as dirty (range)", chunks.len());
    }
}

impl Default for DirtyChunks {
    fn default() -> Self {
        Self::new()
    }
}
