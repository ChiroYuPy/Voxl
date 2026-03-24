//! Background networking with tokio and event-based architecture
//!
//! The networking runs in a background task and communicates via channels.

use tokio::sync::mpsc::{self, Sender, Receiver};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, error, debug, warn};
use voxl_common::network::*;

// Re-export necessary functions from client module
use crate::networking::client::{send_packet, receive_packet};

/// Maximum packets to buffer in channels
const PACKET_CHANNEL_SIZE: usize = 1000;

/// Events sent from network thread to main thread
#[derive(Debug, Clone)]
pub enum NetworkEvent {
    /// Connected to server
    Connected {
        server_name: String,
        player_id: PlayerId,
    },
    /// Connection failed
    ConnectionFailed {
        reason: String,
    },
    /// Disconnected from server
    Disconnected {
        reason: DisconnectReason,
    },
    /// Packet received
    Packet(Packet),
    /// Player update received
    PlayerUpdate(PlayerUpdatePacket),
    /// Block changed on server
    BlockChange(BlockChangePacket),
    /// Chunk data received
    ChunkData(ChunkDataPacket),
    /// Entity spawned
    EntitySpawn(EntitySpawnPacket),
    /// Player connected
    PlayerConnected(PlayerConnectedPacket),
    /// Player disconnected
    PlayerDisconnected(PlayerDisconnectedPacket),
    /// Chat message
    Chat(ChatBroadcastPacket),
    /// Kicked from server
    Kicked(KickedPacket),
    /// Command response from server
    CommandResponse(CommandResponsePacket),
}

/// Commands sent from main thread to network thread
#[derive(Debug)]
pub enum NetworkCommand {
    /// Connect to server
    Connect {
        address: String,
        username: String,
    },
    /// Disconnect from server
    Disconnect,
    /// Send packet
    SendPacket(Packet),
    /// Send player update
    SendPlayerUpdate(PlayerUpdatePacket),
    /// Send block action
    SendBlockAction(BlockActionPacket),
    /// Send command request
    SendCommandRequest(CommandRequestPacket),
}

/// Background networking task
pub struct NetworkTask {
    /// Command sender
    command_tx: Sender<NetworkCommand>,
    /// Event receiver
    event_rx: Receiver<NetworkEvent>,
    /// Running flag
    running: Arc<AtomicBool>,
}

impl NetworkTask {
    /// Start the background network task
    pub fn start() -> Self {
        let (command_tx, mut command_rx) = mpsc::channel::<NetworkCommand>(100);
        let (event_tx, event_rx) = mpsc::channel::<NetworkEvent>(PACKET_CHANNEL_SIZE);

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = running.clone();

        // Spawn background task
        tokio::spawn(async move {
            info!("[NetworkTask] Background task started");

            let mut stream: Option<TcpStream> = None;
            let mut connected = false;

            while running_clone.load(Ordering::Relaxed) {
                // Process commands from main thread
                if let Ok(mut cmd) = command_rx.try_recv() {
                    match cmd {
                        NetworkCommand::Connect { address, username } => {
                            debug!("[NetworkTask] Connecting to {}", address);
                            match Self::do_connect(&address, &username).await {
                                Ok((s, server_name, player_id)) => {
                                    stream = Some(s);
                                    connected = true;
                                    let _ = event_tx.send(NetworkEvent::Connected {
                                        server_name,
                                        player_id,
                                    }).await;
                                }
                                Err(e) => {
                                    let _ = event_tx.send(NetworkEvent::ConnectionFailed {
                                        reason: e,
                                    }).await;
                                }
                            }
                        }
                        NetworkCommand::Disconnect => {
                            if let Some(mut s) = stream.take() {
                                let _ = s.shutdown().await;
                            }
                            connected = false;
                            let _ = event_tx.send(NetworkEvent::Disconnected {
                                reason: DisconnectReason::Left,
                            }).await;
                        }
                        NetworkCommand::SendPacket(packet) => {
                            if let Some(s) = &mut stream {
                                if send_packet(s, &packet).await.is_err() {
                                    error!("[NetworkTask] Failed to send packet");
                                    // Connection lost
                                    stream = None;
                                    connected = false;
                                    let _ = event_tx.send(NetworkEvent::Disconnected {
                                        reason: DisconnectReason::TimedOut,
                                    }).await;
                                }
                            }
                        }
                        NetworkCommand::SendPlayerUpdate(update) => {
                            if let Some(s) = &mut stream {
                                let packet = Packet::new(PacketPayload::PlayerUpdate(update));
                                if send_packet(s, &packet).await.is_err() {
                                    debug!("[NetworkTask] Failed to send player update");
                                }
                            }
                        }
                        NetworkCommand::SendBlockAction(action) => {
                            if let Some(s) = &mut stream {
                                let packet = Packet::new(PacketPayload::BlockAction(action));
                                if send_packet(s, &packet).await.is_err() {
                                    debug!("[NetworkTask] Failed to send block action");
                                }
                            }
                        }
                        NetworkCommand::SendCommandRequest(request) => {
                            if let Some(s) = &mut stream {
                                let packet = Packet::new(PacketPayload::CommandRequest(request));
                                if send_packet(s, &packet).await.is_err() {
                                    debug!("[NetworkTask] Failed to send command request");
                                }
                            }
                        }
                    }
                }

                // Try to receive a packet if connected
                if connected {
                    if let Some(s) = &mut stream {
                        match Self::try_receive_packet(s).await {
                            Ok(Some(packet)) => {
                                // Convert packet to event
                                if let Some(event) = Self::packet_to_event(packet) {
                                    let _ = event_tx.send(event).await;
                                }
                            }
                            Ok(None) => {
                                // No packet available
                            }
                            Err(e) => {
                                if !e.contains("unexpected end of file") && !e.contains("would block") {
                                    error!("[NetworkTask] Receive error: {}", e);
                                }
                                if e.contains("connection reset") || e.contains("broken pipe") {
                                    stream = None;
                                    connected = false;
                                    let _ = event_tx.send(NetworkEvent::Disconnected {
                                        reason: DisconnectReason::TimedOut,
                                    }).await;
                                }
                            }
                        }
                    }
                }

                // Small sleep to avoid busy-waiting (1ms)
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
            }

            info!("[NetworkTask] Background task stopped");
        });

        Self {
            command_tx,
            event_rx,
            running,
        }
    }

    /// Perform connection handshake
    async fn do_connect(
        address: &str,
        username: &str,
    ) -> Result<(TcpStream, String, PlayerId), String> {
        let mut stream = TcpStream::connect(address).await
            .map_err(|e| format!("Failed to connect: {}", e))?;

        // Send handshake
        let handshake = Packet::new(PacketPayload::Handshake(HandshakePacket {
            protocol_version: PROTOCOL_VERSION,
            username: username.to_string(),
        }));

        send_packet(&mut stream, &handshake).await
            .map_err(|e| format!("Failed to send handshake: {}", e))?;

        // Receive response
        let response = receive_packet(&mut stream).await
            .map_err(|e| format!("Failed to receive handshake: {}", e))?;

        match response.payload {
            PacketPayload::HandshakeAccept(accept) => {
                Ok((stream, accept.server_name, accept.player_id))
            }
            PacketPayload::HandshakeReject(reject) => {
                Err(format!("Connection rejected: {}", reject.reason))
            }
            _ => {
                Err("Unexpected packet during handshake".to_string())
            }
        }
    }

    /// Try to receive a packet without blocking
    async fn try_receive_packet(stream: &mut TcpStream) -> Result<Option<Packet>, String> {
        // Use very short timeout (1ms) for non-blocking behavior
        match timeout(Duration::from_millis(1), receive_packet(stream)).await {
            Ok(Ok(packet)) => Ok(Some(packet)),
            Ok(Err(e)) => Err(format!("Receive error: {}", e)),
            Err(_) => Ok(None), // Timeout - no packet available
        }
    }

    /// Convert packet to event
    fn packet_to_event(packet: Packet) -> Option<NetworkEvent> {
        match packet.payload {
            PacketPayload::PlayerUpdate(update) => {
                Some(NetworkEvent::PlayerUpdate(update))
            }
            PacketPayload::BlockChange(change) => {
                Some(NetworkEvent::BlockChange(change))
            }
            PacketPayload::ChunkData(data) => {
                Some(NetworkEvent::ChunkData(data))
            }
            PacketPayload::EntitySpawn(spawn) => {
                Some(NetworkEvent::EntitySpawn(spawn))
            }
            PacketPayload::EntityDespawn(despawn) => {
                Some(NetworkEvent::Packet(Packet::new(PacketPayload::EntityDespawn(despawn))))
            }
            PacketPayload::PlayerConnected(conn) => {
                Some(NetworkEvent::PlayerConnected(conn))
            }
            PacketPayload::PlayerDisconnected(disconn) => {
                Some(NetworkEvent::PlayerDisconnected(disconn))
            }
            PacketPayload::ChatBroadcast(chat) => {
                Some(NetworkEvent::Chat(chat))
            }
            PacketPayload::Kicked(kicked) => {
                Some(NetworkEvent::Kicked(kicked))
            }
            PacketPayload::CommandResponse(response) => {
                Some(NetworkEvent::CommandResponse(response))
            }
            PacketPayload::Disconnect(disconn) => {
                Some(NetworkEvent::Disconnected {
                    reason: disconn.reason,
                })
            }
            PacketPayload::Ping(_) => {
                // Respond to ping automatically
                // TODO: Send pong back
                None
            }
            _ => {
                // Generic packet
                Some(NetworkEvent::Packet(packet))
            }
        }
    }

    /// Connect to server
    pub async fn connect(&self, address: String, username: String) {
        let _ = self.command_tx.send(NetworkCommand::Connect { address, username }).await;
    }

    /// Disconnect from server
    pub async fn disconnect(&self) {
        let _ = self.command_tx.send(NetworkCommand::Disconnect).await;
    }

    /// Send player update (async, will block if channel is full)
    pub async fn send_player_update(&self, update: PlayerUpdatePacket) {
        let _ = self.command_tx.send(NetworkCommand::SendPlayerUpdate(update)).await;
    }

    /// Send player update (non-blocking, drops packet if channel is full)
    pub fn try_send_player_update(&self, update: PlayerUpdatePacket) {
        let _ = self.command_tx.try_send(NetworkCommand::SendPlayerUpdate(update));
    }

    /// Send block action (async, will block if channel is full)
    pub async fn send_block_action(&self, action: BlockActionPacket) {
        let _ = self.command_tx.send(NetworkCommand::SendBlockAction(action)).await;
    }

    /// Send block action (non-blocking, drops packet if channel is full)
    pub fn try_send_block_action(&self, action: BlockActionPacket) {
        let _ = self.command_tx.try_send(NetworkCommand::SendBlockAction(action));
    }

    /// Send command request (async, will block if channel is full)
    pub async fn send_command(&self, command: String) {
        let _ = self.command_tx.send(NetworkCommand::SendCommandRequest(CommandRequestPacket {
            command,
        })).await;
    }

    /// Send command request (non-blocking, drops packet if channel is full)
    pub fn try_send_command(&self, command: String) {
        let _ = self.command_tx.try_send(NetworkCommand::SendCommandRequest(CommandRequestPacket {
            command,
        }));
    }

    /// Try to receive an event (non-blocking)
    pub fn try_recv_event(&mut self) -> Option<NetworkEvent> {
        self.event_rx.try_recv().ok()
    }

    /// Drain all available events (non-blocking)
    pub fn drain_events(&mut self) -> Vec<NetworkEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Check if connected
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }
}

impl Drop for NetworkTask {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
    }
}
