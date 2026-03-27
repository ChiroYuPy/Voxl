use voxl_common::chat::{ChatMessage, ChatComponent};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const MAX_MESSAGES: usize = 100;

pub struct ChatManager {
    messages: Vec<ChatMessage>,
    dirty: Arc<AtomicBool>,
}

impl ChatManager {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            dirty: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn add_message(&mut self, message: ChatMessage) {
        self.messages.push(message);
        if self.messages.len() > MAX_MESSAGES {
            self.messages.remove(0);
        }
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn add_text(&mut self, text: impl Into<String>) {
        self.add_message(ChatMessage::text(text));
    }

    pub fn add_error(&mut self, text: impl Into<String>) {
        self.add_message(ChatMessage::colored(text, "#FF5555"));
    }

    pub fn add_success(&mut self, text: impl Into<String>) {
        self.add_message(ChatMessage::colored(text, "#55FF55"));
    }

    pub fn add_info(&mut self, text: impl Into<String>) {
        self.add_message(ChatMessage::colored(text, "#FFFF55"));
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.dirty.store(true, Ordering::Relaxed);
    }

    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    pub fn mark_clean(&self) {
        self.dirty.store(false, Ordering::Relaxed);
    }

    pub fn dirty_flag(&self) -> Arc<AtomicBool> {
        self.dirty.clone()
    }
}

impl Default for ChatManager {
    fn default() -> Self {
        Self::new()
    }
}
