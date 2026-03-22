use std::collections::{HashMap, HashSet};
use winit::keyboard::{Key, NamedKey};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CanonicalKey {
    Character(char),
    Named(NamedKey),
}

impl CanonicalKey {
    fn from_key(key: &Key) -> Option<Self> {
        match key {
            Key::Character(s) => {
                s.chars().next().map(|c| CanonicalKey::Character(c.to_ascii_lowercase()))
            }
            Key::Named(named) => Some(CanonicalKey::Named(*named)),
            Key::Dead(_) => None,
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameAction {
    MoveForward,
    MoveBackward,
    MoveLeft,
    MoveRight,
    MoveUp,
    MoveDown,

    LookUp,
    LookDown,
    LookLeft,
    LookRight,

    BreakBlock,
    PlaceBlock,
    PickBlock,
    NextBlockType,
    PreviousBlockType,

    ToggleMouseCapture,
    ReleaseMouse,

    IncreaseSpeed,
    DecreaseSpeed,

    ToggleDebugUI,
    OpenChat,

    ToggleFly,
    CycleGameMode,
    ToggleChunkBorders,

    OpenSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InputButton {
    Key(String),  // Stocké comme string pour la sérialisation
    Mouse(u16),
}

impl InputButton {
    pub fn char(c: char) -> Self {
        Self::Key(format!("Key{}", c.to_uppercase()))
    }

    pub fn named(key: NamedKey) -> Self {
        Self::Key(format!("Named{:?}", key))
    }

    pub fn mouse(button: u16) -> Self {
        Self::Mouse(button)
    }

    /// Convertit un InputButton en représentation string pour la config
    pub fn to_string_repr(&self) -> String {
        match self {
            InputButton::Key(s) => s.clone(),
            InputButton::Mouse(b) => format!("Mouse{}", b),
        }
    }

    /// Crée un InputButton depuis une représentation string
    pub fn from_string_repr(s: &str) -> Option<Self> {
        if s.starts_with("Mouse") {
            let button_str = s.trim_start_matches("Mouse");
            button_str.parse::<u16>().ok().map(InputButton::Mouse)
        } else {
            Some(InputButton::Key(s.to_string()))
        }
    }
}

#[derive(Debug, Clone)]
pub struct KeyBindings {
    bindings: HashMap<GameAction, Vec<InputButton>>,
}

impl Default for KeyBindings {
    fn default() -> Self {
        let mut bindings = HashMap::new();

        bindings.insert(GameAction::MoveForward, vec![
            InputButton::char('z'),
        ]);
        bindings.insert(GameAction::MoveBackward, vec![
            InputButton::char('s'),
        ]);
        bindings.insert(GameAction::MoveLeft, vec![
            InputButton::char('q'),
        ]);
        bindings.insert(GameAction::MoveRight, vec![
            InputButton::char('d'),
        ]);
        bindings.insert(GameAction::MoveUp, vec![
            InputButton::named(NamedKey::Space),
        ]);
        bindings.insert(GameAction::MoveDown, vec![
            InputButton::named(NamedKey::Shift),
        ]);

        bindings.insert(GameAction::ToggleMouseCapture, vec![
            InputButton::named(NamedKey::Enter),
        ]);
        bindings.insert(GameAction::ReleaseMouse, vec![
            InputButton::named(NamedKey::Escape),
        ]);

        bindings.insert(GameAction::BreakBlock, vec![
            InputButton::mouse(1),
        ]);
        bindings.insert(GameAction::PlaceBlock, vec![
            InputButton::mouse(3),
        ]);
        bindings.insert(GameAction::PickBlock, vec![
            InputButton::mouse(2), // Middle click
        ]);

        bindings.insert(GameAction::IncreaseSpeed, vec![
            InputButton::named(NamedKey::Control),
        ]);

        // Molette pour changer le bloc
        bindings.insert(GameAction::NextBlockType, vec![
            InputButton::mouse(4), // Molette haut
        ]);
        bindings.insert(GameAction::PreviousBlockType, vec![
            InputButton::mouse(5), // Molette bas
        ]);
        bindings.insert(GameAction::DecreaseSpeed, vec![
            InputButton::char('-'),
        ]);

        bindings.insert(GameAction::ToggleDebugUI, vec![
            InputButton::named(NamedKey::F3),
        ]);

        bindings.insert(GameAction::OpenChat, vec![
            InputButton::char('t'),
        ]);

        // Toggle fly avec F
        bindings.insert(GameAction::ToggleFly, vec![
            InputButton::char('f'),
        ]);

        // Cycle gamemode avec G
        bindings.insert(GameAction::CycleGameMode, vec![
            InputButton::char('g'),
        ]);

        // Toggle chunk borders avec F6
        bindings.insert(GameAction::ToggleChunkBorders, vec![
            InputButton::named(NamedKey::F6),
        ]);

        // Open settings avec F4
        bindings.insert(GameAction::OpenSettings, vec![
            InputButton::named(NamedKey::F4),
        ]);

        Self { bindings }
    }
}

impl KeyBindings {
    pub fn get_bindings(&self, action: GameAction) -> &[InputButton] {
        self.bindings.get(&action).map(|v| v.as_slice()).unwrap_or(&[])
    }

    pub fn is_key_bound(&self, key: &Key, action: GameAction) -> bool {
        self.bindings.get(&action)
            .map(|buttons| {
                buttons.iter().any(|b| {
                    match b {
                        InputButton::Key(bound_key_str) => {
                            // Convertir le bound_key_str en Key pour comparaison
                            if let Some(bound_key) = Self::string_to_key(bound_key_str) {
                                match (&bound_key, key) {
                                    (Key::Character(bound_str), Key::Character(event_str)) => {
                                        bound_str.chars().next().map(|bc| {
                                            event_str.chars().next().map(|ec| {
                                                bc.eq_ignore_ascii_case(&ec)
                                            }).unwrap_or(false)
                                        }).unwrap_or(false)
                                    }
                                    _ => bound_key == *key,
                                }
                            } else {
                                false
                            }
                        }
                        _ => false,
                    }
                })
            })
            .unwrap_or(false)
    }

    /// Helper pour convertir une string en Key
    fn string_to_key(s: &str) -> Option<Key> {
        if s.starts_with("Key") {
            let c = s.trim_start_matches("Key").chars().next()?;
            Some(Key::Character(c.to_string().into()))
        } else if s.starts_with("Named") {
            let named_str = s.trim_start_matches("Named");
            match named_str {
                "Space" => Some(Key::Named(NamedKey::Space)),
                "Shift" => Some(Key::Named(NamedKey::Shift)),
                "Control" => Some(Key::Named(NamedKey::Control)),
                "Alt" => Some(Key::Named(NamedKey::Alt)),
                "Enter" => Some(Key::Named(NamedKey::Enter)),
                "Escape" => Some(Key::Named(NamedKey::Escape)),
                "F3" => Some(Key::Named(NamedKey::F3)),
                "F4" => Some(Key::Named(NamedKey::F4)),
                "F5" => Some(Key::Named(NamedKey::F5)),
                "F6" => Some(Key::Named(NamedKey::F6)),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn is_mouse_bound(&self, button: u16, action: GameAction) -> bool {
        self.bindings.get(&action)
            .map(|buttons| buttons.iter().any(|b| matches!(b, InputButton::Mouse(b) if *b == button)))
            .unwrap_or(false)
    }

    pub fn bind(&mut self, action: GameAction, button: InputButton) {
        self.bindings.entry(action).or_insert_with(Vec::new).push(button);
    }

    pub fn unbind(&mut self, action: GameAction, button: &InputButton) {
        if let Some(buttons) = self.bindings.get_mut(&action) {
            buttons.retain(|b| b != button);
        }
    }

    /// Convertit les bindings actuels en format de configuration
    pub fn to_config(&self) -> voxl_common::config::KeyBindingsConfig {
        let mut bindings = std::collections::HashMap::new();

        for (action, buttons) in &self.bindings {
            let action_name = format!("{:?}", action);
            let button_strings: Vec<String> = buttons.iter()
                .map(|b| b.to_string_repr())
                .collect();
            bindings.insert(action_name, button_strings);
        }

        voxl_common::config::KeyBindingsConfig { bindings }
    }

    /// Crée des bindings depuis une configuration
    pub fn from_config(config: &voxl_common::config::KeyBindingsConfig) -> Self {
        let mut bindings = HashMap::new();

        for (action_name, button_strings) in &config.bindings {
            // Parser le nom de l'action
            let action = match action_name.as_str() {
                "MoveForward" => GameAction::MoveForward,
                "MoveBackward" => GameAction::MoveBackward,
                "MoveLeft" => GameAction::MoveLeft,
                "MoveRight" => GameAction::MoveRight,
                "MoveUp" => GameAction::MoveUp,
                "MoveDown" => GameAction::MoveDown,
                "LookUp" => GameAction::LookUp,
                "LookDown" => GameAction::LookDown,
                "LookLeft" => GameAction::LookLeft,
                "LookRight" => GameAction::LookRight,
                "BreakBlock" => GameAction::BreakBlock,
                "PlaceBlock" => GameAction::PlaceBlock,
                "PickBlock" => GameAction::PickBlock,
                "NextBlockType" => GameAction::NextBlockType,
                "PreviousBlockType" => GameAction::PreviousBlockType,
                "ToggleMouseCapture" => GameAction::ToggleMouseCapture,
                "ReleaseMouse" => GameAction::ReleaseMouse,
                "IncreaseSpeed" => GameAction::IncreaseSpeed,
                "DecreaseSpeed" => GameAction::DecreaseSpeed,
                "ToggleDebugUI" => GameAction::ToggleDebugUI,
                "OpenChat" => GameAction::OpenChat,
                "ToggleFly" => GameAction::ToggleFly,
                "CycleGameMode" => GameAction::CycleGameMode,
                "ToggleChunkBorders" => GameAction::ToggleChunkBorders,
                "OpenSettings" => GameAction::OpenSettings,
                _ => continue,
            };

            // Parser les boutons
            let buttons: Vec<InputButton> = button_strings.iter()
                .filter_map(|s| InputButton::from_string_repr(s))
                .collect();

            if !buttons.is_empty() {
                bindings.insert(action, buttons);
            }
        }

        Self { bindings }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InputState {
    pressed_actions: HashSet<GameAction>,
    just_pressed_actions: HashSet<GameAction>,
    just_released_actions: HashSet<GameAction>,
    mouse_position: (f64, f64),
    last_mouse_position: Option<(f64, f64)>,
    mouse_delta: (f64, f64),
    mouse_captured: bool,
}

impl InputState {
    pub fn is_held(&self, action: GameAction) -> bool {
        self.pressed_actions.contains(&action)
    }

    pub fn just_pressed(&self, action: GameAction) -> bool {
        self.just_pressed_actions.contains(&action)
    }

    pub fn just_released(&self, action: GameAction) -> bool {
        self.just_released_actions.contains(&action)
    }

    pub fn mouse_position(&self) -> (f64, f64) {
        self.mouse_position
    }

    pub fn mouse_delta(&self) -> (f64, f64) {
        self.mouse_delta
    }

    pub fn is_mouse_captured(&self) -> bool {
        self.mouse_captured
    }

    pub fn set_mouse_captured(&mut self, captured: bool) {
        self.mouse_captured = captured;
        if captured {
            self.last_mouse_position = None;
        }
    }

    pub fn update(&mut self) {
        self.just_pressed_actions.clear();
        self.just_released_actions.clear();
        self.mouse_delta = (0.0, 0.0);
    }
}

#[derive(Debug)]
pub struct InputManager {
    bindings: KeyBindings,
    state: InputState,
    held_keys: HashSet<CanonicalKey>,
    held_mouse_buttons: HashSet<u16>,
}

impl Default for InputManager {
    fn default() -> Self {
        Self {
            bindings: KeyBindings::default(),
            state: InputState::default(),
            held_keys: HashSet::new(),
            held_mouse_buttons: HashSet::new(),
        }
    }
}

impl InputManager {
    pub fn with_bindings(bindings: KeyBindings) -> Self {
        Self {
            bindings,
            state: InputState::default(),
            held_keys: HashSet::new(),
            held_mouse_buttons: HashSet::new(),
        }
    }

    pub fn state(&self) -> &InputState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut InputState {
        &mut self.state
    }

    pub fn bindings(&self) -> &KeyBindings {
        &self.bindings
    }

    pub fn bindings_mut(&mut self) -> &mut KeyBindings {
        &mut self.bindings
    }

    pub fn on_key_press(&mut self, key: Key) {
        let canonical = if let Some(k) = CanonicalKey::from_key(&key) {
            k
        } else {
            return;
        };

        self.held_keys.insert(canonical.clone());

        for action in ALL_ACTIONS.iter().copied() {
            if self.bindings.is_key_bound(&key, action) {
                if !self.state.is_held(action) {
                    self.state.just_pressed_actions.insert(action);
                    self.state.pressed_actions.insert(action);
                }
            }
        }
    }

    pub fn on_key_release(&mut self, key: &Key) {
        let canonical = if let Some(k) = CanonicalKey::from_key(key) {
            k
        } else {
            return;
        };

        self.held_keys.remove(&canonical);

        for action in ALL_ACTIONS.iter().copied() {
            if self.bindings.is_key_bound(key, action) {
                let other_key_held = self.held_keys.iter()
                    .any(|k| {
                        let winit_key = match k {
                            CanonicalKey::Character(c) => Key::Character(c.to_string().into()),
                            CanonicalKey::Named(n) => Key::Named(*n),
                        };
                        self.bindings.is_key_bound(&winit_key, action)
                    });

                if !other_key_held {
                    self.state.just_released_actions.insert(action);
                    self.state.pressed_actions.remove(&action);
                }
            }
        }
    }

    pub fn on_mouse_press(&mut self, button: u16) {
        self.held_mouse_buttons.insert(button);

        for action in ALL_ACTIONS.iter().copied() {
            if self.bindings.is_mouse_bound(button, action) {
                self.state.just_pressed_actions.insert(action);
                self.state.pressed_actions.insert(action);
            }
        }
    }

    pub fn on_mouse_release(&mut self, button: u16) {
        self.held_mouse_buttons.remove(&button);

        for action in ALL_ACTIONS.iter().copied() {
            if self.bindings.is_mouse_bound(button, action) {
                self.state.just_released_actions.insert(action);
                self.state.pressed_actions.remove(&action);
            }
        }
    }

    pub fn on_mouse_move(&mut self, x: f64, y: f64) {
        self.state.mouse_position = (x, y);

        if self.state.mouse_captured {
            if let Some(last) = self.state.last_mouse_position {
                let dx = x - last.0;
                let dy = y - last.1;
                self.state.mouse_delta.0 += dx;
                self.state.mouse_delta.1 += dy;
            }
        }

        self.state.last_mouse_position = Some((x, y));
    }

    pub fn on_mouse_motion(&mut self, delta_x: f64, delta_y: f64) {
        if self.state.mouse_captured {
            self.state.mouse_delta.0 += delta_x;
            self.state.mouse_delta.1 += delta_y;
        }
    }

    pub fn set_mouse_captured(&mut self, captured: bool) {
        self.state.set_mouse_captured(captured);
    }

    pub fn update(&mut self) {
        self.state.update();
    }
}

const ALL_ACTIONS: [GameAction; 25] = [
    GameAction::MoveForward,
    GameAction::MoveBackward,
    GameAction::MoveLeft,
    GameAction::MoveRight,
    GameAction::MoveUp,
    GameAction::MoveDown,
    GameAction::LookUp,
    GameAction::LookDown,
    GameAction::LookLeft,
    GameAction::LookRight,
    GameAction::BreakBlock,
    GameAction::PlaceBlock,
    GameAction::PickBlock,
    GameAction::NextBlockType,
    GameAction::PreviousBlockType,
    GameAction::ToggleMouseCapture,
    GameAction::ReleaseMouse,
    GameAction::IncreaseSpeed,
    GameAction::DecreaseSpeed,
    GameAction::ToggleDebugUI,
    GameAction::OpenChat,
    GameAction::ToggleFly,
    GameAction::CycleGameMode,
    GameAction::ToggleChunkBorders,
    GameAction::OpenSettings,
];
