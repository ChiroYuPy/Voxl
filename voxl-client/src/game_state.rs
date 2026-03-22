//! Game state management
//!
//! Manages the connection to the server (embedded or remote) and game state.

use voxl_common::{
    VoxelWorld, SharedVoxelRegistry, EntityWorld, config::ServerMode,
};
use crate::networking::NetworkClient;
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
    /// Network client (for remote or embedded)
    network_client: NetworkClient,
    /// Embedded server (if running)
    embedded_server: Option<EmbeddedServer>,
    /// Connection state
    connection_state: ConnectionState,
    /// Our player entity
    player_entity: Option<Entity>,
    /// Voxel registry (for embedded server mode)
    registry: Option<SharedVoxelRegistry>,
}

impl GameState {
    /// Creates a new game state
    pub fn new() -> Self {
        Self {
            network_client: NetworkClient::new(),
            embedded_server: None,
            connection_state: ConnectionState::Disconnected,
            player_entity: None,
            registry: None,
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

        match mode {
            ServerMode::Embedded => {
                // Start embedded server with shared registry
                let settings = voxl_common::ServerSettings {
                    port,
                    ..Default::default()
                };

                let registry = self.registry.clone().ok_or_else(|| {
                    "Registry not set for embedded mode! Call set_registry() first.".to_string()
                })?;

                match EmbeddedServer::start_with_registry(settings, registry) {
                    Ok(server) => {
                        self.embedded_server = Some(server);
                        info!("[GameState] Embedded server started");
                    }
                    Err(e) => {
                        let msg = format!("Failed to start embedded server: {}", e);
                        error!("[GameState] {}", msg);
                        self.connection_state = ConnectionState::Failed(msg.clone());
                        return Err(msg);
                    }
                }

                // Give the server thread time to start listening before connecting
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                // Connect to embedded server (localhost)
                let server_addr = format!("127.0.0.1:{}", port);
                self.connect_to_server(&server_addr, username).await?;
            }
            ServerMode::Remote => {
                // Connect to remote server
                let server_addr = format!("{}:{}", address, port);
                self.connect_to_server(&server_addr, username).await?;
            }
        }

        Ok(())
    }

    /// Connects to a server
    async fn connect_to_server(&mut self, address: &str, username: &str) -> Result<(), String> {
        info!("[GameState] Connecting to server at '{}' as '{}'...", address, username);

        match self.network_client.connect(address, username).await {
            Ok(()) => {
                self.connection_state = ConnectionState::Connected;
                info!("[GameState] Connected to server!");
                Ok(())
            }
            Err(e) => {
                let msg = format!("Failed to connect: {}", e);
                error!("[GameState] {}", msg);
                self.connection_state = ConnectionState::Failed(msg.clone());
                Err(msg)
            }
        }
    }

    /// Disconnects from the server
    pub async fn disconnect(&mut self) {
        info!("[GameState] Disconnecting...");

        self.network_client.disconnect().await;

        if let Some(server) = self.embedded_server.take() {
            server.stop();
        }

        self.connection_state = ConnectionState::Disconnected;
        self.player_entity = None;

        info!("[GameState] Disconnected");
    }

    /// Sends player update to server
    pub async fn send_player_update(
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
            return Ok(()); // Silently ignore if not connected
        }

        self.network_client.send_player_update(x, y, z, yaw, pitch, on_ground, sequence).await
    }

    /// Sends block action to server
    pub async fn send_block_action(
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

        self.network_client.send_block_action(x, y, z, action, sequence).await
    }

    /// Requests chunks from server
    pub async fn request_chunks(&mut self, chunks: Vec<(i32, i32, i32)>) -> Result<(), String> {
        if !self.is_connected() {
            return Ok(());
        }

        self.network_client.request_chunks(chunks).await
    }

    /// Receives and processes a packet from server (with timeout)
    pub async fn receive_packet(&mut self, timeout_ms: u64) -> Result<Option<voxl_common::network::Packet>, String> {
        if !self.is_connected() {
            return Ok(None);
        }

        self.network_client.receive_packet(timeout_ms).await
    }

    /// Returns true if connected to server
    pub fn is_connected(&self) -> bool {
        self.connection_state == ConnectionState::Connected
    }

    /// Returns the current connection state
    pub fn connection_state(&self) -> &ConnectionState {
        &self.connection_state
    }

    /// Returns our player ID
    pub fn player_id(&self) -> Option<u32> {
        self.network_client.player_id()
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}
