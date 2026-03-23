//! ECS components for game entities

use glam::Vec3;

/// Position of an entity in the world
#[derive(Debug, Clone, Copy)]
pub struct Position {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Position {
    pub fn new(pos: Vec3) -> Self {
        Self {
            x: pos.x,
            y: pos.y,
            z: pos.z,
        }
    }

    pub fn as_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    pub fn set(&mut self, pos: Vec3) {
        self.x = pos.x;
        self.y = pos.y;
        self.z = pos.z;
    }
}

impl From<Vec3> for Position {
    fn from(v: Vec3) -> Self {
        Self::new(v)
    }
}

/// Velocity of an entity (movement per second)
#[derive(Debug, Clone, Copy, Default)]
pub struct Velocity {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Velocity {
    pub fn new(v: Vec3) -> Self {
        Self {
            x: v.x,
            y: v.y,
            z: v.z,
        }
    }

    pub fn as_vec3(&self) -> Vec3 {
        Vec3::new(self.x, self.y, self.z)
    }

    pub fn set(&mut self, v: Vec3) {
        self.x = v.x;
        self.y = v.y;
        self.z = v.z;
    }

    pub fn add(&mut self, v: Vec3) {
        self.x += v.x;
        self.y += v.y;
        self.z += v.z;
    }

    pub fn is_zero(&self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.z == 0.0
    }
}

impl From<Vec3> for Velocity {
    fn from(v: Vec3) -> Self {
        Self::new(v)
    }
}

/// Game mode for player-controlled entities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    /// Creative mode: can toggle fly, has collisions
    Creative { fly_enabled: bool },
    /// Spectator mode: always flying, no collisions
    Spectator,
}

impl GameMode {
    /// Returns true if the entity is in fly mode (spectator or creative with fly enabled)
    pub fn is_flying(&self) -> bool {
        match self {
            GameMode::Spectator => true,
            GameMode::Creative { fly_enabled } => *fly_enabled,
        }
    }

    /// Returns true if collisions should be enabled (not spectator)
    pub fn has_collisions(&self) -> bool {
        !matches!(self, GameMode::Spectator)
    }

    /// Toggles fly in creative mode
    pub fn toggle_fly(&mut self) {
        if let GameMode::Creative { fly_enabled } = self {
            *fly_enabled = !*fly_enabled;
        }
    }

    /// Sets fly in creative mode
    pub fn set_fly(&mut self, enabled: bool) {
        if let GameMode::Creative { fly_enabled } = self {
            *fly_enabled = enabled;
        }
    }

    /// Returns the display name of the game mode
    pub fn name(&self) -> &'static str {
        match self {
            GameMode::Spectator => "Spectator",
            GameMode::Creative { fly_enabled: true } => "Creative (Flying)",
            GameMode::Creative { fly_enabled: false } => "Creative",
        }
    }
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::Creative { fly_enabled: false }  // Start without fly by default
    }
}

/// Marker: this entity is controlled by a player
#[derive(Debug, Clone, Copy)]
pub struct PlayerControlled {
    /// Game mode
    pub game_mode: GameMode,
    /// Mouse sensitivity for rotation
    pub look_sensitivity: f32,
    /// Pitch limits (up/down) in radians
    pub pitch_limits: (f32, f32),
    /// Sprint enabled
    pub is_sprinting: bool,
    /// Speed multiplier when sprinting
    pub sprint_multiplier: f32,
    /// Sneaking (crouching) - slower movement, prevents falling
    pub is_sneaking: bool,
    /// Movement speed multiplier when sneaking
    pub sneak_multiplier: f32,
    /// Current eye height (smoothly interpolated)
    pub current_eye_height: f32,
}

impl PlayerControlled {
    pub fn new() -> Self {
        Self {
            game_mode: GameMode::default(),
            look_sensitivity: 0.002,
            pitch_limits: (-std::f32::consts::FRAC_PI_2 + 0.01, std::f32::consts::FRAC_PI_2 - 0.01),
            is_sprinting: false,
            sprint_multiplier: 2.0,   // Sprint: 2x walking speed (ground)
            is_sneaking: false,
            sneak_multiplier: 0.3,    // Sneak: 30% of walking speed
            current_eye_height: 0.7,  // Start at standing eye height
        }
    }

    /// Returns true if the entity is in fly mode
    pub fn is_flying(&self) -> bool {
        self.game_mode.is_flying()
    }

    /// Returns true if collisions are enabled
    pub fn has_collisions(&self) -> bool {
        self.game_mode.has_collisions()
    }

    /// Toggles fly (only works in creative mode)
    pub fn toggle_fly(&mut self) {
        self.game_mode.toggle_fly();
    }

    /// Changes the game mode
    pub fn set_game_mode(&mut self, mode: GameMode) {
        self.game_mode = mode;
    }

    /// Returns a reference to the game mode
    pub fn get_game_mode(&self) -> &GameMode {
        &self.game_mode
    }
}

impl Default for PlayerControlled {
    fn default() -> Self {
        Self::new()
    }
}

/// Look direction (yaw/pitch) for entities that look at something
#[derive(Debug, Clone, Copy)]
pub struct LookDirection {
    pub yaw: f32,
    pub pitch: f32,
}

impl LookDirection {
    pub fn new() -> Self {
        Self {
            yaw: std::f32::consts::PI / 4.0,
            pitch: -0.3,
        }
    }

    /// Returns the forward vector (look direction)
    pub fn forward(&self) -> Vec3 {
        let x = self.yaw.cos() * self.pitch.cos();
        let y = self.pitch.sin();
        let z = self.yaw.sin() * self.pitch.cos();
        Vec3::new(x, y, z).normalize()
    }

    /// Returns the right vector
    pub fn right(&self) -> Vec3 {
        let forward = self.forward();
        Vec3::new(0.0, 1.0, 0.0).cross(forward).normalize()
    }

    /// Applies mouse movement (delta)
    pub fn apply_mouse_delta(&mut self, dx: f32, dy: f32, sensitivity: f32, pitch_limits: (f32, f32)) {
        self.yaw -= dx * sensitivity;
        self.pitch -= dy * sensitivity;
        self.pitch = self.pitch.clamp(pitch_limits.0, pitch_limits.1);
    }
}

impl Default for LookDirection {
    fn default() -> Self {
        Self::new()
    }
}

/// Marker: this entity is affected by physics
#[derive(Debug, Clone, Copy)]
pub struct PhysicsAffected {
    /// Target ground movement speed
    pub move_speed: f32,
    /// Applied gravity (m/s²)
    pub gravity: f32,
    /// Terminal fall velocity
    pub terminal_velocity: f32,
    /// Ground acceleration (how fast we reach move_speed)
    pub ground_acceleration: f32,
    /// Ground drag/friction (velocity decay per second)
    pub ground_drag: f32,
    /// Air acceleration
    pub air_acceleration: f32,
    /// Air drag
    pub air_drag: f32,
    /// Jump height
    pub jump_height: f32,
    /// Is the entity on the ground?
    pub on_ground: bool,
}

impl PhysicsAffected {
    pub fn new() -> Self {
        Self {
            move_speed: 6.0,        // Increased base walking speed
            gravity: 32.0,         // Minecraft gravity
            terminal_velocity: 78.4, // Minecraft terminal velocity
            ground_acceleration: 80.0,  // Fast acceleration on ground
            ground_drag: 15.0,     // Ground friction (Minecraft-style)
            air_acceleration: 15.0,    // Slower acceleration in air
            air_drag: 2.0,         // Less air drag
            jump_height: 1.252,    // Minecraft jump height
            on_ground: false,
        }
    }

    /// Returns the initial vertical velocity to reach jump_height
    pub fn jump_velocity(&self) -> f32 {
        (2.0 * self.gravity * self.jump_height).sqrt()
    }

    pub fn with_fly_mode(mut self) -> Self {
        self.gravity = 0.0;
        self
    }
}

impl Default for PhysicsAffected {
    fn default() -> Self {
        Self::new()
    }
}

/// AABB (Axis-Aligned Bounding Box) for collisions
#[derive(Debug, Clone, Copy)]
pub struct AABB {
    /// Half-dimensions (radius) from center
    /// For a player: (0.3, 0.9, 0.3) = width 0.6, height 1.8
    pub half_size: Vec3,
}

impl AABB {
    /// Creates an AABB for a player (standard Minecraft size)
    pub fn player_size() -> Self {
        Self {
            half_size: Vec3::new(0.3, 0.9, 0.3),
        }
    }

    /// Returns the AABB bounds for a given position
    pub fn bounds(&self, position: &Position) -> (Vec3, Vec3) {
        let pos = position.as_vec3();
        let min = pos - self.half_size;
        let max = pos + self.half_size;
        (min, max)
    }

    /// Returns the list of voxels potentially colliding with this AABB
    pub fn get_potential_collisions(&self, position: &Position) -> Vec<glam::IVec3> {
        let (min, max) = self.bounds(position);

        let min_voxel = glam::IVec3::new(
            min.x.floor() as i32,
            min.y.floor() as i32,
            min.z.floor() as i32,
        );
        let max_voxel = glam::IVec3::new(
            max.x.floor() as i32,
            max.y.floor() as i32,
            max.z.floor() as i32,
        );

        let mut voxels = Vec::new();
        for x in min_voxel.x..=max_voxel.x {
            for y in min_voxel.y..=max_voxel.y {
                for z in min_voxel.z..=max_voxel.z {
                    voxels.push(glam::IVec3::new(x, y, z));
                }
            }
        }
        voxels
    }
}

/// Optional component: entity name (for debug)
#[derive(Debug, Clone)]
pub struct Name(pub String);

impl Name {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}
