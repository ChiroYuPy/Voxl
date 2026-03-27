use voxl_common::chat::ChatMessage;
use voxl_common::network::*;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::mpsc::{self, Sender, Receiver};

pub struct ServerChatManager {
    tx: Sender<ChatMessage>,
}

impl ServerChatManager {
    pub fn new() -> (Self, Receiver<ChatMessage>) {
        let (tx, rx) = mpsc::channel::<ChatMessage>(1000);
        (Self { tx }, rx)
    }

    pub fn broadcast(&self, message: ChatMessage) {
        let _ = self.tx.try_send(message);
    }

    pub fn broadcast_text(&self, text: &str) {
        self.broadcast(ChatMessage::text(text));
    }
}

impl Clone for ServerChatManager {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
        }
    }
}
