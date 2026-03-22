//! Core server implementation
//!
//! Manages the voxel world, entities, and overall server lifecycle.

use voxl_common::{
    VoxelWorld, SharedVoxelRegistry, WorldGenerator,
    ServerSettings, CHUNK_SIZE,
    entities::EntityWorld,
};
use tracing::{info, warn, error};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;

use crate::connection::ConnectionManager;

/// Server instance managing world, entities, and connections
pub struct Server {
    /// The voxel world (authoritative)
    pub world: Arc<RwLock<VoxelWorld>>,
    /// Entity world (ECS)
    pub entities: Arc<RwLock<EntityWorld>>,
    /// Block registry
    pub registry: SharedVoxelRegistry,
    /// World generator
    pub worldgen: WorldGenerator,
    /// Server settings
    pub settings: ServerSettings,
    /// Connection manager
    pub connections: ConnectionManager,
}

impl Server {
    /// Creates a new server instance
    pub fn new(settings: ServerSettings) -> Result<Self, Box<dyn std::error::Error>> {
        info!("=== Voxl Server Initialization ===");
        info!("Server name: {}", settings.server_name);
        info!("MOTD: {}", settings.motd);
        info!("Port: {}", settings.port);
        info!("Max players: {}", settings.max_players);
        info!("World gen distance: {} chunks", settings.world_gen_distance);

        // Create registry
        let registry = SharedVoxelRegistry::new();

        // Load block definitions and models
        info!("Loading block definitions...");
        if let Err(e) = registry.load_models() {
            warn!("Failed to load models: {}", e);
        }

        if let Err(e) = registry.load_from_folder() {
            warn!("Failed to load blocks from folder: {}", e);
        }

        // Initialize world generator
        let mut worldgen = WorldGenerator::new();
        worldgen.init_block_ids(&registry);

        // Create worlds
        let world = VoxelWorld::new(registry.clone());
        let world = Arc::new(RwLock::new(world));

        let entities = EntityWorld::new();
        let entities = Arc::new(RwLock::new(entities));

        // Create connection manager
        let connections = ConnectionManager::new(settings.max_players);

        info!("=== Server Initialized ===");

        Ok(Server {
            world,
            entities,
            registry,
            worldgen,
            settings,
            connections,
        })
    }

    /// Generates initial world around spawn
    pub fn generate_initial_world(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("=== Starting Initial World Generation ===");
        let gen_distance = self.settings.world_gen_distance as i32;
        let start_time = Instant::now();

        // Track generation statistics
        let mut total_chunks = 0u32;
        let mut total_blocks = 0u64;

        // Generate chunks in a spiral pattern around spawn
        let mut chunks_to_generate: Vec<(i32, i32, i32)> = Vec::new();

        // Generate vertical chunks for each column
        for cy in 0..(voxl_common::WORLD_HEIGHT / CHUNK_SIZE) {
            for cz in -gen_distance..=gen_distance {
                for cx in -gen_distance..=gen_distance {
                    chunks_to_generate.push((cx, cy as i32, cz));
                }
            }
        }

        info!("Chunks to generate: {}", chunks_to_generate.len());

        // Sort by distance from spawn (0, 0) for spiral pattern
        chunks_to_generate.sort_by_key(|&(cx, _cy, cz)| {
            let dist_sq = cx * cx + cz * cz;
            dist_sq
        });

        // Generate chunks
        for (cx, cy, cz) in chunks_to_generate {
            // Check if chunk already exists
            {
                let world = self.world.read().unwrap();
                if world.get_chunk_existing(cx, cy, cz).is_some() {
                    continue;
                }
            }

            // Generate chunk
            let stats = self.generate_chunk(cx, cy, cz)?;

            // Update statistics
            total_chunks += 1;
            total_blocks += stats.blocks_placed as u64;

            // Log every 100 chunks
            if total_chunks % 100 == 0 {
                let elapsed = start_time.elapsed();
                let chunks_per_sec = total_chunks as f64 / elapsed.as_secs_f64();
                info!("[Progress] Generated {} chunks ({:.2} chunks/sec)",
                    total_chunks, chunks_per_sec);
            }
        }

        // Calculate final statistics
        let total_duration = start_time.elapsed();

        info!("=== World Generation Complete ===");
        info!("Total chunks generated: {}", total_chunks);
        info!("Total blocks placed: {}", total_blocks);
        info!("Total time: {:.2}s", total_duration.as_secs_f64());
        info!("Average chunks/sec: {:.2}", total_chunks as f64 / total_duration.as_secs_f64());

        Ok(())
    }

    /// Generates a single chunk
    pub fn generate_chunk(&self, cx: i32, cy: i32, cz: i32) -> Result<voxl_common::worldgen::ChunkGenStats, Box<dyn std::error::Error>> {
        // Create new chunk
        let mut chunk = voxl_common::VoxelChunk::new();

        // Generate with logging if verbose
        let stats = if self.settings.verbose_worldgen {
            self.worldgen.generate_chunk_logged(&mut chunk, &self.registry, cx, cy, cz)
        } else {
            let start = Instant::now();
            self.worldgen.generate_chunk(&mut chunk, &self.registry, cx, cy, cz);
            voxl_common::worldgen::ChunkGenStats {
                blocks_placed: chunk.count_blocks(),
                duration_ns: start.elapsed().as_nanos() as u64,
            }
        };

        // Log individual chunk if verbose
        if self.settings.verbose_worldgen && stats.blocks_placed > 0 {
            info!("[Chunk] Generated chunk ({},{},{}): {} blocks, {:.3}ms",
                cx, cy, cz, stats.blocks_placed, stats.duration_ms());
        }

        // Insert into world
        {
            let mut world = self.world.write().unwrap();
            world.insert_chunk(cx, cy, cz, chunk);
        }

        Ok(stats)
    }

    /// Runs the server main loop
    pub async fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let addr = format!("0.0.0.0:{}", self.settings.port);
        let listener = TcpListener::bind(&addr).await?;

        info!("=== Server Started ===");
        info!("Listening on {}", addr);
        info!("Press Ctrl+C to stop");

        // Spawn tick loop for game logic
        let entities = self.entities.clone();
        let connections = self.connections.clone();
        tokio::spawn(async move {
            server_tick_loop(entities, connections).await;
        });

        // Accept connections loop
        loop {
            tokio::select! {
                result = listener.accept() => {
                    match result {
                        Ok((stream, addr)) => {
                            info!("[Server] New connection from {}", addr);

                            // Clone shared state for the handler
                            let world = self.world.clone();
                            let entities = self.entities.clone();
                            let registry = self.registry.clone();
                            let settings = self.settings.clone();
                            let connections = self.connections.clone();

                            // Spawn connection handler
                            tokio::spawn(async move {
                                crate::connection::handle_connection(
                                    stream, addr,
                                    world, entities, registry, settings,
                                    connections,
                                ).await;
                            });
                        }
                        Err(e) => {
                            error!("[Server] Error accepting connection: {}", e);
                        }
                    }
                }
                _ = tokio::time::sleep(Duration::from_secs(1)) => {
                    // Keepalive tick
                }
            }
        }
    }
}

/// Server tick loop - runs game logic at 20 TPS
async fn server_tick_loop(
    entities: Arc<RwLock<EntityWorld>>,
    connections: ConnectionManager,
) {
    let mut interval = tokio::time::interval(Duration::from_millis(50)); // 20 ticks per second

    loop {
        interval.tick().await;

        // TODO: Run ECS systems
        // - Physics system
        // - Update positions based on velocities
        // - Check collisions

        // TODO: Broadcast entity updates to all clients
    }
}
