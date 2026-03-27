use serde::{Serialize, Deserialize};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatComponent {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

impl ChatComponent {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            color: None,
        }
    }

    pub fn color(mut self, color: impl Into<String>) -> Self {
        self.color = Some(color.into());
        self
    }
}

impl From<String> for ChatComponent {
    fn from(s: String) -> Self {
        Self::text(s)
    }
}

impl From<&str> for ChatComponent {
    fn from(s: &str) -> Self {
        Self::text(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub components: Vec<ChatComponent>,
}

impl ChatMessage {
    pub fn new() -> Self {
        Self {
            components: Vec::new(),
        }
    }

    pub fn text(text: impl Into<String>) -> Self {
        Self {
            components: vec![ChatComponent::text(text)],
        }
    }

    pub fn colored(text: impl Into<String>, color: impl Into<String>) -> Self {
        Self {
            components: vec![ChatComponent::text(text).color(color)],
        }
    }

    pub fn add(mut self, component: ChatComponent) -> Self {
        self.components.push(component);
        self
    }

    pub fn add_text(mut self, text: impl Into<String>) -> Self {
        self.components.push(ChatComponent::text(text));
        self
    }

    pub fn components(&self) -> &[ChatComponent] {
        &self.components
    }

    pub fn plain_text(&self) -> String {
        self.components.iter().map(|c| c.text.clone()).collect::<Vec<_>>().join("")
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }
}

impl Default for ChatMessage {
    fn default() -> Self {
        Self::new()
    }
}

impl From<String> for ChatMessage {
    fn from(s: String) -> Self {
        Self::text(s)
    }
}

impl From<&str> for ChatMessage {
    fn from(s: &str) -> Self {
        Self::text(s)
    }
}

impl From<ChatComponent> for ChatMessage {
    fn from(c: ChatComponent) -> Self {
        Self {
            components: vec![c],
        }
    }
}

impl From<Vec<ChatComponent>> for ChatMessage {
    fn from(components: Vec<ChatComponent>) -> Self {
        Self { components }
    }
}

impl fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.plain_text())
    }
}

pub fn text(text: &str) -> ChatMessage {
    ChatMessage::text(text)
}

pub fn error(text: &str) -> ChatMessage {
    ChatMessage::colored(text, "#FF5555")
}

pub fn success(text: &str) -> ChatMessage {
    ChatMessage::colored(text, "#55FF55")
}

pub fn info(text: &str) -> ChatMessage {
    ChatMessage::colored(text, "#FFFF55")
}
