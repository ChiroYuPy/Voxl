//! Core server implementation
//!
//! Manages the voxel world, entities, and overall server lifecycle.

use voxl_common::{
    VoxelWorld, SharedVoxelRegistry, WorldGenerator,
    ServerSettings, CHUNK_SIZE, PlayerId,
    entities::EntityWorld,
};
use tracing::{info, warn, error};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use hecs::Entity;

use crate::connection::ConnectionManager;
use crate::dispatcher::CommandDispatcher;

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
    /// Shutdown signal for embedded mode
    shutdown: Option<tokio::sync::broadcast::Sender<()>>,
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
            shutdown: None,
        })
    }

    /// Creates a new server instance with a shared registry (for embedded mode)
    pub fn with_registry(settings: ServerSettings, registry: SharedVoxelRegistry) -> Result<Self, Box<dyn std::error::Error>> {
        info!("=== Voxl Server Initialization (Embedded Mode) ===");
        info!("Server name: {}", settings.server_name);
        info!("Port: {}", settings.port);

        // Load models if not already loaded
        if !registry.has_models() {
            if let Err(e) = registry.load_models() {
                warn!("Failed to load models: {}", e);
            }
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

        info!("=== Server Initialized (Embedded) ===");

        Ok(Server {
            world,
            entities,
            registry,
            worldgen,
            settings,
            connections,
            shutdown: None,
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

                            // Create command dispatcher for this connection
                            let dispatcher = CommandDispatcher::with_defaults();
                            let world_c = world.clone();
                            let entities_c = entities.clone();
                            let registry_c = registry.clone();
                            let settings_c = settings.clone();

                            let execute_command = move |command: &str,
                                                        player_id: PlayerId,
                                                        username: &str,
                                                        entity: Option<Entity>,
                                                        players: &[(PlayerId, String)]| {
                                dispatcher.dispatch(
                                    command,
                                    player_id,
                                    username,
                                    entity,
                                    &world_c,
                                    &entities_c,
                                    &registry_c,
                                    &settings_c,
                                    players,
                                )
                            };

                            // Spawn connection handler
                            tokio::spawn(async move {
                                crate::connection::handle_connection(
                                    stream, addr,
                                    world, entities, registry, settings,
                                    connections,
                                    execute_command,
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

/// Runs the server in embedded mode (for single player)
/// This function is designed to be run in a background thread.
///
/// Returns the actual port the server is listening on.
pub fn run_embedded_server(
    settings: voxl_common::ServerSettings,
    registry: voxl_common::SharedVoxelRegistry,
) -> Result<u16, Box<dyn std::error::Error>> {
    use std::sync::mpsc;

    // Create a channel to communicate the actual port back
    let (port_tx, port_rx) = mpsc::channel();

    // Spawn the server thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();

        let _ = rt.block_on(async {
            // Create the server with the shared registry
            let server = Server::with_registry(settings, registry).unwrap();

            // Generate initial world before accepting connections
            info!("[EmbeddedServer] Generating initial world...");
            if let Err(e) = server.generate_initial_world() {
                error!("[EmbeddedServer] Failed to generate initial world: {}", e);
            }

            // Bind to get the actual port
            let addr = format!("0.0.0.0:{}", server.settings.port);
            let listener = TcpListener::bind(&addr).await.unwrap();

            // Get the actual port
            let actual_port = listener.local_addr().unwrap().port();

            // Send the port back
            let _ = port_tx.send(actual_port);

            info!("[EmbeddedServer] Listening on 0.0.0.0:{}", actual_port);

            // Spawn tick loop for game logic
            let entities = server.entities.clone();
            let connections = server.connections.clone();
            tokio::spawn(async move {
                server_tick_loop(entities, connections).await;
            });

            // Accept connections loop (single client for embedded mode)
            let mut has_client = false;

            loop {
                tokio::select! {
                    result = listener.accept() => {
                        if !result.is_err() && !has_client {
                            let (stream, addr) = result.unwrap();

                            info!("[EmbeddedServer] Client connected from {}", addr);

                            // Clone shared state for the handler
                            let world = server.world.clone();
                            let entities = server.entities.clone();
                            let registry = server.registry.clone();
                            let settings = server.settings.clone();
                            let connections = server.connections.clone();

                            // Create command dispatcher
                            let dispatcher = CommandDispatcher::with_defaults();
                            let world_c = world.clone();
                            let entities_c = entities.clone();
                            let registry_c = registry.clone();
                            let settings_c = settings.clone();

                            let execute_command = move |command: &str,
                                                        player_id: PlayerId,
                                                        username: &str,
                                                        entity: Option<hecs::Entity>,
                                                        players: &[(PlayerId, String)]| {
                                dispatcher.dispatch(
                                    command,
                                    player_id,
                                    username,
                                    entity,
                                    &world_c,
                                    &entities_c,
                                    &registry_c,
                                    &settings_c,
                                    players,
                                )
                            };

                            // Spawn connection handler
                            tokio::spawn(async move {
                                crate::connection::handle_connection(
                                    stream, addr,
                                    world, entities, registry, settings,
                                    connections,
                                    execute_command,
                                ).await;
                            });

                            has_client = true;
                        }
                    }
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {
                        // Keepalive tick
                    }
                }
            }
        });
    });

    // Wait for the actual port (with timeout - increased for world generation)
    let actual_port = port_rx.recv_timeout(std::time::Duration::from_secs(30))?;
    Ok(actual_port)
}
