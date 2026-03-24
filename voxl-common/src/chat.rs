//! Chat message system with structured JSON format
//!
//! This module provides a Minecraft-like chat system where messages are
//! composed of multiple components with different styles (colors, formatting).
//! Messages are serialized as JSON for network transmission and parsed
//! client-side for display, allowing the UI layer to be changed without
//! breaking compatibility.
//!
//! # Example
//!
//! ```json
//! [
//!   {"text": "Hello ", "color": "white"},
//!   {"text": "world", "color": "red", "bold": true},
//!   {"text": "!", "color": "yellow"}
//! ]
//! ```

use serde::{Serialize, Deserialize};

/// Text color codes (similar to Minecraft)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatColor {
    /// Black text     - §0
    Black,
    /// Dark blue      - §1
    DarkBlue,
    /// Dark green     - §2
    DarkGreen,
    /// Dark aqua      - §3
    DarkAqua,
    /// Dark red       - §4
    DarkRed,
    /// Dark purple    - §5
    DarkPurple,
    /// Gold           - §6
    Gold,
    /// Gray           - §7
    Gray,
    /// Dark gray      - §8
    DarkGray,
    /// Blue           - §9
    Blue,
    /// Green          - §a
    Green,
    /// Aqua           - §b
    Aqua,
    /// Red            - §c
    Red,
    /// Light purple   - §d
    LightPurple,
    /// Yellow         - §e
    Yellow,
    /// White          - §f
    White,
    /// Reset to default
    Reset,
}

impl ChatColor {
    /// Get the legacy section code (§) for this color
    pub fn legacy_code(self) -> char {
        match self {
            ChatColor::Black => '0',
            ChatColor::DarkBlue => '1',
            ChatColor::DarkGreen => '2',
            ChatColor::DarkAqua => '3',
            ChatColor::DarkRed => '4',
            ChatColor::DarkPurple => '5',
            ChatColor::Gold => '6',
            ChatColor::Gray => '7',
            ChatColor::DarkGray => '8',
            ChatColor::Blue => '9',
            ChatColor::Green => 'a',
            ChatColor::Aqua => 'b',
            ChatColor::Red => 'c',
            ChatColor::LightPurple => 'd',
            ChatColor::Yellow => 'e',
            ChatColor::White => 'f',
            ChatColor::Reset => 'r',
        }
    }

    /// Parse from legacy code
    pub fn from_legacy_code(c: char) -> Option<Self> {
        match c {
            '0' => Some(ChatColor::Black),
            '1' => Some(ChatColor::DarkBlue),
            '2' => Some(ChatColor::DarkGreen),
            '3' => Some(ChatColor::DarkAqua),
            '4' => Some(ChatColor::DarkRed),
            '5' => Some(ChatColor::DarkPurple),
            '6' => Some(ChatColor::Gold),
            '7' => Some(ChatColor::Gray),
            '8' => Some(ChatColor::DarkGray),
            '9' => Some(ChatColor::Blue),
            'a' | 'A' => Some(ChatColor::Green),
            'b' | 'B' => Some(ChatColor::Aqua),
            'c' | 'C' => Some(ChatColor::Red),
            'd' | 'D' => Some(ChatColor::LightPurple),
            'e' | 'E' => Some(ChatColor::Yellow),
            'f' | 'F' => Some(ChatColor::White),
            'r' | 'R' => Some(ChatColor::Reset),
            _ => None,
        }
    }
}

impl Default for ChatColor {
    fn default() -> Self {
        ChatColor::White
    }
}

/// Text formatting styles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChatFormat {
    /// Bold text
    #[serde(default)]
    pub bold: bool,
    /// Italic text
    #[serde(default)]
    pub italic: bool,
    /// Underlined text
    #[serde(default)]
    pub underlined: bool,
    /// Strikethrough text
    #[serde(default)]
    pub strikethrough: bool,
    /// Obfuscated text (random characters)
    #[serde(default)]
    pub obfuscated: bool,
}

impl ChatFormat {
    /// Empty formatting
    pub const NONE: Self = ChatFormat {
        bold: false,
        italic: false,
        underlined: false,
        strikethrough: false,
        obfuscated: false,
    };

    /// Create new format
    pub const fn new() -> Self {
        Self::NONE
    }

    /// Set bold
    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// Set italic
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// Set underlined
    pub const fn underlined(mut self) -> Self {
        self.underlined = true;
        self
    }

    /// Set strikethrough
    pub const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }

    /// Set obfuscated
    pub const fn obfuscated(mut self) -> Self {
        self.obfuscated = true;
        self
    }

    /// Check if any format is active
    pub fn has_formatting(self) -> bool {
        self.bold || self.italic || self.underlined || self.strikethrough || self.obfuscated
    }

    /// Get legacy format codes
    pub fn legacy_codes(self) -> String {
        let mut codes = String::new();
        if self.bold { codes.push('l'); }
        if self.italic { codes.push('o'); }
        if self.underlined { codes.push('n'); }
        if self.strikethrough { codes.push('m'); }
        if self.obfuscated { codes.push('k'); }
        codes
    }
}

/// A single component of a chat message
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatComponent {
    /// The text content (for text components)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,

    /// Translation key (for translatable components)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub translate: Option<String>,

    /// Translation arguments (used with translate)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub with: Vec<ChatComponent>,

    /// Text color
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ChatColor>,

    /// Text formatting
    #[serde(default)]
    pub format: ChatFormat,

    /// Click event (when clicked)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_event: Option<ClickEvent>,

    /// Hover event (when hovered)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_event: Option<HoverEvent>,
}

impl Default for ChatComponent {
    fn default() -> Self {
        Self::empty()
    }
}

impl ChatComponent {
    /// Create an empty component
    pub fn empty() -> Self {
        ChatComponent {
            text: None,
            translate: None,
            with: Vec::new(),
            color: None,
            format: ChatFormat::NONE,
            click_event: None,
            hover_event: None,
        }
    }

    /// Create a simple text component
    pub fn text(text: impl Into<String>) -> Self {
        ChatComponent {
            text: Some(text.into()),
            ..Self::empty()
        }
    }

    /// Create a translatable component
    pub fn translate(key: impl Into<String>) -> Self {
        ChatComponent {
            translate: Some(key.into()),
            ..Self::empty()
        }
    }

    /// Set color
    pub fn color(mut self, color: ChatColor) -> Self {
        self.color = Some(color);
        self
    }

    /// Set format
    pub fn format(mut self, format: ChatFormat) -> Self {
        self.format = format;
        self
    }

    /// Set bold
    pub fn bold(mut self) -> Self {
        self.format.bold = true;
        self
    }

    /// Set italic
    pub fn italic(mut self) -> Self {
        self.format.italic = true;
        self
    }

    /// Set underlined
    pub fn underlined(mut self) -> Self {
        self.format.underlined = true;
        self
    }

    /// Add translation arguments
    pub fn with(mut self, args: Vec<ChatComponent>) -> Self {
        self.with = args;
        self
    }

    /// Set click event
    pub fn click(mut self, action: ClickAction, value: String) -> Self {
        self.click_event = Some(ClickEvent { action, value });
        self
    }

    /// Set hover event
    pub fn hover(mut self, action: HoverAction, value: ChatComponent) -> Self {
        self.hover_event = Some(HoverEvent { action, value: Box::new(value) });
        self
    }

    /// Get the display text (without formatting codes)
    pub fn get_text(&self) -> String {
        if let Some(text) = &self.text {
            text.clone()
        } else if let Some(key) = &self.translate {
            // For translation keys, return the key itself for now
            // The client should handle actual translation
            key.clone()
        } else {
            String::new()
        }
    }
}

impl From<String> for ChatComponent {
    fn from(s: String) -> Self {
        ChatComponent::text(s)
    }
}

impl From<&str> for ChatComponent {
    fn from(s: &str) -> Self {
        ChatComponent::text(s)
    }
}

/// Click event action
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClickAction {
    /// Open a URL
    OpenUrl,
    /// Run a command
    RunCommand,
    /// Suggest a command in chat
    SuggestCommand,
    /// Copy to clipboard
    CopyToClipboard,
}

/// Click event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClickEvent {
    pub action: ClickAction,
    pub value: String,
}

/// Hover event action
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoverAction {
    /// Show text
    ShowText,
    /// Show an item
    ShowItem,
    /// Show an entity
    ShowEntity,
}

/// Hover event
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoverEvent {
    pub action: HoverAction,
    pub value: Box<ChatComponent>,
}

/// A complete chat message (can be a single component or multiple)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ChatMessage {
    /// Single component
    Single(ChatComponent),
    /// Multiple components
    Multiple(Vec<ChatComponent>),
}

impl PartialEq for ChatMessage {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ChatMessage::Single(a), ChatMessage::Single(b)) => a == b,
            (ChatMessage::Multiple(a), ChatMessage::Multiple(b)) => a == b,
            (ChatMessage::Single(a), ChatMessage::Multiple(b)) => b.as_slice() == std::slice::from_ref(a),
            (ChatMessage::Multiple(a), ChatMessage::Single(b)) => a.as_slice() == std::slice::from_ref(b),
        }
    }
}

impl std::fmt::Display for ChatMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display as plain text (without formatting codes)
        write!(f, "{}", self.plain_text())
    }
}

impl ChatMessage {
    /// Create a message from a single component
    pub fn single(component: ChatComponent) -> Self {
        ChatMessage::Single(component)
    }

    /// Create a message from multiple components
    pub fn multiple(components: Vec<ChatComponent>) -> Self {
        ChatMessage::Multiple(components)
    }

    /// Create a simple text message
    pub fn text(text: impl Into<String>) -> Self {
        ChatMessage::Single(ChatComponent::text(text))
    }

    /// Get all components in the message
    pub fn components(&self) -> &[ChatComponent] {
        match self {
            ChatMessage::Single(c) => std::slice::from_ref(c),
            ChatMessage::Multiple(v) => v,
        }
    }

    /// Parse from legacy format with § codes
    ///
    /// Example: "§cError: §fSomething went wrong"
    pub fn from_legacy(text: &str) -> Self {
        let mut components = Vec::new();
        let mut current_text = String::new();
        let mut current_color = ChatColor::White;
        let mut current_format = ChatFormat::NONE;

        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if i + 1 < chars.len() && chars[i] == '§' {
                // Save current component if there's text
                if !current_text.is_empty() {
                    components.push(ChatComponent {
                        text: Some(current_text.clone()),
                        color: Some(current_color),
                        format: current_format,
                        ..ChatComponent::empty()
                    });
                    current_text.clear();
                }

                // Parse format code
                let code_char = chars[i + 1];
                i += 2;

                if let Some(color) = ChatColor::from_legacy_code(code_char) {
                    if color == ChatColor::Reset {
                        current_color = ChatColor::White;
                        current_format = ChatFormat::NONE;
                    } else {
                        current_color = color;
                    }
                } else {
                    // Format codes
                    match code_char {
                        'l' | 'L' => current_format.bold = true,
                        'o' | 'O' => current_format.italic = true,
                        'n' | 'N' => current_format.underlined = true,
                        'm' | 'M' => current_format.strikethrough = true,
                        'k' | 'K' => current_format.obfuscated = true,
                        'r' | 'R' => {
                            current_color = ChatColor::White;
                            current_format = ChatFormat::NONE;
                        }
                        _ => {}
                    }
                }
            } else {
                current_text.push(chars[i]);
                i += 1;
            }
        }

        // Don't forget the last component
        if !current_text.is_empty() {
            components.push(ChatComponent {
                text: Some(current_text),
                color: Some(current_color),
                format: current_format,
                ..ChatComponent::empty()
            });
        }

        // Build the result based on number of components
        match components.len() {
            0 => ChatMessage::Single(ChatComponent::text("")),
            1 => {
                let c = components.into_iter().next().unwrap();
                ChatMessage::Single(c)
            }
            _ => ChatMessage::Multiple(components),
        }
    }

    /// Convert to legacy format with § codes
    pub fn to_legacy(&self) -> String {
        let mut result = String::new();
        let mut last_color = ChatColor::White;
        let mut last_format = ChatFormat::NONE;

        for component in self.components() {
            let color = component.color.unwrap_or(ChatColor::White);
            let format = component.format;

            // Add color code if changed
            if color != last_color || format != last_format {
                result.push('§');
                result.push(color.legacy_code());

                // Add format codes if needed
                let format_codes = format.legacy_codes();
                for c in format_codes.chars() {
                    result.push('§');
                    result.push(c);
                }

                last_color = color;
                last_format = format;
            }

            // Add text content
            if let Some(text) = &component.text {
                result.push_str(text);
            } else if let Some(key) = &component.translate {
                // For now, just use the key
                result.push_str(key);
            }
        }

        result
    }

    /// Get the plain text content (without formatting)
    pub fn plain_text(&self) -> String {
        self.components()
            .iter()
            .map(|c| c.get_text())
            .collect::<Vec<_>>()
            .join("")
    }
}

impl From<ChatComponent> for ChatMessage {
    fn from(component: ChatComponent) -> Self {
        ChatMessage::Single(component)
    }
}

impl From<String> for ChatMessage {
    fn from(s: String) -> Self {
        ChatMessage::text(s)
    }
}

impl From<&str> for ChatMessage {
    fn from(s: &str) -> Self {
        ChatMessage::text(s)
    }
}

/// Convert legacy codes with § prefix to ChatMessage
///
/// Convenience function for creating messages from legacy format
pub fn text(legacy: &str) -> ChatMessage {
    ChatMessage::from_legacy(legacy)
}

/// Create a simple text message
pub fn raw(text: &str) -> ChatMessage {
    ChatMessage::text(text)
}

/// Create an error message (red)
pub fn error(text: &str) -> ChatMessage {
    ChatMessage::Single(ChatComponent::text(text).color(ChatColor::Red))
}

/// Create a success message (green)
pub fn success(text: &str) -> ChatMessage {
    ChatMessage::Single(ChatComponent::text(text).color(ChatColor::Green))
}

/// Create an info message (yellow)
pub fn info(text: &str) -> ChatMessage {
    ChatMessage::Single(ChatComponent::text(text).color(ChatColor::Yellow))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_legacy_parse() {
        let msg = ChatMessage::from_legacy("§cError: §fSomething went wrong");
        let components = msg.components();
        assert_eq!(components.len(), 2);
        assert_eq!(components[0].text, Some("Error: ".to_string()));
        assert_eq!(components[0].color, Some(ChatColor::Red));
        assert_eq!(components[1].text, Some("Something went wrong".to_string()));
        assert_eq!(components[1].color, Some(ChatColor::White));
    }

    #[test]
    fn test_to_legacy() {
        let msg = ChatMessage::multiple(vec![
            ChatComponent::text("Error: ").color(ChatColor::Red),
            ChatComponent::text("Something went wrong").color(ChatColor::White),
        ]);
        let legacy = msg.to_legacy();
        assert!(legacy.contains("§c"));
        assert!(legacy.contains("§f"));
    }

    #[test]
    fn test_plain_text() {
        let msg = ChatMessage::from_legacy("§cError: §fSomething went wrong");
        assert_eq!(msg.plain_text(), "Error: Something went wrong");
    }

    #[test]
    fn test_simple_text() {
        let msg = ChatMessage::text("Hello, world!");
        assert_eq!(msg.plain_text(), "Hello, world!");
    }
}
