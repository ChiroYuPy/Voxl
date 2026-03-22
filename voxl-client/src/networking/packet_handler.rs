//! Packet handler for processing server packets
//!
//! Handles incoming packets from the server and updates the client state.

use voxl_common::{
    network::*,
    VoxelWorld, SharedVoxelRegistry, EntityWorld,
};
use tracing::{info, warn, error};

/// Handles an incoming packet from the server
pub fn handle_packet(
    packet: Packet,
    world: &VoxelWorld,
    entities: &EntityWorld,
    registry: &SharedVoxelRegistry,
) {
    match packet.payload {
        PacketPayload::Ping(ping) => {
            // Server sent a ping, should reply with pong
            // This is handled by the network client
        }

        PacketPayload::PlayerPosition(pos) => {
            // Another player moved
            info!("[Network] Player {} moved to ({:.1}, {:.1}, {:.1})",
                pos.player_id, pos.x, pos.y, pos.z);
            // TODO: Update entity position for this player
        }

        PacketPayload::BlockChange(change) => {
            // Block was placed/broken
            info!("[Network] Block changed at ({},{},{}) to ID {}",
                change.x, change.y, change.z, change.block_id);
            // TODO: Update local world
        }

        PacketPayload::ChunkData(chunk) => {
            // Received chunk data from server
            info!("[Network] Received chunk data for ({},{},{})",
                chunk.cx, chunk.cy, chunk.cz);
            // TODO: Deserialize and insert chunk into world
        }

        PacketPayload::EntitySpawn(spawn) => {
            // Entity spawned
            info!("[Network] Entity {} spawned at ({:.1}, {:.1}, {:.1})",
                spawn.entity_id, spawn.x, spawn.y, spawn.z);
            // TODO: Spawn entity in local ECS
        }

        PacketPayload::EntityDespawn(despawn) => {
            // Entity despawned
            info!("[Network] Entity {} despawned", despawn.entity_id);
            // TODO: Despawn entity from local ECS
        }

        PacketPayload::PlayerConnected(conn) => {
            // Player connected
            info!("[Network] Player '{}' (ID: {}) connected at ({:.1}, {:.1}, {:.1})",
                conn.username, conn.player_id, conn.x, conn.y, conn.z);
            // TODO: Spawn player entity
        }

        PacketPayload::PlayerDisconnected(disconn) => {
            // Player disconnected
            info!("[Network] Player {} disconnected: {:?}",
                disconn.player_id, disconn.reason);
            // TODO: Despawn player entity
        }

        PacketPayload::Kicked(kicked) => {
            // We were kicked
            error!("[Network] Kicked from server: {}", kicked.reason);
            // TODO: Handle kick - show message to user, return to menu
        }

        PacketPayload::ChatBroadcast(chat) => {
            // Chat message broadcast
            info!("[Chat] {}: {}", chat.username, chat.message);
            // TODO: Add to chat log
        }

        _ => {
            warn!("[Network] Received unhandled packet type: {:?}", packet.header.packet_type);
        }
    }
}
