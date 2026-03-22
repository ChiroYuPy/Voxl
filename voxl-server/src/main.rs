//! Voxl Server - Headless server with networking
//!
//! Dedicated server managing:
//! - World generation and authoritative state
//! - Player connections via TCP
//! - Entity synchronization (ECS)
//! - Network communication with clients

pub mod server;
pub mod connection;
pub mod player;

use server::Server;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    info!("=== Voxl Server Starting ===");

    // Load server settings
    let settings = voxl_common::ServerSettings::default();

    // Create and run server
    let server = Server::new(settings)?;
    server.run().await?;

    Ok(())
}
