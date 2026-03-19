//! Composants ECS pour les entités du jeu

use glam::Vec3;

/// Position d'une entité dans le monde
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

/// Vélocité d'une entité (mouvement par seconde)
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

/// Mode de jeu pour une entité contrôlée par le joueur
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    /// Mode créatif: peut toggle le fly, a des collisions
    Creative { fly_enabled: bool },
    /// Mode spectateur: toujours en vol, pas de collisions
    Spectator,
}

impl GameMode {
    /// Retourne true si l'entité est en mode vol (spectateur ou créatif avec fly activé)
    pub fn is_flying(&self) -> bool {
        match self {
            GameMode::Spectator => true,
            GameMode::Creative { fly_enabled } => *fly_enabled,
        }
    }

    /// Retourne true si les collisions doivent être désactivées (mode spectateur)
    pub fn has_collisions(&self) -> bool {
        !matches!(self, GameMode::Spectator)
    }

    /// Toggle le fly en mode créatif
    pub fn toggle_fly(&mut self) {
        if let GameMode::Creative { fly_enabled } = self {
            *fly_enabled = !*fly_enabled;
        }
    }

    /// Active le fly en mode créatif
    pub fn set_fly(&mut self, enabled: bool) {
        if let GameMode::Creative { fly_enabled } = self {
            *fly_enabled = enabled;
        }
    }

    /// Retourne le nom du mode pour l'affichage
    pub fn name(&self) -> &'static str {
        match self {
            GameMode::Spectator => "Spectateur",
            GameMode::Creative { fly_enabled: true } => "Créatif (Vol)",
            GameMode::Creative { fly_enabled: false } => "Créatif",
        }
    }
}

impl Default for GameMode {
    fn default() -> Self {
        GameMode::Creative { fly_enabled: true }
    }
}

/// Marqueur: cette entité est contrôlée par le joueur
#[derive(Debug, Clone, Copy)]
pub struct PlayerControlled {
    /// Mode de jeu
    pub game_mode: GameMode,
    /// Sensibilité de la souris pour la rotation
    pub look_sensitivity: f32,
    /// Limites du pitch (haut/bas) en radians
    pub pitch_limits: (f32, f32),
    /// Sprint activé
    pub is_sprinting: bool,
    /// Multiplicateur de vitesse quand sprint
    pub sprint_multiplier: f32,
}

impl PlayerControlled {
    pub fn new() -> Self {
        Self {
            game_mode: GameMode::default(),
            look_sensitivity: 0.002,
            pitch_limits: (-std::f32::consts::FRAC_PI_2 + 0.01, std::f32::consts::FRAC_PI_2 - 0.01),
            is_sprinting: false,
            sprint_multiplier: 2.5,
        }
    }

    /// Retourne true si l'entité est en mode vol
    pub fn is_flying(&self) -> bool {
        self.game_mode.is_flying()
    }

    /// Retourne true si les collisions sont activées
    pub fn has_collisions(&self) -> bool {
        self.game_mode.has_collisions()
    }

    /// Toggle le fly (seulement en mode créatif)
    pub fn toggle_fly(&mut self) {
        self.game_mode.toggle_fly();
    }

    /// Change le mode de jeu
    pub fn set_game_mode(&mut self, mode: GameMode) {
        self.game_mode = mode;
    }

    /// Retourne une référence au mode de jeu
    pub fn get_game_mode(&self) -> &GameMode {
        &self.game_mode
    }
}

impl Default for PlayerControlled {
    fn default() -> Self {
        Self::new()
    }
}

/// Direction du regard (yaw/pitch) pour les entités qui regardent quelque chose
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

    /// Retourne le vecteur forward (direction du regard)
    pub fn forward(&self) -> Vec3 {
        let x = self.yaw.cos() * self.pitch.cos();
        let y = self.pitch.sin();
        let z = self.yaw.sin() * self.pitch.cos();
        Vec3::new(x, y, z).normalize()
    }

    /// Retourne le vecteur right (direction droite)
    pub fn right(&self) -> Vec3 {
        let forward = self.forward();
        Vec3::new(0.0, 1.0, 0.0).cross(forward).normalize()
    }

    /// Applique un mouvement de souris (delta)
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

/// Marqueur: cette entité est affectée par la physique du monde
#[derive(Debug, Clone, Copy)]
pub struct PhysicsAffected {
    /// Vitesse de déplacement au sol
    pub move_speed: f32,
    /// Gravité appliquée (m/s²)
    pub gravity: f32,
    /// Vitesse terminale de chute
    pub terminal_velocity: f32,
    /// Facteur de frottement au sol (0-1)
    pub ground_friction: f32,
    /// Facteur de résistance de l'air (0-1)
    pub air_resistance: f32,
    /// Hauteur de saut
    pub jump_height: f32,
    /// L'entité est-elle au sol ?
    pub on_ground: bool,
}

impl PhysicsAffected {
    pub fn new() -> Self {
        Self {
            move_speed: 8.0,
            gravity: 25.0,      // ~9.81 * un peu plus pour gameplay
            terminal_velocity: 50.0,
            ground_friction: 0.1,
            air_resistance: 0.02,
            jump_height: 1.2,
            on_ground: false,
        }
    }

    /// Retourne la vitesse verticale initiale pour atteindre jump_height
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

/// AABB (Axis-Aligned Bounding Box) pour les collisions
#[derive(Debug, Clone, Copy)]
pub struct AABB {
    /// Demi-dimensions (rayon) depuis le centre
    /// Pour un joueur: (0.3, 0.9, 0.3) = largeur 0.6, hauteur 1.8
    pub half_size: Vec3,
}

impl AABB {
    /// Crée un AABB pour un joueur (taille standard Minecraft)
    pub fn player_size() -> Self {
        Self {
            half_size: Vec3::new(0.3, 0.9, 0.3),
        }
    }

    /// Retourne les limites de l'AABB pour une position donnée
    pub fn bounds(&self, position: &Position) -> (Vec3, Vec3) {
        let pos = position.as_vec3();
        let min = pos - self.half_size;
        let max = pos + self.half_size;
        (min, max)
    }

    /// Retourne la liste des voxels potentiellement en collision avec cette AABB
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

/// Composant optionnel: nom de l'entité (pour debug)
#[derive(Debug, Clone)]
pub struct Name(pub String);

impl Name {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}
