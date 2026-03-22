//! Network client for connecting to voxl server
//!
//! Manages TCP connection, packet sending, and server communication.

use voxl_common::network::*;
use tracing::{info, error};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::time::Duration;

/// Network client for server communication
pub struct NetworkClient {
    /// TCP stream to server
    stream: Option<TcpStream>,
    /// Our player ID
    player_id: Option<PlayerId>,
    /// Server name
    server_name: Option<String>,
    /// Connected state
    connected: bool,
}

impl NetworkClient {
    /// Creates a new network client
    pub fn new() -> Self {
        Self {
            stream: None,
            player_id: None,
            server_name: None,
            connected: false,
        }
    }

    /// Connects to a server
    pub async fn connect(&mut self, address: &str, username: &str) -> Result<(), String> {
        info!("[Network] Connecting to server at {}", address);

        // Connect to server
        let mut stream = TcpStream::connect(address).await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        info!("[Network] Connected! Performing handshake...");

        // Send handshake
        let handshake = Packet::new(PacketPayload::Handshake(HandshakePacket {
            protocol_version: PROTOCOL_VERSION,
            username: username.to_string(),
        }));

        send_packet(&mut stream, &handshake).await
            .map_err(|e| format!("Failed to send handshake: {}", e))?;

        // Wait for response
        let response = receive_packet(&mut stream).await
            .map_err(|e| format!("Failed to receive handshake response: {}", e))?;

        match response.payload {
            PacketPayload::HandshakeAccept(accept) => {
                info!("[Network] Handshake accepted!");
                info!("[Network] Server: {}", accept.server_name);
                info!("[Network] MOTD: {}", accept.motd);
                info!("[Network] Player ID: {}", accept.player_id);

                self.player_id = Some(accept.player_id);
                self.server_name = Some(accept.server_name);
                self.stream = Some(stream);
                self.connected = true;

                Ok(())
            }
            PacketPayload::HandshakeReject(reject) => {
                let msg = format!("Connection rejected: {}", reject.reason);
                error!("[Network] {}", msg);
                Err(msg)
            }
            _ => {
                let msg = format!("Unexpected packet during handshake: {:?}", response.header.packet_type);
                error!("[Network] {}", msg);
                Err(msg)
            }
        }
    }

    /// Disconnects from the server
    pub async fn disconnect(&mut self) {
        if !self.connected {
            return;
        }

        info!("[Network] Disconnecting from server...");

        // Send disconnect packet
        if let Some(stream) = &mut self.stream {
            let disconnect = Packet::new(PacketPayload::Disconnect(DisconnectPacket {
                reason: DisconnectReason::Left,
            }));

            let _ = send_packet(stream, &disconnect).await;
        }

        self.connected = false;
        self.stream = None;
        self.player_id = None;
        self.server_name = None;
    }

    /// Sends a player update packet
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
        if !self.connected || self.stream.is_none() {
            return Err("Not connected to server".to_string());
        }

        let packet = Packet::new(PacketPayload::PlayerUpdate(PlayerUpdatePacket {
            sequence,
            x, y, z,
            yaw, pitch,
            on_ground,
        }));

        send_packet(self.stream.as_mut().unwrap(), &packet).await
            .map_err(|e| e.to_string())
    }

    /// Sends a block action packet
    pub async fn send_block_action(
        &mut self,
        x: i32,
        y: i32,
        z: i32,
        action: BlockActionType,
        sequence: u32,
    ) -> Result<(), String> {
        if !self.connected || self.stream.is_none() {
            return Err("Not connected to server".to_string());
        }

        let packet = Packet::new(PacketPayload::BlockAction(BlockActionPacket {
            sequence,
            x, y, z,
            action,
        }));

        send_packet(self.stream.as_mut().unwrap(), &packet).await
            .map_err(|e| e.to_string())
    }

    /// Sends a chunk request packet
    pub async fn request_chunks(&mut self, chunks: Vec<(i32, i32, i32)>) -> Result<(), String> {
        if !self.connected || self.stream.is_none() {
            return Err("Not connected to server".to_string());
        }

        let packet = Packet::new(PacketPayload::ChunkRequest(ChunkRequestPacket {
            chunks,
        }));

        send_packet(self.stream.as_mut().unwrap(), &packet).await
            .map_err(|e| e.to_string())
    }

    /// Sends a chat message
    pub async fn send_chat(&mut self, message: String) -> Result<(), String> {
        if !self.connected || self.stream.is_none() {
            return Err("Not connected to server".to_string());
        }

        let packet = Packet::new(PacketPayload::ChatMessage(ChatMessagePacket {
            message,
        }));

        send_packet(self.stream.as_mut().unwrap(), &packet).await
            .map_err(|e| e.to_string())
    }

    /// Receives a packet from the server (with timeout)
    pub async fn receive_packet(&mut self, timeout_ms: u64) -> Result<Option<Packet>, String> {
        if !self.connected || self.stream.is_none() {
            return Err("Not connected to server".to_string());
        }

        let stream = self.stream.as_mut().unwrap();

        match tokio::time::timeout(
            Duration::from_millis(timeout_ms),
            receive_packet(stream)
        ).await {
            Ok(Ok(packet)) => Ok(Some(packet)),
            Ok(Err(e)) => Err(e.to_string()),
            Err(_) => Ok(None), // Timeout
        }
    }

    /// Returns true if connected to server
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Returns our player ID
    pub fn player_id(&self) -> Option<PlayerId> {
        self.player_id
    }

    /// Returns server name
    pub fn server_name(&self) -> Option<&str> {
        self.server_name.as_deref()
    }
}

impl Default for NetworkClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Sends a packet to a stream
pub async fn send_packet<S>(stream: &mut S, packet: &Packet) -> Result<(), Box<dyn std::error::Error>>
where
    S: AsyncWriteExt + Unpin,
{
    let bytes = packet.to_bytes()?;

    // Send packet length first (u32)
    let len = bytes.len() as u32;
    stream.write_all(&len.to_be_bytes()).await?;

    // Send packet data
    stream.write_all(&bytes).await?;

    // Flush to ensure data is sent
    stream.flush().await?;

    Ok(())
}

/// Receives a packet from a stream
pub async fn receive_packet<S>(stream: &mut S) -> Result<Packet, Box<dyn std::error::Error>>
where
    S: AsyncReadExt + Unpin,
{
    // Read packet length
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).await?;
    let len = u32::from_be_bytes(len_bytes) as usize;

    // Sanity check
    if len > 10_000_000 {
        return Err("Packet too large".into());
    }

    // Read packet data
    let mut buffer = vec![0u8; len];
    stream.read_exact(&mut buffer).await?;

    // Deserialize
    let packet = Packet::from_bytes(&buffer)?;
    Ok(packet)
}
