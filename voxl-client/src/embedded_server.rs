//! Embedded server for single player / LAN mode
//!
//! Runs a voxl server in a background thread for local play.

use voxl_common::{
    VoxelWorld, SharedVoxelRegistry,
    network::{Packet, PacketPayload, PacketType, ChunkDataPacket, HandshakeAcceptPacket, PACKET_MAGIC, PROTOCOL_VERSION},
    worldgen::WorldGenerator,
    voxel::chunk::WORLD_HEIGHT,
};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, error, debug, warn};
use std::sync::atomic::{AtomicBool, Ordering};
use std::net::{TcpListener, TcpStream};
use std::io::{Read, Write};

/// Embedded server handle
pub struct EmbeddedServer {
    #[allow(dead_code)]
    server_thread: Option<thread::JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
}

impl EmbeddedServer {
    /// Starts an embedded server in a background thread
    /// Pass a SharedVoxelRegistry to ensure client and server use the same block IDs
    pub fn start_with_registry(settings: voxl_common::ServerSettings, registry: SharedVoxelRegistry) -> Result<Self, Box<dyn std::error::Error>> {
        info!("[EmbeddedServer] Starting embedded server on port {}...", settings.port);

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        let server_thread = thread::spawn(move || {
            Self::run_server(settings, shutdown_clone, registry);
        });

        Ok(Self {
            server_thread: Some(server_thread),
            shutdown,
        })
    }

    /// Starts an embedded server in a background thread (creates own registry)
    pub fn start(settings: voxl_common::ServerSettings) -> Result<Self, Box<dyn std::error::Error>> {
        // Create a new registry (will have different IDs - not recommended for embedded mode!)
        let registry = SharedVoxelRegistry::new();
        Self::start_with_registry(settings, registry)
    }

    /// Main server loop
    fn run_server(settings: voxl_common::ServerSettings, shutdown: Arc<AtomicBool>, registry: SharedVoxelRegistry) {
        info!("[EmbeddedServer] Thread started");

        // Load models if not already loaded
        if !registry.has_models() {
            if let Err(e) = registry.load_models() {
                warn!("[EmbeddedServer] Failed to load models: {}", e);
            }
        }

        let mut worldgen = WorldGenerator::new();
        worldgen.init_block_ids(&registry);

        let world = Arc::new(RwLock::new(VoxelWorld::new(registry.clone())));

        // Start TCP listener
        let listener = match TcpListener::bind(format!("0.0.0.0:{}", settings.port)) {
            Ok(l) => {
                info!("[EmbeddedServer] Listening on 0.0.0.0:{}", settings.port);
                l
            }
            Err(e) => {
                error!("[EmbeddedServer] Failed to bind to port {}: {}", settings.port, e);
                return;
            }
        };

        listener.set_nonblocking(true).unwrap_or_else(|e| {
            warn!("[EmbeddedServer] Failed to set non-blocking: {}", e);
        });

        let mut client_stream: Option<TcpStream> = None;
        let mut client_connected = false;

        // Track player position for chunk generation
        let mut player_pos: (i32, i32, i32) = (0, 80, 0);
        let mut chunks_sent = std::collections::HashSet::new();

        let mut last_chunk_gen = Instant::now();
        const CHUNK_GEN_INTERVAL: Duration = Duration::from_millis(10); // 100 chunks per second (faster!)

        while !shutdown.load(Ordering::Relaxed) {
            // Accept new connection
            if !client_connected {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        info!("[EmbeddedServer] New connection from {}", addr);
                        stream.set_nonblocking(true).unwrap_or_else(|e| {
                            warn!("[EmbeddedServer] Failed to set non-blocking: {}", e);
                        });
                        client_stream = Some(stream);
                        client_connected = true;

                        // Send handshake/motd
                        if let Some(ref mut stream) = client_stream {
                            Self::send_packet(stream, Packet {
                                header: voxl_common::network::PacketHeader {
                                    magic: PACKET_MAGIC,
                                    version: PROTOCOL_VERSION,
                                    packet_type: PacketType::HandshakeAccept as u8,
                                },
                                payload: PacketPayload::HandshakeAccept(HandshakeAcceptPacket {
                                    player_id: 1,
                                    server_name: "Voxl Embedded Server".to_string(),
                                    motd: "Welcome to Voxl!".to_string(),
                                }),
                            }).ok();
                        }
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        // No pending connection
                    }
                    Err(e) => {
                        warn!("[EmbeddedServer] Accept error: {}", e);
                    }
                }
            }

            // Handle existing client
            if client_connected {
                if let Some(mut stream) = client_stream.take() {
                    // Receive packets from client
                    let mut should_disconnect = false;
                    match Self::receive_packet(&mut stream) {
                        Ok(Some(packet)) => {
                            match packet.payload {
                                PacketPayload::PlayerPosition(pos) => {
                                    // Update player position
                                    player_pos = (pos.x as i32, pos.y as i32, pos.z as i32);
                                    debug!("[EmbeddedServer] Player at ({}, {}, {})", pos.x, pos.y, pos.z);
                                }
                                PacketPayload::BlockAction(action) => {
                                    // Handle block action
                                    let result = {
                                        let mut world = world.write().unwrap();
                                        match action.action {
                                            voxl_common::network::BlockActionType::Place(block_id) => {
                                                world.set_voxel(action.x, action.y, action.z, Some(block_id as usize))
                                            }
                                            voxl_common::network::BlockActionType::Break => {
                                                world.set_voxel(action.x, action.y, action.z, None)
                                            }
                                        }
                                    };

                                    // Send block change back to client (confirmation)
                                    let block_id = if let Ok(world) = world.read() {
                                        world.get_voxel_opt(action.x, action.y, action.z)
                                    } else {
                                        None
                                    };

                                    if let Some(id) = block_id {
                                        Self::send_packet(&mut stream, Packet {
                                            header: voxl_common::network::PacketHeader {
                                                magic: PACKET_MAGIC,
                                                version: PROTOCOL_VERSION,
                                                packet_type: PacketType::BlockChange as u8,
                                            },
                                            payload: PacketPayload::BlockChange(voxl_common::network::BlockChangePacket {
                                                x: action.x,
                                                y: action.y,
                                                z: action.z,
                                                block_id: id,
                                            }),
                                        }).ok();
                                    }
                                }
                                _ => {}
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            let is_would_block = e.downcast_ref::<std::io::Error>()
                                .map(|io_err| io_err.kind() == std::io::ErrorKind::WouldBlock)
                                .unwrap_or(false);

                            if !is_would_block {
                                error!("[EmbeddedServer] Receive error: {}", e);
                                should_disconnect = true;
                            }
                        }
                    }

                    // Generate and send chunks around player
                    let now = Instant::now();
                    let chunks_before = chunks_sent.len();  // Track before entering the if block
                    if now.duration_since(last_chunk_gen) >= CHUNK_GEN_INTERVAL && !should_disconnect {
                        last_chunk_gen = now;

                        // Generate chunks in a radius around player (including vertical)
                        let view_distance = 8;  // Horizontal radius
                        let vertical_range = 4;  // Vertical chunks to generate
                        let px = player_pos.0 >> 4;
                        let py = player_pos.1 >> 4;
                        let pz = player_pos.2 >> 4;

                        debug!("[EmbeddedServer] Generating chunks around player chunk ({}, {}, {}) - view_distance={}, vertical={}",
                              px, py, pz, view_distance, vertical_range);

                        let mut send_buffer_full = false;
                        'outer: for dx in -view_distance..=view_distance {
                            if send_buffer_full { break; }
                            for dz in -view_distance..=view_distance {
                                if send_buffer_full { break; }
                                let cx = px + dx;
                                let cz = pz + dz;

                                // Generate vertical range of chunks
                                for dy in -vertical_range..=vertical_range {
                                    let cy = py + dy;

                                    // Skip if above world height or too deep
                                    if cy < 0 || cy * 16 >= WORLD_HEIGHT as i32 {
                                        continue;
                                    }

                                    if chunks_sent.contains(&(cx, cy, cz)) {
                                        continue; // Already sent
                                    }

                                    // Generate chunk
                                    let chunk_data = Self::generate_chunk_data(&world, &worldgen, &registry, cx, cy, cz);
                                    if let Ok(data) = chunk_data {
                                        debug!("[EmbeddedServer] Sending chunk ({},{},{})", cx, cy, cz);
                                        // Send chunk to client
                                        let send_result = Self::send_packet(&mut stream, Packet {
                                            header: voxl_common::network::PacketHeader {
                                                magic: PACKET_MAGIC,
                                                version: PROTOCOL_VERSION,
                                                packet_type: PacketType::ChunkData as u8,
                                            },
                                            payload: PacketPayload::ChunkData(ChunkDataPacket {
                                                cx,
                                                cy,
                                                cz,
                                                data,
                                            }),
                                        });

                                        match send_result {
                                            Ok(_) => {
                                                // Chunk sent successfully
                                                chunks_sent.insert((cx, cy, cz));
                                            }
                                            Err(e) => {
                                                // Send failed (buffer full or error)
                                                if e.contains("WouldBlock") {
                                                    // Send buffer full, stop sending this frame
                                                    debug!("[EmbeddedServer] Send buffer full, pausing chunk sends");
                                                    send_buffer_full = true;
                                                    break 'outer;
                                                } else {
                                                    // Other error, log but continue
                                                    debug!("[EmbeddedServer] Failed to send chunk ({},{},{}): {}", cx, cy, cz, e);
                                                }
                                            }
                                        }
                                    } else {
                                        // Chunk not sent (generation failed)
                                        debug!("[EmbeddedServer] Skipping chunk ({},{},{}) - generation failed", cx, cy, cz);
                                    }

                                    // Limit chunks per frame
                                    if chunks_sent.len() % 50 == 0 {
                                        break;
                                    }
                                }  // End for dy
                            }  // End for dz
                        }  // End for dx
                    }

                    let chunks_after = chunks_sent.len();
                    if chunks_after > chunks_before {
                        debug!("[EmbeddedServer] Sent {} chunks this frame (total: {})", chunks_after - chunks_before, chunks_after);
                    }

                    // Put stream back or clear if disconnected
                    if should_disconnect {
                        client_connected = false;
                        chunks_sent.clear();
                    } else {
                        client_stream = Some(stream);
                    }
                }
            }

            // Small sleep to prevent busy-waiting
            std::thread::sleep(Duration::from_millis(10));

            // Log chunk stats every 5 seconds (500 ticks)
            static LOG_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let count = LOG_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if count % 500 == 0 {
                info!("[EmbeddedServer] Stats: {} chunks sent total, player at ({},{},{})",
                      chunks_sent.len(), player_pos.0, player_pos.1, player_pos.2);
            }
        }

        info!("[EmbeddedServer] Shutting down");
    }

    /// Generate chunk data for sending to client
    fn generate_chunk_data(
        world: &Arc<RwLock<VoxelWorld>>,
        worldgen: &WorldGenerator,
        registry: &SharedVoxelRegistry,
        cx: i32,
        cy: i32,
        cz: i32,
    ) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
        // Check if chunk already exists
        let needs_generation = {
            let w = world.read().unwrap();
            w.get_chunk_existing(cx, cy, cz).is_none()
        };

        if needs_generation {
            // Generate chunk
            let mut chunk = voxl_common::VoxelChunk::new();
            worldgen.generate_chunk(&mut chunk, registry, cx, cy, cz);

            // Insert into world
            let mut w = world.write().unwrap();
            w.insert_chunk(cx, cy, cz, chunk);

            // Get the chunk back and serialize it
            if let Some(chunk) = w.get_chunk_existing(cx, cy, cz) {
                Ok(chunk.to_bytes())
            } else {
                Err("Failed to retrieve generated chunk".into())
            }
        } else {
            // Chunk exists, serialize it
            let w = world.read().unwrap();
            if let Some(chunk) = w.get_chunk_existing(cx, cy, cz) {
                Ok(chunk.to_bytes())
            } else {
                Err("Chunk not found".into())
            }
        }
    }

    /// Send a packet to the client (non-blocking)
    fn send_packet(stream: &mut TcpStream, packet: Packet) -> Result<(), String> {
        let bytes = packet.to_bytes().map_err(|e| format!("{:?}", e))?;
        let len = bytes.len() as u32;

        // Try to write length, return error if would block (send buffer full)
        match stream.write(&len.to_be_bytes()) {
            Ok(n) if n == 4 => {}
            Ok(_) => return Err("Partial write of length".into()),
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                return Err("WouldBlock - send buffer full".into());
            }
            Err(e) => return Err(format!("{:?}", e)),
        }

        // Try to write data
        let mut written = 0;
        while written < bytes.len() {
            match stream.write(&bytes[written..]) {
                Ok(n) => {
                    written += n;
                    if n == 0 {
                        return Err("Write returned 0".into());
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err("WouldBlock - send buffer full".into());
                }
                Err(e) => return Err(format!("{:?}", e)),
            }
        }

        // Try to flush
        match stream.flush() {
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Flush would block, but data is written, so that's okay
            }
            Err(e) => return Err(format!("{:?}", e)),
        }

        Ok(())
    }

    /// Receive a packet from the client (non-blocking)
    fn receive_packet(stream: &mut TcpStream) -> Result<Option<Packet>, Box<dyn std::error::Error>> {
        // Read length
        let mut len_bytes = [0u8; 4];
        match stream.read_exact(&mut len_bytes) {
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                return Ok(None);
            }
            Err(e) => return Err(Box::new(e)),
        }

        let len = u32::from_be_bytes(len_bytes) as usize;

        // Read data
        let mut data = vec![0u8; len];
        stream.read_exact(&mut data)?;

        // Deserialize packet
        let packet = Packet::from_bytes(&data)?;
        Ok(Some(packet))
    }

    /// Stops the embedded server
    pub fn stop(mut self) {
        info!("[EmbeddedServer] Stopping embedded server...");
        self.shutdown.store(true, Ordering::Relaxed);

        if let Some(thread) = self.server_thread.take() {
            let _ = thread.join();
        }

        info!("[EmbeddedServer] Embedded server stopped");
    }
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        if self.server_thread.is_some() {
            self.shutdown.store(true, Ordering::Relaxed);
            if let Some(thread) = self.server_thread.take() {
                let _ = thread.join();
            }
        }
    }
}
