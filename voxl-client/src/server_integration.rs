//! Server integration module
//!
//! Handles server communication and packet processing for the client.

use voxl_common::{
    VoxelWorld, network::*,
    entities::EntityWorld,
    ChatMessage,
};
use crate::game_state::GameState;
use crate::server_chunk_tracker::ChunkTracker;
use crate::networking::async_task::NetworkEvent;
use std::sync::Arc;
use std::sync::RwLock;
use std::collections::VecDeque;
use tracing::{info, warn, error, debug};

/// Command response waiting to be processed by the UI
#[derive(Debug, Clone)]
pub struct PendingCommandResponse {
    pub success: bool,
    pub message: ChatMessage,
}

/// Processes a packet received from the server
pub fn process_server_packet(
    packet: Packet,
    world: &Arc<RwLock<VoxelWorld>>,
    entities: &EntityWorld,
    chunk_tracker: &Arc<ChunkTracker>,
) {
    match packet.payload {
        PacketPayload::Ping(_ping) => {
            // Server sent ping - ignore for now
            debug!("[Server] Received ping");
        }

        PacketPayload::PlayerPosition(pos) => {
            // Another player moved
            info!("[Server] Player {} moved to ({:.1}, {:.1}, {:.1})",
                pos.player_id, pos.x, pos.y, pos.z);
            // TODO: Update other player entity position
        }

        PacketPayload::BlockChange(change) => {
            // Block was changed by server
            debug!("[Server] Block changed at ({},{},{}) -> {}",
                change.x, change.y, change.z, change.block_id);

            // Update world
            let mut world = world.write().unwrap();
            let result = world.set_voxel(change.x, change.y, change.z, Some(change.block_id));

            // Mark chunk as dirty for remeshing
            let cx = result.modified_chunk.0;
            let cy = result.modified_chunk.1;
            let cz = result.modified_chunk.2;
            chunk_tracker.on_server_update((cx, cy, cz));

            debug!("[Server] Chunk ({},{},{}) marked dirty after block change",
                cx, cy, cz);
        }

        PacketPayload::ChunkData(data) => {
            // Received chunk data from server
            debug!("[Server] Received chunk data for ({},{},{})", data.cx, data.cy, data.cz);

            // Deserialize chunk from bytes
            match voxl_common::VoxelChunk::from_bytes(&data.data) {
                Ok(chunk) => {
                    debug!("[Server] Inserting chunk ({},{},{})", data.cx, data.cy, data.cz);

                    // Insert into world
                    let mut world = world.write().unwrap();
                    world.insert_chunk(data.cx, data.cy, data.cz, chunk);

                    // Mark chunk as loaded - will trigger meshing
                    chunk_tracker.on_chunk_loaded((data.cx, data.cy, data.cz));
                }
                Err(e) => {
                    error!("[Server] Failed to deserialize chunk ({},{},{}): {}",
                        data.cx, data.cy, data.cz, e);
                }
            }
        }

        PacketPayload::EntitySpawn(spawn) => {
            info!("[Server] Entity {} spawned at ({:.1}, {:.1}, {:.1}), type: {:?}",
                spawn.entity_id, spawn.x, spawn.y, spawn.z, spawn.entity_type);
            // TODO: Spawn entity in client ECS
        }

        PacketPayload::EntityDespawn(despawn) => {
            info!("[Server] Entity {} despawned", despawn.entity_id);
            // TODO: Despawn entity from client ECS
        }

        PacketPayload::PlayerConnected(conn) => {
            info!("[Server] Player '{}' (ID: {}) connected at ({:.1}, {:.1}, {:.1})",
                conn.username, conn.player_id, conn.x, conn.y, conn.z);
            // TODO: Spawn player entity
        }

        PacketPayload::PlayerDisconnected(disconn) => {
            info!("[Server] Player {} disconnected: {:?}",
                disconn.player_id, disconn.reason);
            // TODO: Despawn player entity
        }

        PacketPayload::Kicked(kicked) => {
            error!("[Server] Kicked from server: {}", kicked.reason);
            // TODO: Handle kick - show message, return to menu
        }

        PacketPayload::ChatBroadcast(chat) => {
            info!("[Chat] {}: {}", chat.username, chat.message);
            // TODO: Add to chat log
        }

        _ => {
            warn!("[Server] Received unhandled packet type: {:?}", packet.header.packet_type);
        }
    }
}

/// Server integration manager
pub struct ServerIntegration {
    /// Connection state
    pub game_state: GameState,
    /// Chunk tracker
    pub chunk_tracker: Arc<ChunkTracker>,
    /// Pending command responses from server
    pending_responses: VecDeque<PendingCommandResponse>,
}

impl ServerIntegration {
    /// Creates a new server integration
    pub fn new() -> Self {
        Self {
            game_state: GameState::new(),
            chunk_tracker: Arc::new(ChunkTracker::new()),
            pending_responses: VecDeque::new(),
        }
    }

    /// Starts connection to server
    pub async fn start(&mut self, mode: voxl_common::config::ServerMode, address: String, port: u16, username: &str) -> Result<(), String> {
        self.game_state.start(mode, address, port, username).await
    }

    /// Processes network events from background task (non-blocking)
    /// Call this in the main game loop - NO BLOCKING!
    pub fn process_network_events(&mut self, world: &Arc<RwLock<VoxelWorld>>, entities: &EntityWorld) {
        let events = self.game_state.process_network_events();

        for event in events {
            match event {
                NetworkEvent::Packet(packet) => {
                    // Handle generic packets
                    debug!("[Server] Received packet: {:?}", packet.header.packet_type);
                    process_server_packet(packet, world, entities, &self.chunk_tracker);
                }
                NetworkEvent::PlayerUpdate(update) => {
                    // Handle player update (client->server packet, doesn't include player_id)
                    debug!("[Server] Received player update packet: ({:.1}, {:.1}, {:.1})",
                        update.x, update.y, update.z);
                    // TODO: Track player position if needed
                }
                NetworkEvent::BlockChange(change) => {
                    // Handle block change
                    debug!("[Server] Block changed at ({},{},{}) -> {}",
                        change.x, change.y, change.z, change.block_id);

                    let mut world = world.write().unwrap();
                    let result = world.set_voxel(change.x, change.y, change.z, Some(change.block_id));

                    // Mark chunk as dirty for remeshing
                    let cx = result.modified_chunk.0;
                    let cy = result.modified_chunk.1;
                    let cz = result.modified_chunk.2;
                    self.chunk_tracker.on_server_update((cx, cy, cz));

                    debug!("[Server] Chunk ({},{},{}) marked dirty after block change",
                        cx, cy, cz);
                }
                NetworkEvent::ChunkData(data) => {
                    // Handle chunk data
                    debug!("[Server] Received chunk data for ({},{},{})", data.cx, data.cy, data.cz);

                    match voxl_common::VoxelChunk::from_bytes(&data.data) {
                        Ok(chunk) => {
                            debug!("[Server] Inserting chunk ({},{},{})", data.cx, data.cy, data.cz);

                            let mut world = world.write().unwrap();
                            world.insert_chunk(data.cx, data.cy, data.cz, chunk);

                            // Mark chunk as loaded - will trigger meshing
                            self.chunk_tracker.on_chunk_loaded((data.cx, data.cy, data.cz));
                        }
                        Err(e) => {
                            error!("[Server] Failed to deserialize chunk ({},{},{}): {}",
                                data.cx, data.cy, data.cz, e);
                        }
                    }
                }
                NetworkEvent::EntitySpawn(spawn) => {
                    info!("[Server] Entity {} spawned at ({:.1}, {:.1}, {:.1}), type: {:?}",
                        spawn.entity_id, spawn.x, spawn.y, spawn.z, spawn.entity_type);
                    // TODO: Spawn entity in client ECS
                }
                NetworkEvent::PlayerConnected(conn) => {
                    info!("[Server] Player '{}' (ID: {}) connected at ({:.1}, {:.1}, {:.1})",
                        conn.username, conn.player_id, conn.x, conn.y, conn.z);
                    // TODO: Spawn player entity
                }
                NetworkEvent::PlayerDisconnected(disconn) => {
                    info!("[Server] Player {} disconnected: {:?}",
                        disconn.player_id, disconn.reason);
                    // TODO: Despawn player entity
                }
                NetworkEvent::Chat(chat) => {
                    info!("[Chat] {}: {}", chat.username, chat.message);
                    // TODO: Add to chat log
                }
                NetworkEvent::Kicked(kicked) => {
                    error!("[Server] Kicked from server: {}", kicked.reason);
                    // TODO: Handle kick - show message, return to menu
                }
                NetworkEvent::Connected { server_name, player_id } => {
                    info!("[Server] Connected to {} (player ID: {})", server_name, player_id);
                }
                NetworkEvent::ConnectionFailed { reason } => {
                    error!("[Server] Connection failed: {}", reason);
                }
                NetworkEvent::Disconnected { reason } => {
                    info!("[Server] Disconnected: {:?}", reason);
                }
                NetworkEvent::CommandResponse(response) => {
                    info!("[Server] Command response: success={}, message={}", response.success, response.message);
                    // Store response for UI to process
                    self.pending_responses.push_back(PendingCommandResponse {
                        success: response.success,
                        message: response.message,
                    });
                }
            }
        }
    }

    /// Processes packets from server (DEPRECATED - use process_network_events)
    #[deprecated(note = "Use process_network_events instead")]
    pub async fn process_packets(
        &mut self,
        _world: &Arc<RwLock<VoxelWorld>>,
        _entities: &EntityWorld,
    ) {
        // This is now a no-op - use process_network_events from the main loop instead
    }

    /// Sends player update to server (non-blocking)
    pub fn send_player_update(
        &mut self,
        x: f32,
        y: f32,
        z: f32,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
        sequence: u32,
    ) {
        if let Err(e) = self.game_state.send_player_update(x, y, z, yaw, pitch, on_ground, sequence) {
            error!("[Network] Failed to send player update: {}", e);
        }
    }

    /// Sends block action to server (non-blocking)
    pub fn send_block_action(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        action: BlockActionType,
        sequence: u32,
    ) {
        if let Err(e) = self.game_state.send_block_action(x, y, z, action, sequence) {
            error!("[Network] Failed to send block action: {}", e);
        }
    }

    /// Returns true if connected to server
    pub fn is_connected(&self) -> bool {
        self.game_state.is_connected()
    }

    /// Drains all pending command responses
    pub fn drain_command_responses(&mut self) -> Vec<PendingCommandResponse> {
        std::mem::take(&mut self.pending_responses).into_iter().collect()
    }

    /// Sends command to server (non-blocking)
    pub fn send_command(&mut self, command: String) -> Result<(), String> {
        self.game_state.send_command(command)
    }
}

impl Default for ServerIntegration {
    fn default() -> Self {
        Self::new()
    }
}
