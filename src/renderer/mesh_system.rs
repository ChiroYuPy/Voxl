use crate::voxel::{ChunkPos, VoxelWorld};
use crate::renderer::voxel_map::{VoxelVertex, generate_chunk_mesh};
use crossbeam_channel::{Sender, Receiver, unbounded};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{JoinHandle, self};
use std::sync::RwLock;

pub struct RebuiltMesh {
    pub chunk_pos: ChunkPos,
    pub vertices: Vec<VoxelVertex>,
}

pub struct MeshBuildSystem {
    tx: Sender<ChunkPos>,
    rx: Receiver<RebuiltMesh>,
    thread_handle: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl MeshBuildSystem {
    pub fn new(world: Arc<RwLock<VoxelWorld>>) -> Self {
        let (tx, rx_in) = unbounded();
        let (tx_out, rx) = unbounded();
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let handle = thread::spawn(move || {
            loop {
                match rx_in.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok((cx, cy, cz)) => {
                        if shutdown_clone.load(Ordering::Relaxed) {
                            break;
                        }
                        if let Ok(world) = world.read() {
                            if let Some(chunk) = world.get_chunk_existing(cx, cy, cz) {
                                let registry = world.registry();
                                let vertices = generate_chunk_mesh(chunk, &world, cx, cy, cz, registry);
                                let _ = tx_out.send(RebuiltMesh {
                                    chunk_pos: (cx, cy, cz),
                                    vertices,
                                });
                            }
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                        if shutdown_clone.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                }
            }
        });

        Self {
            tx,
            rx,
            thread_handle: Some(handle),
            shutdown,
        }
    }

    pub fn request_rebuild(&self, cx: i32, cy: i32, cz: i32) {
        let _ = self.tx.send((cx, cy, cz));
    }

    pub fn request_rebuild_many(&self, chunks: &[ChunkPos]) {
        for &(cx, cy, cz) in chunks {
            let _ = self.tx.send((cx, cy, cz));
        }
    }

    pub fn try_recv(&self) -> Option<RebuiltMesh> {
        self.rx.try_recv().ok()
    }

    pub fn recv(&self) -> Result<RebuiltMesh, crossbeam_channel::RecvError> {
        self.rx.recv()
    }
}

impl Drop for MeshBuildSystem {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        drop(self.tx.clone());
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}
