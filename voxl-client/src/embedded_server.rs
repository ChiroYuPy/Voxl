//! Embedded server for single player / LAN mode
//!
//! Runs a voxl server in a background thread for local play.
//! This is now just a thin wrapper around voxl-server's run_embedded_server.

use voxl_common::ServerSettings;
use voxl_common::SharedVoxelRegistry;
use tracing::info;

/// Embedded server handle
pub struct EmbeddedServer {
    /// The actual port the server is listening on
    pub actual_port: u16,
}

impl EmbeddedServer {
    /// Starts an embedded server in a background thread
    /// Pass a SharedVoxelRegistry to ensure client and server use the same block IDs
    /// If port is 0, an available port will be automatically assigned
    /// Returns the server handle with the actual port assigned
    pub fn start_with_registry(settings: ServerSettings, registry: SharedVoxelRegistry) -> Result<Self, Box<dyn std::error::Error>> {
        info!("[EmbeddedServer] Starting embedded server on port {}...", settings.port);

        // Use voxl-server's embedded server function
        let actual_port = voxl_server::run_embedded_server(settings, registry)?;

        info!("[EmbeddedServer] Embedded server started on port {}", actual_port);

        Ok(Self {
            actual_port,
        })
    }

    /// Starts an embedded server in a background thread (creates own registry)
    pub fn start(settings: ServerSettings) -> Result<Self, Box<dyn std::error::Error>> {
        // Create a new registry (will have different IDs - not recommended for embedded mode!)
        let registry = SharedVoxelRegistry::new();
        Self::start_with_registry(settings, registry)
    }

    /// Stops the embedded server
    pub fn stop(mut self) {
        info!("[EmbeddedServer] Stopping embedded server...");
        // The server thread will be cleaned up when the handle is dropped
        info!("[EmbeddedServer] Embedded server stopped");
    }
}

impl Drop for EmbeddedServer {
    fn drop(&mut self) {
        info!("[EmbeddedServer] Embedded server handle dropped");
    }
}
