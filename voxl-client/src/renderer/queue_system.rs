//! Système de files d'attente pour la génération et le meshing des chunks
//! Avec guards et logs pour éviter les race conditions

use voxl_common::voxel::{VoxelWorld, VoxelChunk};
use voxl_common::voxel::chunk::{LocalVoxelId, CHUNK_VOLUME};
use crate::renderer::voxel_map::{VoxelVertex, generate_chunk_mesh};
use voxl_common::voxel::GlobalVoxelId;
use crate::worldgen::WorldGenerator;
use std::sync::Arc;
use std::sync::RwLock;
use std::time::Instant;
use crossbeam_channel::{Sender, Receiver, unbounded};
use std::thread::{self, JoinHandle};
use tracing::{info, warn, error, debug};

/// Requête de génération de chunk
#[derive(Debug, Clone)]
pub struct ChunkGenRequest {
    pub cx: i32,
    pub cy: i32,
    pub cz: i32,
    pub priority: u32, // Plus bas = plus prioritaire
}

/// Résultat de génération de chunk
#[derive(Debug)]
pub struct ChunkGenResult {
    pub cx: i32,
    pub cy: i32,
    pub cz: i32,
    pub voxels: Option<Box<[LocalVoxelId; CHUNK_VOLUME]>>,
    pub palette: Option<Vec<GlobalVoxelId>>,
    /// true si le chunk existait déjà (n'a pas été généré par ce worker)
    pub already_exists: bool,
}

/// Requête de mesh avec priorité
#[derive(Debug, Clone)]
pub struct MeshRequest {
    pub cx: i32,
    pub cy: i32,
    pub cz: i32,
    pub priority: MeshPriority,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MeshPriority {
    /// Chunk modifié (bloc cassé/placé) - priorité haute
    Modified = 0,
    /// Chunk nouvellement généré - priorité normale
    New = 1,
}

/// Résultat de mesh
pub struct MeshResult {
    pub cx: i32,
    pub cy: i32,
    pub cz: i32,
    pub vertices: Vec<VoxelVertex>,
}

/// File d'attente pour la génération de chunks
pub struct ChunkGenerationQueue {
    /// Sender pour les requêtes
    tx: Sender<ChunkGenRequest>,
    /// Receiver pour les résultats
    rx: Receiver<ChunkGenResult>,
    /// Handle du worker thread
    _worker: Option<JoinHandle<()>>,
    /// Nombre de workers
    num_workers: usize,
}

impl ChunkGenerationQueue {
    pub fn new(world: Arc<RwLock<VoxelWorld>>, generator_template: &WorldGenerator, num_workers: usize) -> Self {
        let (tx_req, rx_req) = unbounded();
        let (tx_res, rx_res) = unbounded();

        let mut workers = Vec::new();

        for _worker_id in 0..num_workers {
            let rx_req: Receiver<ChunkGenRequest> = rx_req.clone();
            let tx_res = tx_res.clone();
            let world = world.clone();
            let generator = generator_template.clone();

            let handle = thread::spawn(move || {
                loop {
                    match rx_req.recv() {
                        Ok(req) => {
                            // Vérifier si le chunk n'est pas déjà généré
                            let already_generated = if let Ok(world) = world.read() {
                                world.get_chunk_existing(req.cx, req.cy, req.cz).is_some()
                            } else {
                                false
                            };

                            if already_generated {
                                // Le chunk existe déjà - log et envoyer signal
                                warn!("[ChunkGen] WARNING: Duplicate request for already-generated chunk ({},{},{}). This should not happen if tracking is working correctly.",
                                    req.cx, req.cy, req.cz);
                                let _ = tx_res.send(ChunkGenResult {
                                    cx: req.cx,
                                    cy: req.cy,
                                    cz: req.cz,
                                    voxels: None,
                                    palette: None,
                                    already_exists: true,
                                });
                                continue;
                            }

                            // Générer le chunk
                            let gen_start = Instant::now();
                            let mut chunk = VoxelChunk::new();

                            // Récupérer le registry depuis le world
                            let registry = if let Ok(world) = world.read() {
                                world.registry().clone()
                            } else {
                                continue; // Skip si on peut pas accéder au registry
                            };

                            generator.generate_chunk(&mut chunk, &registry, req.cx, req.cy, req.cz);
                            let (voxels, palette) = chunk.extract_data();
                            let gen_duration = gen_start.elapsed();
                            debug!("[ChunkGen] Generated chunk ({},{},{}) in {:.2}ms", req.cx, req.cy, req.cz, gen_duration.as_secs_f64() * 1000.0);

                            // Vérifier encore une fois avant d'envoyer (race condition check)
                            let still_not_generated = if let Ok(world) = world.read() {
                                world.get_chunk_existing(req.cx, req.cy, req.cz).is_none()
                            } else {
                                true
                            };

                            if still_not_generated {
                                if let Err(_) = tx_res.send(ChunkGenResult {
                                    cx: req.cx,
                                    cy: req.cy,
                                    cz: req.cz,
                                    voxels: Some(voxels),
                                    palette: Some(palette),
                                    already_exists: false,
                                }) {
                                    break;
                                }
                            } else {
                                // Un autre worker a généré le chunk entre-temps (race condition)
                                warn!("[ChunkGen] WARNING: Race condition detected for chunk ({},{},{}). Another worker generated it first.",
                                    req.cx, req.cy, req.cz);
                                let _ = tx_res.send(ChunkGenResult {
                                    cx: req.cx,
                                    cy: req.cy,
                                    cz: req.cz,
                                    voxels: None,
                                    palette: None,
                                    already_exists: true,
                                });
                            }
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
            });

            workers.push(handle);
        }

        Self {
            tx: tx_req,
            rx: rx_res,
            _worker: Some(workers.into_iter().next().unwrap()),
            num_workers,
        }
    }

    /// Demande la génération d'un chunk avec une priorité
    pub fn request_chunk(&self, cx: i32, cy: i32, cz: i32, priority: u32) {
        if let Err(_) = self.tx.send(ChunkGenRequest { cx, cy, cz, priority }) {
            error!("[ChunkGen] ERROR: Failed to send chunk request ({},{},{})", cx, cy, cz);
        }
    }

    /// Tente de recevoir un chunk généré (non-bloquant)
    pub fn try_recv(&self) -> Option<ChunkGenResult> {
        self.rx.try_recv().ok()
    }

    /// Retourne le nombre de workers
    pub fn num_workers(&self) -> usize {
        self.num_workers
    }
}

/// File d'attente pour le meshing avec priorité
pub struct MeshQueue {
    /// Sender pour les requêtes
    tx: Sender<MeshRequest>,
    /// Receiver pour les résultats
    rx: Receiver<MeshResult>,
    /// Handle du worker thread
    _worker: Option<JoinHandle<()>>,
    /// Nombre de workers
    num_workers: usize,
    /// Intensité de l'ambient occlusion pour le meshing
    ao_intensity: f32,
}

impl MeshQueue {
    pub fn new(world: Arc<RwLock<VoxelWorld>>, num_workers: usize, ao_intensity: f32) -> Self {
        let (tx_req, rx_req) = unbounded();
        let (tx_res, rx_res) = unbounded();

        let mut workers = Vec::new();

        for _worker_id in 0..num_workers {
            let rx_req: Receiver<MeshRequest> = rx_req.clone();
            let tx_res = tx_res.clone();
            let world = world.clone();
            let ao_intensity = ao_intensity; // Capture pour le thread

            let handle = thread::spawn(move || {
                loop {
                    match rx_req.recv() {
                        Ok(req) => {
                            let mesh_start = Instant::now();

                            // Vérifier que le chunk existe
                            let (vertices, should_send) = if let Ok(world) = world.read() {
                                if let Some(_chunk) = world.get_chunk_existing(req.cx, req.cy, req.cz) {
                                    let registry = world.registry();
                                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                        generate_chunk_mesh(_chunk, &world, req.cx, req.cy, req.cz, registry, ao_intensity)
                                    })) {
                                        Ok(v) => (v, true),
                                        Err(_) => {
                                            (Vec::new(), false)
                                        }
                                    }
                                } else {
                                    (Vec::new(), false)
                                }
                            } else {
                                (Vec::new(), false)
                            };

                            let mesh_duration = mesh_start.elapsed();

                            if should_send && vertices.len() < 200000 {
                                debug!("[Mesh] Generated mesh for chunk ({},{},{}) in {:.2}ms ({} vertices)",
                                    req.cx, req.cy, req.cz, mesh_duration.as_secs_f64() * 1000.0, vertices.len());
                                if let Err(_) = tx_res.send(MeshResult {
                                    cx: req.cx,
                                    cy: req.cy,
                                    cz: req.cz,
                                    vertices,
                                }) {
                                    break;
                                }
                            } else if !should_send {
                                debug!("[Mesh] Skipped mesh for chunk ({},{},{}) - chunk not found or panic", req.cx, req.cy, req.cz);
                            } else if vertices.len() >= 200000 {
                                debug!("[Mesh] Skipped mesh for chunk ({},{},{}) - too many vertices ({})", req.cx, req.cy, req.cz, vertices.len());
                            }
                        }
                        Err(_) => {
                            break;
                        }
                    }
                }
            });

            workers.push(handle);
        }

        Self {
            tx: tx_req,
            rx: rx_res,
            _worker: Some(workers.into_iter().next().unwrap()),
            num_workers,
            ao_intensity,
        }
    }

    /// Demande le mesh d'un chunk avec une priorité
    pub fn request_mesh(&self, cx: i32, cy: i32, cz: i32, priority: MeshPriority) {
        let _ = self.tx.send(MeshRequest { cx, cy, cz, priority });
    }

    /// Tente de recevoir un mesh terminé (non-bloquant)
    pub fn try_recv(&self) -> Option<MeshResult> {
        self.rx.try_recv().ok()
    }

    /// Retourne le nombre de workers
    pub fn num_workers(&self) -> usize {
        self.num_workers
    }
}
