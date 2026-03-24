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
    /// The actual port the server is listening on (may differ from requested port)
    pub actual_port: u16,
}

impl EmbeddedServer {
    /// Starts an embedded server in a background thread
    /// Pass a SharedVoxelRegistry to ensure client and server use the same block IDs
    /// If port is 0, an available port will be automatically assigned
    /// Returns the server handle with the actual port assigned
    pub fn start_with_registry(settings: voxl_common::ServerSettings, registry: SharedVoxelRegistry) -> Result<Self, Box<dyn std::error::Error>> {
        info!("[EmbeddedServer] Starting embedded server on port {}...", settings.port);

        // First, bind to find an available port if port is 0
        let actual_port = if settings.port == 0 {
            let listener = TcpListener::bind("0.0.0.0:0")?;
            let actual_port = listener.local_addr()?.port();
            info!("[EmbeddedServer] Auto-assigned port: {}", actual_port);
            actual_port
        } else {
            settings.port
        };

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();

        // Update settings with actual port
        let mut settings = settings;
        settings.port = actual_port;

        let server_thread = thread::spawn(move || {
            Self::run_server(settings, shutdown_clone, registry);
        });

        Ok(Self {
            server_thread: Some(server_thread),
            shutdown,
            actual_port,
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

        // Send buffer to prevent partial writes from corrupting the stream
        let mut send_buffer: Vec<u8> = Vec::new();

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

                        // Send handshake/motd (direct send for handshake)
                        if let Some(ref mut stream) = client_stream {
                            Self::send_packet_direct(stream, Packet {
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
                    // First, try to flush any pending send data
                    let buffer_empty = Self::flush_send_buffer(&mut stream, &mut send_buffer).unwrap_or(true);

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

                                    // Send block change back to client (confirmation) - queue it
                                    let block_id = if let Ok(world) = world.read() {
                                        world.get_voxel_opt(action.x, action.y, action.z)
                                    } else {
                                        None
                                    };

                                    if let Some(id) = block_id {
                                        let _ = Self::queue_packet(&mut send_buffer, Packet {
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
                                        });
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
                    if now.duration_since(last_chunk_gen) >= CHUNK_GEN_INTERVAL && !should_disconnect && buffer_empty {
                        last_chunk_gen = now;

                        // Generate chunks in spiral pattern from center outward
                        let view_distance = 8;  // Horizontal radius
                        let vertical_range = 4;  // Vertical chunks to generate
                        let px = player_pos.0 >> 4;
                        let py = player_pos.1 >> 4;
                        let pz = player_pos.2 >> 4;

                        debug!("[EmbeddedServer] Generating chunks around player chunk ({}, {}, {}) - view_distance={}, vertical={}",
                              px, py, pz, view_distance, vertical_range);

                        // Generate chunk positions in spiral order (center first, then outward)
                        let mut chunks_to_generate: Vec<(i32, i32, i32, i32)> = Vec::new();
                        for radius in 0i32..=view_distance {
                            for dx in -radius..=radius {
                                for dz in -radius..=radius {
                                    // Only add chunks at the current radius layer (outer edge)
                                    // This creates concentric squares instead of revisiting inner chunks
                                    if dx.abs() != radius && dz.abs() != radius {
                                        continue;
                                    }
                                    let dist_sq = dx * dx + dz * dz;
                                    chunks_to_generate.push((px + dx, pz + dz, dist_sq, radius));
                                }
                            }
                        }

                        // Sort by distance (center first)
                        chunks_to_generate.sort_by_key(|&(_, _, dist_sq, _)| dist_sq);

                        let mut chunks_queued = 0usize;
                        const MAX_CHUNKS_PER_FRAME: usize = 50;

                        for (cx, cz, _dist_sq, _radius) in chunks_to_generate {
                            if chunks_queued >= MAX_CHUNKS_PER_FRAME { break; }

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
                                    debug!("[EmbeddedServer] Queueing chunk ({},{},{})", cx, cy, cz);
                                    // Queue chunk for sending
                                    let queue_result = Self::queue_packet(&mut send_buffer, Packet {
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

                                        if queue_result.is_ok() {
                                            // Chunk queued successfully - mark as sent
                                            chunks_sent.insert((cx, cy, cz));
                                            chunks_queued += 1;
                                        }
                                    }
                                }
                            }

                        let chunks_after = chunks_sent.len();
                        if chunks_after > chunks_before {
                            debug!("[EmbeddedServer] Sent {} chunks this frame (total: {})", chunks_after - chunks_before, chunks_after);
                        }
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

    /// Add a packet to the send buffer (doesn't write to stream yet)
    fn queue_packet(send_buffer: &mut Vec<u8>, packet: Packet) -> Result<(), String> {
        let bytes = packet.to_bytes().map_err(|e| format!("{:?}", e))?;
        let len = bytes.len() as u32;

        // Add length prefix then data
        send_buffer.extend_from_slice(&len.to_be_bytes());
        send_buffer.extend_from_slice(&bytes);

        Ok(())
    }

    /// Flush the send buffer to the stream (non-blocking)
    /// Returns true if buffer is now empty, false if more data remains
    fn flush_send_buffer(stream: &mut TcpStream, send_buffer: &mut Vec<u8>) -> Result<bool, Box<dyn std::error::Error>> {
        if send_buffer.is_empty() {
            return Ok(true);
        }

        match stream.write(&send_buffer) {
            Ok(n) => {
                if n == send_buffer.len() {
                    // All data written
                    send_buffer.clear();
                    let _ = stream.flush();
                    Ok(true)
                } else if n > 0 {
                    // Partial write - remove written portion
                    send_buffer.drain(0..n);
                    Ok(false) // Buffer not empty
                } else {
                    Err("Write returned 0".into())
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                Ok(false) // Buffer not empty, try again later
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Send a packet directly to the client (legacy, use queue_packet + flush instead)
    #[allow(dead_code)]
    fn send_packet_direct(stream: &mut TcpStream, packet: Packet) -> Result<(), String> {
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
