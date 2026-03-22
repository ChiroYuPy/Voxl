//! Client networking module
//!
//! Handles TCP connection to the voxl server.

pub mod client;
pub mod packet_handler;

pub use client::NetworkClient;
