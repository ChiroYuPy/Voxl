//! Network module for client-server communication
//!
//! This module defines:
//! - Network messages/protocol
//! - Packet serialization
//! - Connection management

use serde::{Deserialize, Serialize};
use std::net::SocketAddr;

/// Maximum number of blocks that can be placed/broken in one packet
pub const MAX_BLOCK_CHANGES_PER_PACKET: usize = 64;

/// Unique identifier for a player connection
pub type PlayerId = u32;

/// Magic number for packet validation (helps detect corruption)
pub const PACKET_MAGIC: u32 = 0x564F584C; // "VOXL" in hex

/// Network protocol version
pub const PROTOCOL_VERSION: u32 = 1;

/// Packet header for validation and routing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketHeader {
    /// Magic number for validation
    pub magic: u32,
    /// Protocol version
    pub version: u32,
    /// Packet type identifier
    pub packet_type: u8,
}

impl PacketHeader {
    /// Creates a new packet header
    pub fn new(packet_type: PacketType) -> Self {
        Self {
            magic: PACKET_MAGIC,
            version: PROTOCOL_VERSION,
            packet_type: packet_type as u8,
        }
    }

    /// Validates the packet header
    pub fn is_valid(&self) -> bool {
        self.magic == PACKET_MAGIC && self.version == PROTOCOL_VERSION
    }
}

/// Network packet types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum PacketType {
    /// Client → Server: Initial connection request
    Handshake = 0,
    /// Server → Client: Connection accepted with player ID
    HandshakeAccept = 1,
    /// Server → Client: Connection rejected
    HandshakeReject = 2,

    /// Client → Server: Player movement/look update
    PlayerUpdate = 3,
    /// Server → Client: Broadcast of player movement
    PlayerPosition = 4,

    /// Client → Server: Block place/break action
    BlockAction = 5,
    /// Server → Client: Block change notification
    BlockChange = 6,

    /// Server → Client: Chunk data
    ChunkData = 7,
    /// Client → Server: Request chunks
    ChunkRequest = 8,

    /// Server → Client: Entity spawned
    EntitySpawn = 9,
    /// Server → Client: Entity despawned
    EntityDespawn = 10,

    /// Server → Client: Player connected
    PlayerConnected = 11,
    /// Server → Client: Player disconnected
    PlayerDisconnected = 12,

    /// Client → Server: Disconnect request
    Disconnect = 13,
    /// Server → Client: Kicked/Disconnected
    Kicked = 14,

    /// Client → Server: Chat message
    ChatMessage = 15,
    /// Server → Client: Broadcast chat message
    ChatBroadcast = 16,

    /// Server → Client: Keep-alive ping
    Ping = 17,
    /// Client → Server: Pong response
    Pong = 18,
}

/// Complete network packet with header and payload
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub header: PacketHeader,
    pub payload: PacketPayload,
}

/// Packet payload variants
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PacketPayload {
    Handshake(HandshakePacket),
    HandshakeAccept(HandshakeAcceptPacket),
    HandshakeReject(HandshakeRejectPacket),

    PlayerUpdate(PlayerUpdatePacket),
    PlayerPosition(PlayerPositionPacket),

    BlockAction(BlockActionPacket),
    BlockChange(BlockChangePacket),

    ChunkData(ChunkDataPacket),
    ChunkRequest(ChunkRequestPacket),

    EntitySpawn(EntitySpawnPacket),
    EntityDespawn(EntityDespawnPacket),

    PlayerConnected(PlayerConnectedPacket),
    PlayerDisconnected(PlayerDisconnectedPacket),

    Disconnect(DisconnectPacket),
    Kicked(KickedPacket),

    ChatMessage(ChatMessagePacket),
    ChatBroadcast(ChatBroadcastPacket),

    Ping(PingPacket),
    Pong(PongPacket),
}

// ============================================================================
// Packet Definitions
// ============================================================================

/// Handshake: Client → Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakePacket {
    pub protocol_version: u32,
    pub username: String,
}

/// Handshake Accept: Server → Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeAcceptPacket {
    pub player_id: PlayerId,
    pub server_name: String,
    pub motd: String,
}

/// Handshake Reject: Server → Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRejectPacket {
    pub reason: String,
}

/// Player Update: Client → Server (movement/look)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerUpdatePacket {
    /// Sequence number for packet ordering
    pub sequence: u32,
    /// Position in world coordinates
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Look direction (yaw, pitch in radians)
    pub yaw: f32,
    pub pitch: f32,
    /// Is player on ground?
    pub on_ground: bool,
}

/// Player Position: Server → Client (broadcast)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerPositionPacket {
    pub player_id: PlayerId,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
}

/// Block Action: Client → Server (place/break)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockActionPacket {
    pub sequence: u32,
    /// Block position
    pub x: i32,
    pub y: i32,
    pub z: i32,
    /// Action type
    pub action: BlockActionType,
}

/// Block Change: Server → Client (notification)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockChangePacket {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block_id: crate::GlobalVoxelId,
}

/// Block action types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockActionType {
    Place(u32),  // Block ID to place
    Break,
}

/// Chunk Data: Server → Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkDataPacket {
    pub cx: i32,
    pub cy: i32,
    pub cz: i32,
    /// Serialized chunk data (palette + voxel array)
    pub data: Vec<u8>,
}

/// Chunk Request: Client → Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRequestPacket {
    pub chunks: Vec<(i32, i32, i32)>,
}

/// Entity Spawn: Server → Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySpawnPacket {
    pub entity_id: u64,
    pub player_id: Option<PlayerId>,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub entity_type: EntityType,
}

/// Entity Despawn: Server → Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDespawnPacket {
    pub entity_id: u64,
}

/// Entity types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    Player,
    // Future: Mob, Item, etc.
}

/// Player Connected: Server → Client (broadcast)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerConnectedPacket {
    pub player_id: PlayerId,
    pub username: String,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Player Disconnected: Server → Client (broadcast)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDisconnectedPacket {
    pub player_id: PlayerId,
    pub reason: DisconnectReason,
}

/// Disconnect reasons
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisconnectReason {
    Left,
    TimedOut,
    Kicked,
    Error,
}

/// Disconnect: Client → Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisconnectPacket {
    pub reason: DisconnectReason,
}

/// Kicked: Server → Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KickedPacket {
    pub reason: String,
}

/// Chat Message: Client → Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessagePacket {
    pub message: String,
}

/// Chat Broadcast: Server → Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatBroadcastPacket {
    pub player_id: PlayerId,
    pub username: String,
    pub message: String,
}

/// Ping: Server → Client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PingPacket {
    pub timestamp: u64,
}

/// Pong: Client → Server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PongPacket {
    pub timestamp: u64,
}

// ============================================================================
// Helper functions
// ============================================================================

impl Packet {
    /// Creates a new packet with the given payload
    pub fn new(payload: PacketPayload) -> Self {
        let packet_type = match &payload {
            PacketPayload::Handshake(_) => PacketType::Handshake,
            PacketPayload::HandshakeAccept(_) => PacketType::HandshakeAccept,
            PacketPayload::HandshakeReject(_) => PacketType::HandshakeReject,
            PacketPayload::PlayerUpdate(_) => PacketType::PlayerUpdate,
            PacketPayload::PlayerPosition(_) => PacketType::PlayerPosition,
            PacketPayload::BlockAction(_) => PacketType::BlockAction,
            PacketPayload::BlockChange(_) => PacketType::BlockChange,
            PacketPayload::ChunkData(_) => PacketType::ChunkData,
            PacketPayload::ChunkRequest(_) => PacketType::ChunkRequest,
            PacketPayload::EntitySpawn(_) => PacketType::EntitySpawn,
            PacketPayload::EntityDespawn(_) => PacketType::EntityDespawn,
            PacketPayload::PlayerConnected(_) => PacketType::PlayerConnected,
            PacketPayload::PlayerDisconnected(_) => PacketType::PlayerDisconnected,
            PacketPayload::Disconnect(_) => PacketType::Disconnect,
            PacketPayload::Kicked(_) => PacketType::Kicked,
            PacketPayload::ChatMessage(_) => PacketType::ChatMessage,
            PacketPayload::ChatBroadcast(_) => PacketType::ChatBroadcast,
            PacketPayload::Ping(_) => PacketType::Ping,
            PacketPayload::Pong(_) => PacketType::Pong,
        };

        Self {
            header: PacketHeader::new(packet_type),
            payload,
        }
    }

    /// Serializes the packet to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, bincode::Error> {
        bincode::serialize(self)
    }

    /// Deserializes a packet from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, bincode::Error> {
        bincode::deserialize(data)
    }

    /// Validates the packet
    pub fn is_valid(&self) -> bool {
        self.header.is_valid()
    }
}

/// Player connection information
#[derive(Debug)]
pub struct PlayerConnection {
    pub player_id: PlayerId,
    pub username: String,
    pub address: SocketAddr,
    pub entity_id: Option<u64>,
}

impl PlayerConnection {
    pub fn new(player_id: PlayerId, username: String, address: SocketAddr) -> Self {
        Self {
            player_id,
            username,
            address,
            entity_id: None,
        }
    }
}
