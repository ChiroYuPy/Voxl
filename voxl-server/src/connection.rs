//! Connection management
//!
//! Handles TCP connections to clients and packet routing.

use voxl_common::{
    VoxelWorld, SharedVoxelRegistry,
    EntityWorld, ServerSettings,
    entities::{Position, Velocity, LookDirection},
    network::*,
};
use tracing::{info, warn, error};
use std::sync::{Arc, RwLock};
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use crate::player::{ServerPlayer, spawn_player_entity, despawn_player_entity};

/// Next player ID (atomic for thread safety)
static NEXT_PLAYER_ID: AtomicU32 = AtomicU32::new(1);

/// Connection manager tracking all active connections
#[derive(Clone)]
pub struct ConnectionManager {
    players: Arc<RwLock<HashMap<PlayerId, ServerPlayer>>>,
    max_players: usize,
}

impl ConnectionManager {
    pub fn new(max_players: usize) -> Self {
        Self {
            players: Arc::new(RwLock::new(HashMap::new())),
            max_players,
        }
    }

    /// Gets the number of connected players
    pub fn player_count(&self) -> usize {
        self.players.read().unwrap().len()
    }

    /// Checks if server is full
    pub fn is_full(&self) -> bool {
        self.player_count() >= self.max_players
    }

    /// Adds a new player
    pub fn add_player(&self, player_id: PlayerId, username: String) -> Result<(), String> {
        let mut players = self.players.write().unwrap();

        if players.contains_key(&player_id) {
            return Err(format!("Player ID {} already exists", player_id));
        }

        let player = ServerPlayer::new(player_id, username);
        players.insert(player_id, player);

        Ok(())
    }

    /// Removes a player
    pub fn remove_player(&self, player_id: PlayerId) -> Option<ServerPlayer> {
        let mut players = self.players.write().unwrap();
        players.remove(&player_id)
    }

    /// Gets a player by ID
    pub fn get_player(&self, player_id: PlayerId) -> Option<ServerPlayer> {
        let players = self.players.read().unwrap();
        players.get(&player_id).cloned()
    }

    /// Gets all connected players
    pub fn get_all_players(&self) -> Vec<ServerPlayer> {
        let players = self.players.read().unwrap();
        players.values().cloned().collect()
    }

    /// Gets the players Arc reference
    pub fn players_arc(&self) -> Arc<RwLock<HashMap<PlayerId, ServerPlayer>>> {
        self.players.clone()
    }
}

/// Handles a client connection
pub async fn handle_connection(
    mut stream: TcpStream,
    addr: SocketAddr,
    world: Arc<RwLock<VoxelWorld>>,
    entities: Arc<RwLock<EntityWorld>>,
    registry: SharedVoxelRegistry,
    settings: ServerSettings,
    connections: ConnectionManager,
) {
    info!("[Handler] Connection handler started for {}", addr);

    // Allocate player ID
    let player_id = NEXT_PLAYER_ID.fetch_add(1, Ordering::SeqCst);

    // Phase 1: Handshake
    let handshake_result = handle_handshake(&mut stream, player_id, &settings, &connections).await;

    let username = match handshake_result {
        Ok(username) => username,
        Err(_) => {
            // Handshake failed, connection already closed
            warn!("[Handler] Handshake failed for {}, closing connection", addr);
            return;
        }
    };

    // Phase 2: Connection established - spawn player entity
    let entity = spawn_player_entity(&entities, player_id, &username);

    // Update player with entity reference
    {
        let mut players = connections.players.write().unwrap();
        if let Some(player) = players.get_mut(&player_id) {
            player.entity = Some(entity);
        }
    }

    info!("[Handler] Player '{}' (ID: {}) fully connected from {}",
        username, player_id, addr);

    // TODO: Send EntitySpawn packet to other clients

    // Phase 3: Main game loop - handle packets
    let game_loop_result = game_loop(&mut stream, player_id, &username, &world, &entities, &registry, &settings, &connections).await;

    // Phase 4: Cleanup (disconnection)
    info!("[Handler] Player '{}' (ID: {}) disconnecting: {:?}",
        username, player_id, game_loop_result);

    // Remove player entity
    if let Some(player) = connections.remove_player(player_id) {
        if let Some(entity) = player.entity {
            despawn_player_entity(&entities, entity, &username);
        }

        // TODO: Broadcast PlayerDisconnected packet to all clients
    }

    info!("[Handler] Connection handler ended for {}", addr);
}

/// Handles the handshake phase
async fn handle_handshake(
    stream: &mut TcpStream,
    player_id: PlayerId,
    settings: &ServerSettings,
    connections: &ConnectionManager,
) -> Result<String, ()> {
    // Wait for handshake packet
    let packet = match receive_packet(stream).await {
        Ok(p) => p,
        Err(e) => {
            error!("[Handler] Failed to receive handshake: {}", e);
            return Err(());
        }
    };

    if !packet.is_valid() {
        error!("[Handler] Invalid handshake packet");
        return Err(());
    }

    let handshake = match packet.payload {
        PacketPayload::Handshake(h) => h,
        _ => {
            error!("[Handler] Expected Handshake packet");
            return Err(());
        }
    };

    let username = handshake.username.clone();

    // Check protocol version
    if handshake.protocol_version != PROTOCOL_VERSION {
        warn!("[Handler] Protocol mismatch: expected {}, got {}",
            PROTOCOL_VERSION, handshake.protocol_version);

        let reject_packet = Packet::new(PacketPayload::HandshakeReject(HandshakeRejectPacket {
            reason: format!("Protocol version mismatch. Server: {}, Client: {}",
                PROTOCOL_VERSION, handshake.protocol_version),
        }));

        let _ = send_packet(stream, &reject_packet).await;
        return Err(());
    }

    // Check player limit
    if connections.is_full() {
        warn!("[Handler] Server full, rejecting player '{}'", username);

        let reject_packet = Packet::new(PacketPayload::HandshakeReject(HandshakeRejectPacket {
            reason: "Server is full".to_string(),
        }));

        let _ = send_packet(stream, &reject_packet).await;
        return Err(());
    }

    // Add player
    if let Err(e) = connections.add_player(player_id, username.clone()) {
        warn!("[Handler] Failed to add player '{}': {}", username, e);

        let reject_packet = Packet::new(PacketPayload::HandshakeReject(HandshakeRejectPacket {
            reason: e,
        }));

        let _ = send_packet(stream, &reject_packet).await;
        return Err(());
    }

    // Send handshake accept
    let accept_packet = Packet::new(PacketPayload::HandshakeAccept(HandshakeAcceptPacket {
        player_id,
        server_name: settings.server_name.clone(),
        motd: settings.motd.clone(),
    }));

    if let Err(e) = send_packet(stream, &accept_packet).await {
        error!("[Handler] Failed to send handshake accept: {}", e);
        // Remove player since we couldn't complete handshake
        connections.remove_player(player_id);
        return Err(());
    }

    Ok(username)
}

/// Main game loop for a connected client
async fn game_loop(
    stream: &mut TcpStream,
    player_id: PlayerId,
    username: &str,
    world: &Arc<RwLock<VoxelWorld>>,
    entities: &Arc<RwLock<EntityWorld>>,
    registry: &SharedVoxelRegistry,
    settings: &ServerSettings,
    connections: &ConnectionManager,
) -> DisconnectReason {
    let mut last_ping = std::time::Instant::now();
    let ping_interval = Duration::from_secs(30);

    loop {
        // Set read timeout (30 seconds)
        let read_result = tokio::time::timeout(
            Duration::from_secs(30),
            receive_packet(stream)
        ).await;

        let packet = match read_result {
            Ok(Ok(packet)) => packet,
            Ok(Err(e)) => {
                error!("[Handler] Read error for '{}': {}", username, e);
                return DisconnectReason::Error;
            }
            Err(_) => {
                warn!("[Handler] Connection timeout for '{}'", username);
                return DisconnectReason::TimedOut;
            }
        };

        // Process packet
        let result = process_packet(packet, player_id, username, world, entities, registry, settings, connections).await;

        match result {
            Ok(should_continue) => {
                if !should_continue {
                    return DisconnectReason::Left;
                }
            }
            Err(reason) => {
                return reason;
            }
        }

        // Send keepalive ping every 30 seconds
        if last_ping.elapsed() >= ping_interval {
            let ping_packet = Packet::new(PacketPayload::Ping(PingPacket {
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64,
            }));

            if let Err(e) = send_packet(stream, &ping_packet).await {
                error!("[Handler] Failed to send ping: {}", e);
                return DisconnectReason::Error;
            }

            last_ping = std::time::Instant::now();
        }
    }
}

/// Processes received packet
async fn process_packet(
    packet: Packet,
    player_id: PlayerId,
    username: &str,
    world: &Arc<RwLock<VoxelWorld>>,
    entities: &Arc<RwLock<EntityWorld>>,
    registry: &SharedVoxelRegistry,
    settings: &ServerSettings,
    connections: &ConnectionManager,
) -> Result<bool, DisconnectReason> {
    if !packet.is_valid() {
        error!("[Handler] Invalid packet from '{}'", username);
        return Ok(false);
    }

    match &packet.payload {
        PacketPayload::PlayerUpdate(update) => {
            // Update player entity in ECS
            if let Some(player) = connections.get_player(player_id) {
                if let Some(entity) = player.entity {
                    let mut entities = entities.write().unwrap();

                    if let Ok(mut pos) = entities.ecs_world.query_one_mut::<&mut Position>(entity) {
                        pos.set(glam::Vec3::new(update.x, update.y, update.z));
                    }

                    if let Ok(mut look) = entities.ecs_world.query_one_mut::<&mut LookDirection>(entity) {
                        look.yaw = update.yaw;
                        look.pitch = update.pitch;
                    }

                    if let Ok(_) = entities.ecs_world.query_one_mut::<&mut Velocity>(entity) {
                        // Velocity will be updated by physics system
                        // For now, we just store the input direction
                    }

                    // TODO: Broadcast PlayerPosition to all other clients
                }
            }
        }

        PacketPayload::BlockAction(action) => {
            // Process block place/break
            let mut world = world.write().unwrap();

            match action.action {
                BlockActionType::Place(block_id) => {
                    let _ = world.set_voxel(action.x, action.y, action.z, Some(block_id as usize));

                    // TODO: Broadcast BlockChange to all clients
                    info!("[Handler] '{}' placed block {} at ({},{},{})", username, block_id, action.x, action.y, action.z);
                }
                BlockActionType::Break => {
                    let _ = world.set_voxel(action.x, action.y, action.z, None);

                    // TODO: Broadcast BlockChange to all clients
                    info!("[Handler] '{}' broke block at ({},{},{})", username, action.x, action.y, action.z);
                }
            }
        }

        PacketPayload::ChunkRequest(request) => {
            // TODO: Send requested chunks to client
            info!("[Handler] '{}' requested {} chunks", username, request.chunks.len());
        }

        PacketPayload::ChatMessage(msg) => {
            // Broadcast chat message to all clients
            info!("[Chat] '{}: {}'", username, msg.message);

            // TODO: Implement chat broadcasting
        }

        PacketPayload::Pong(pong) => {
            // Received pong response
        }

        PacketPayload::Disconnect(_) => {
            info!("[Handler] '{}' is disconnecting", username);
            return Ok(false);
        }

        _ => {
            warn!("[Handler] Unexpected packet from '{}'", username);
        }
    }

    Ok(true)
}

/// Sends a packet to a stream
pub async fn send_packet(stream: &mut TcpStream, packet: &Packet) -> Result<(), String> {
    let bytes = packet.to_bytes()
        .map_err(|e| format!("Failed to serialize: {}", e))?;

    // Send packet length first (u32)
    let len = bytes.len() as u32;
    stream.write_all(&len.to_be_bytes()).await
        .map_err(|e| format!("Failed to write length: {}", e))?;

    // Send packet data
    stream.write_all(&bytes).await
        .map_err(|e| format!("Failed to write data: {}", e))?;

    // Flush to ensure data is sent
    stream.flush().await
        .map_err(|e| format!("Failed to flush: {}", e))?;

    Ok(())
}

/// Receives a packet from a stream
pub async fn receive_packet(stream: &mut TcpStream) -> Result<Packet, String> {
    // Read packet length
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await
        .map_err(|e| format!("Failed to read length: {}", e))?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    // Sanity check
    if len > 10_000_000 {
        return Err("Packet too large".to_string());
    }

    // Read packet data
    let mut buffer = vec![0u8; len];
    stream.read_exact(&mut buffer).await
        .map_err(|e| format!("Failed to read data: {}", e))?;

    // Deserialize
    let packet = Packet::from_bytes(&buffer)
        .map_err(|e| format!("Failed to deserialize: {}", e))?;
    Ok(packet)
}
