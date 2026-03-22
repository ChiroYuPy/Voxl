//! OLD: Tracker d'état des chunks pour éviter les doublons dans le pipeline
//!
//! NOTE: This is the OLD local generation tracker.
//! Use crate::chunk_tracker::ChunkTracker for the new server-based system.

use voxl_common::voxel::ChunkPos;
use std::collections::HashSet;
use std::sync::{Arc, RwLock};
use tracing::warn;

/// OLD: Track l'état des chunks dans le pipeline de génération/meshing
#[deprecated(note = "Use crate::chunk_tracker::ChunkTracker for server-based system")]
pub struct LocalChunkTracker {
    /// Chunks en attente de génération
    pending_generation: HashSet<ChunkPos>,
    /// Chunks en cours de génération
    generating: HashSet<ChunkPos>,
    /// Chunks générés (existent dans le monde)
    generated: HashSet<ChunkPos>,
    /// Chunks en attente de meshing
    pending_mesh: HashSet<ChunkPos>,
    /// Chunks en cours de meshing
    meshing: HashSet<ChunkPos>,
    /// Chunks meshés (ont un mesh renderable)
    meshed: HashSet<ChunkPos>,
}

impl LocalChunkTracker {
    pub fn new() -> Self {
        Self {
            pending_generation: HashSet::new(),
            generating: HashSet::new(),
            generated: HashSet::new(),
            pending_mesh: HashSet::new(),
            meshing: HashSet::new(),
            meshed: HashSet::new(),
        }
    }

    /// Marque un chunk comme en attente de génération
    pub fn mark_pending_generation(&mut self, pos: ChunkPos) -> bool {
        // Si déjà généré, on ne demande pas la génération
        if self.generated.contains(&pos) {
            warn!("[ChunkTracker] WARNING: Chunk {:?} already generated, skipping gen request", pos);
            return false;
        }
        // Si déjà en attente ou en cours, on ne redemande pas
        if self.pending_generation.contains(&pos) || self.generating.contains(&pos) {
            // Log seulement si c'est anormal (ce qui indiquerait un bug dans la logique d'appel)
            if self.pending_generation.contains(&pos) {
                warn!("[ChunkTracker] WARNING: Chunk {:?} already in pending_generation. This indicates duplicate request calls.", pos);
            }
            return false;
        }
        self.pending_generation.insert(pos);
        true
    }

    /// Marque un chunk comme en cours de génération
    pub fn mark_generating(&mut self, pos: ChunkPos) {
        self.pending_generation.remove(&pos);
        self.generating.insert(pos);
    }

    /// Marque un chunk comme généré (inséré dans le monde)
    pub fn mark_generated(&mut self, pos: ChunkPos) {
        // Important: retirer de TOUS les états de génération
        self.pending_generation.remove(&pos);
        self.generating.remove(&pos);
        self.generated.insert(pos);
        // Note: ne PAS ajouter à pending_mesh automatiquement
        // Le mesh sera demandé explicitement par request_mesh_single
    }

    /// Marque un chunk comme en attente de meshing (priorité haute pour modification)
    pub fn mark_pending_mesh_modified(&mut self, pos: ChunkPos) {
        self.pending_mesh.insert(pos);
    }

    /// Marque un chunk comme en cours de meshing
    pub fn mark_meshing(&mut self, pos: ChunkPos) {
        self.pending_mesh.remove(&pos);
        self.meshing.insert(pos);
    }

    /// Marque un chunk comme meshé
    pub fn mark_meshed(&mut self, pos: ChunkPos) {
        self.meshing.remove(&pos);
        self.pending_mesh.remove(&pos);  // Important: retirer de pending aussi!
        self.meshed.insert(pos);
    }

    /// Retire un chunk meshé (si le mesh est vide par exemple)
    pub fn unmark_meshed(&mut self, pos: ChunkPos) {
        self.meshed.remove(&pos);
    }

    /// Retire un chunk de tous les états de meshing (pour remesh forcé)
    pub fn clear_mesh_state(&mut self, pos: ChunkPos) {
        self.pending_mesh.remove(&pos);
        self.meshing.remove(&pos);
        self.meshed.remove(&pos);
    }

    /// Vérifie si un chunk est généré
    pub fn is_generated(&self, pos: &ChunkPos) -> bool {
        self.generated.contains(pos)
    }

    /// Vérifie si un chunk est en attente ou en cours de génération
    pub fn is_generating(&self, pos: &ChunkPos) -> bool {
        self.pending_generation.contains(pos) || self.generating.contains(pos)
    }

    /// Vérifie si un chunk est en attente ou en cours de meshing
    pub fn is_meshing(&self, pos: &ChunkPos) -> bool {
        self.pending_mesh.contains(pos) || self.meshing.contains(pos)
    }

    /// Retourne le nombre de chunks en attente de génération
    pub fn pending_generation_count(&self) -> usize {
        self.pending_generation.len()
    }

    /// Retourne le nombre de chunks en attente de meshing
    pub fn pending_mesh_count(&self) -> usize {
        self.pending_mesh.len()
    }

    /// Retire un chunk (si déchargé par exemple)
    pub fn remove_chunk(&mut self, pos: &ChunkPos) {
        self.pending_generation.remove(pos);
        self.generating.remove(pos);
        self.generated.remove(pos);
        self.pending_mesh.remove(pos);
        self.meshing.remove(pos);
        self.meshed.remove(pos);
    }

    /// Nettoie les états pour un chunk qui va être regénéré
    pub fn clear_for_regeneration(&mut self, pos: &ChunkPos) {
        self.meshed.remove(pos);
        self.pending_mesh.remove(pos);
        self.meshing.remove(pos);
    }

    /// Nettoie les entrées orphelines (chunks déjà générés mais encore dans pending_generation)
    pub fn cleanup_pending_generated(&mut self, generated_chunks: &HashSet<ChunkPos>) {
        // Retirer de pending_generation tous les chunks qui sont déjà générés
        self.pending_generation.retain(|pos| !generated_chunks.contains(pos));
    }

    /// Nettoie pending_generation en vérifiant chaque chunk dans le monde (plus lent mais précis)
    pub fn cleanup_pending_generation_verify<F>(&mut self, exists_fn: F)
    where
        F: Fn(ChunkPos) -> bool + Send + Sync,
    {
        let count_before = self.pending_generation.len();
        if count_before == 0 {
            return;
        }

        // Afficher les chunks en attente qui n'existent pas
        let mut non_existent = Vec::new();
        self.pending_generation.retain(|pos| {
            let exists = exists_fn(*pos);
            if !exists {
                non_existent.push(*pos);
            }
            exists
        });

        if !non_existent.is_empty() {
            warn!("[Cleanup] Removed {} non-existent chunks from pending_gen (remaining: {})",
                non_existent.len(), self.pending_generation.len());
            // Afficher les 10 premiers pour debug
            for pos in non_existent.iter().take(10) {
                warn!("  - {:?}", pos);
            }
            if non_existent.len() > 10 {
                warn!("  ... and {} more", non_existent.len() - 10);
            }
        }
    }
}

/// Version thread-safe du tracker
pub struct LocalSharedChunkTracker(Arc<RwLock<LocalChunkTracker>>);

impl LocalSharedChunkTracker {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(LocalChunkTracker::new())))
    }

    pub fn mark_pending_generation(&self, pos: ChunkPos) -> bool {
        if let Ok(mut tracker) = self.0.write() {
            tracker.mark_pending_generation(pos)
        } else {
            false
        }
    }

    pub fn mark_generated(&self, pos: ChunkPos) {
        if let Ok(mut tracker) = self.0.write() {
            tracker.mark_generated(pos);
        }
    }

    pub fn mark_pending_mesh_modified(&self, pos: ChunkPos) {
        if let Ok(mut tracker) = self.0.write() {
            tracker.mark_pending_mesh_modified(pos);
        }
    }

    pub fn mark_meshed(&self, pos: ChunkPos) {
        if let Ok(mut tracker) = self.0.write() {
            tracker.mark_meshed(pos);
        }
    }

    pub fn unmark_meshed(&self, pos: ChunkPos) {
        if let Ok(mut tracker) = self.0.write() {
            tracker.unmark_meshed(pos);
        }
    }

    /// Nettoie tous les états de meshing pour un chunk (utilisé pour remesh forcé)
    pub fn clear_mesh_state(&self, pos: ChunkPos) {
        if let Ok(mut tracker) = self.0.write() {
            tracker.clear_mesh_state(pos);
        }
    }

    /// Nettoie les entrées orphelines dans pending_generation
    pub fn cleanup_pending_generated(&self, generated_chunks: &HashSet<ChunkPos>) {
        if let Ok(mut tracker) = self.0.write() {
            tracker.cleanup_pending_generated(generated_chunks);
        }
    }

    /// Nettoie pending_generation en vérifiant chaque chunk dans le monde
    pub fn cleanup_pending_generation_verify<F>(&self, exists_fn: F)
    where
        F: Fn(ChunkPos) -> bool + Send + Sync,
    {
        if let Ok(mut tracker) = self.0.write() {
            tracker.cleanup_pending_generation_verify(exists_fn);
        }
    }

    pub fn is_generated(&self, pos: &ChunkPos) -> bool {
        if let Ok(tracker) = self.0.read() {
            tracker.is_generated(pos)
        } else {
            false
        }
    }

    pub fn is_generating(&self, pos: &ChunkPos) -> bool {
        if let Ok(tracker) = self.0.read() {
            tracker.is_generating(pos)
        } else {
            false
        }
    }

    pub fn take_pending_mesh(&self, max_count: usize) -> Vec<ChunkPos> {
        if let Ok(mut tracker) = self.0.write() {
            let mut result = Vec::new();
            // Prendre les premiers chunks de la HashSet
            for pos in tracker.pending_mesh.iter().take(max_count) {
                result.push(*pos);
            }
            // Retirer ceux qu'on a pris
            for pos in &result {
                tracker.pending_mesh.remove(pos);
                tracker.meshing.insert(*pos);
            }
            result
        } else {
            Vec::new()
        }
    }

    pub fn get_stats(&self) -> (usize, usize, usize) {
        if let Ok(tracker) = self.0.read() {
            (
                tracker.pending_generation.len(),
                tracker.generating.len(),
                tracker.pending_mesh.len(),
            )
        } else {
            (0, 0, 0)
        }
    }

    /// Vérifie si un chunk est déjà en attente ou en cours de meshing
    pub fn is_meshing_or_pending(&self, pos: &ChunkPos) -> bool {
        if let Ok(tracker) = self.0.read() {
            tracker.pending_mesh.contains(pos) || tracker.meshing.contains(pos)
        } else {
            false
        }
    }

    /// Marque directement un chunk comme pending mesh (sans vérifications)
    pub fn mark_pending_mesh_direct(&self, pos: ChunkPos) {
        if let Ok(mut tracker) = self.0.write() {
            tracker.pending_mesh.insert(pos);
        }
    }

    /// Nettoie les chunks en attente qui sont hors des limites verticales
    pub fn cleanup_out_of_bounds(&self, max_y: i32) {
        if let Ok(mut tracker) = self.0.write() {
            let before = tracker.pending_generation.len();
            tracker.pending_generation.retain(|pos| pos.1 >= 0 && pos.1 < max_y);
            let after = tracker.pending_generation.len();
            if before != after {
                warn!("[Cleanup] Removed {} out-of-bounds chunks from pending_generation (was: {}, now: {})",
                    before - after, before, after);
            }
        }
    }
}

impl Clone for LocalSharedChunkTracker {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
