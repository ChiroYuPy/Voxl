//! Game state management
//!
//! Manages the connection to the server (embedded or remote) and game state.

use voxl_common::{
    VoxelWorld, SharedVoxelRegistry, EntityWorld, config::ServerMode, network::*,
};
use crate::networking::NetworkClient;
use crate::networking::async_task::{NetworkTask, NetworkEvent};
use crate::embedded_server::EmbeddedServer;
use std::sync::{Arc, RwLock};
use hecs::Entity;
use glam::Vec3;
use tracing::{info, error, warn};

/// Connection state
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Failed(String),
}

/// Game state - manages server connection
pub struct GameState {
    /// Network client (legacy, kept for compatibility)
    network_client: NetworkClient,
    /// Background network task
    network_task: Option<NetworkTask>,
    /// Embedded server (if running)
    embedded_server: Option<EmbeddedServer>,
    /// Connection state
    connection_state: ConnectionState,
    /// Our player entity
    player_entity: Option<Entity>,
    /// Voxel registry (for embedded server mode)
    registry: Option<SharedVoxelRegistry>,
    /// Our player ID
    player_id: Option<PlayerId>,
}

impl GameState {
    /// Creates a new game state
    pub fn new() -> Self {
        Self {
            network_client: NetworkClient::new(),
            network_task: None,
            embedded_server: None,
            connection_state: ConnectionState::Disconnected,
            player_entity: None,
            registry: None,
            player_id: None,
        }
    }

    /// Sets the voxel registry (must be called before starting in embedded mode)
    pub fn set_registry(&mut self, registry: SharedVoxelRegistry) {
        self.registry = Some(registry);
    }

    /// Starts the game in the given mode
    pub async fn start(&mut self, mode: ServerMode, address: String, port: u16, username: &str) -> Result<(), String> {
        info!("[GameState] Starting game: mode={:?}, address={}, port={}", mode, address, port);

        self.connection_state = ConnectionState::Connecting;

        // Start background network task
        self.network_task = Some(NetworkTask::start());

        match mode {
            ServerMode::Embedded => {
                // Start embedded server with shared registry
                // Use port 0 to auto-assign an available port
                let settings = voxl_common::ServerSettings {
                    port: 0,  // Auto-assign available port
                    ..Default::default()
                };

                let registry = self.registry.clone().ok_or_else(|| {
                    "Registry not set for embedded mode! Call set_registry() first.".to_string()
                })?;

                match EmbeddedServer::start_with_registry(settings, registry) {
                    Ok(server) => {
                        let actual_port = server.actual_port;
                        self.embedded_server = Some(server);
                        info!("[GameState] Embedded server started on port {}", actual_port);

                        // Give the server thread time to start listening before connecting
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        // Connect to embedded server (localhost) using actual port
                        let server_addr = format!("127.0.0.1:{}", actual_port);
                        info!("[GameState] Connecting to embedded server at {}", server_addr);
                        self.connect_to_server_async(&server_addr, username).await;

                        // Wait a bit for connection to establish
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        info!("[GameState] Connection state after waiting: {:?}", self.connection_state);
                    }
                    Err(e) => {
                        let msg = format!("Failed to start embedded server: {}", e);
                        error!("[GameState] {}", msg);
                        self.connection_state = ConnectionState::Failed(msg.clone());
                        return Err(msg);
                    }
                }
            }
            ServerMode::Remote => {
                // Connect to remote server
                let server_addr = format!("{}:{}", address, port);
                self.connect_to_server_async(&server_addr, username).await;
            }
        }

        Ok(())
    }

    /// Connects to a server (async, non-blocking)
    async fn connect_to_server_async(&mut self, address: &str, username: &str) {
        info!("[GameState] Connecting to server at '{}' as '{}'...", address, username);

        if let Some(task) = &self.network_task {
            task.connect(address.to_string(), username.to_string()).await;
        }
    }

    /// Process network events from background task (non-blocking)
    /// Call this in the main game loop to handle received packets
    pub fn process_network_events(&mut self) -> Vec<NetworkEvent> {
        if let Some(task) = &mut self.network_task {
            let events = task.drain_events();

            // Process events and update connection state
            for event in &events {
                match event {
                    NetworkEvent::Connected { server_name, player_id } => {
                        info!("[GameState] Connected to server: {} (player ID: {})", server_name, player_id);
                        self.connection_state = ConnectionState::Connected;
                        self.player_id = Some(*player_id);
                    }
                    NetworkEvent::ConnectionFailed { reason } => {
                        error!("[GameState] Connection failed: {}", reason);
                        self.connection_state = ConnectionState::Failed(reason.clone());
                    }
                    NetworkEvent::Disconnected { .. } => {
                        info!("[GameState] Disconnected from server");
                        self.connection_state = ConnectionState::Disconnected;
                        self.player_id = None;
                    }
                    _ => {}
                }
            }

            events
        } else {
            Vec::new()
        }
    }

    /// Disconnects from the server
    pub async fn disconnect(&mut self) {
        info!("[GameState] Disconnecting...");

        if let Some(task) = &self.network_task {
            task.disconnect().await;
        }

        if let Some(server) = self.embedded_server.take() {
            server.stop();
        }

        self.connection_state = ConnectionState::Disconnected;
        self.player_entity = None;
        self.player_id = None;

        info!("[GameState] Disconnected");
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
    ) -> Result<(), String> {
        if !self.is_connected() {
            return Ok(());
        }

        if let Some(task) = &self.network_task {
            let update = PlayerUpdatePacket {
                sequence,
                x, y, z,
                yaw, pitch,
                on_ground,
            };
            task.try_send_player_update(update);
            Ok(())
        } else {
            Err("Network task not running".to_string())
        }
    }

    /// Sends block action to server (non-blocking)
    pub fn send_block_action(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        action: voxl_common::network::BlockActionType,
        sequence: u32,
    ) -> Result<(), String> {
        if !self.is_connected() {
            return Ok(());
        }

        if let Some(task) = &self.network_task {
            let block_action = BlockActionPacket {
                sequence,
                x, y, z,
                action,
            };
            task.try_send_block_action(block_action);
            Ok(())
        } else {
            Err("Network task not running".to_string())
        }
    }

    /// Sends command to server (non-blocking)
    pub fn send_command(&mut self, command: String) -> Result<(), String> {
        if !self.is_connected() {
            return Err("Not connected to server".to_string());
        }

        if let Some(task) = &self.network_task {
            task.try_send_command(command);
            Ok(())
        } else {
            Err("Network task not running".to_string())
        }
    }

    /// Sends chat message to server (non-blocking)
    pub fn send_chat_message(&mut self, message: String) -> Result<(), String> {
        info!("[GameState] send_chat_message called with: '{}'", message);
        if !self.is_connected() {
            return Err("Not connected to server".to_string());
        }

        if let Some(task) = &self.network_task {
            task.try_send_chat_message(message);
            info!("[GameState] try_send_chat_message called");
            Ok(())
        } else {
            Err("Network task not running".to_string())
        }
    }

    /// Requests chunks from server (non-blocking)
    pub fn request_chunks(&mut self, chunks: Vec<(i32, i32, i32)>) -> Result<(), String> {
        if !self.is_connected() {
            return Err("Not connected to server".to_string());
        }

        if let Some(task) = &self.network_task {
            task.try_send_chunk_request(chunks);
            Ok(())
        } else {
            Err("Network task not running".to_string())
        }
    }

    /// Receives and processes a packet from server (deprecated - use process_network_events)
    #[deprecated(note = "Use process_network_events instead")]
    pub async fn receive_packet(&mut self, _timeout_ms: u64) -> Result<Option<voxl_common::network::Packet>, String> {
        // This method is kept for compatibility but events should be processed via process_network_events
        Ok(None)
    }

    /// Returns true if connected to server
    pub fn is_connected(&self) -> bool {
        self.connection_state == ConnectionState::Connected
    }

    /// Returns true if running in embedded mode (single player)
    pub fn is_embedded_mode(&self) -> bool {
        self.embedded_server.is_some()
    }

    /// Returns the current connection state
    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
    }

    /// Returns our player ID
    pub fn player_id(&self) -> Option<u32> {
        self.player_id
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}
