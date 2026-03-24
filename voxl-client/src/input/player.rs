use glam::Vec3;
use crate::renderer::Camera;
use super::keybinds::{InputState, GameAction};

fn vec3a_to_vec3(v: glam::Vec3A) -> Vec3 {
    Vec3::new(v.x, v.y, v.z)
}

#[derive(Debug, Clone)]
pub struct MovementConfig {
    pub move_speed: f32,
    pub sprint_multiplier: f32,
    pub look_sensitivity: f32,
    pub pitch_limits: (f32, f32),
    pub fly_mode: bool,
    pub enable_sprint: bool,
}

impl Default for MovementConfig {
    fn default() -> Self {
        Self {
            move_speed: 8.0,
            sprint_multiplier: 2.5,
            look_sensitivity: 0.002,
            pitch_limits: (-std::f32::consts::FRAC_PI_2 + 0.01, std::f32::consts::FRAC_PI_2 - 0.01),
            fly_mode: true,
            enable_sprint: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayerController {
    config: MovementConfig,
    velocity: Vec3,
    is_sprinting: bool,
}

impl Default for PlayerController {
    fn default() -> Self {
        Self {
            config: MovementConfig::default(),
            velocity: Vec3::ZERO,
            is_sprinting: false,
        }
    }
}

impl PlayerController {
    pub fn with_config(config: MovementConfig) -> Self {
        Self {
            config,
            velocity: Vec3::ZERO,
            is_sprinting: false,
        }
    }

    pub fn config(&self) -> &MovementConfig {
        &self.config
    }

    pub fn config_mut(&mut self) -> &mut MovementConfig {
        &mut self.config
    }

    pub fn velocity(&self) -> Vec3 {
        self.velocity
    }

    pub fn is_sprinting(&self) -> bool {
        self.is_sprinting
    }

    pub fn update(&mut self, camera: &mut Camera, input: &InputState, delta_time: f32) {
        if input.is_mouse_captured() {
            let (dx, dy) = input.mouse_delta();
            camera.yaw -= dx as f32 * self.config.look_sensitivity;
            camera.pitch -= dy as f32 * self.config.look_sensitivity;
            camera.pitch = camera.pitch.clamp(self.config.pitch_limits.0, self.config.pitch_limits.1);
            camera.normalize_yaw();
        }

        let mut move_dir = Vec3::ZERO;

        let forward = vec3a_to_vec3(camera.forward());
        let right = vec3a_to_vec3(camera.right());

        if input.is_held(GameAction::MoveForward) {
            move_dir += forward;
        }
        if input.is_held(GameAction::MoveBackward) {
            move_dir -= forward;
        }

        if input.is_held(GameAction::MoveRight) {
            move_dir += right;
        }
        if input.is_held(GameAction::MoveLeft) {
            move_dir -= right;
        }

        if self.config.fly_mode {
            if input.is_held(GameAction::MoveUp) {
                move_dir.y += 1.0;
            }
            if input.is_held(GameAction::MoveDown) {
                move_dir.y -= 1.0;
            }
        }

        if move_dir.length_squared() > 0.0 {
            move_dir = move_dir.normalize();
        }

        self.is_sprinting = self.config.enable_sprint && input.is_held(GameAction::IncreaseSpeed);

        let speed = self.config.move_speed * if self.is_sprinting {
            self.config.sprint_multiplier
        } else {
            1.0
        };

        let target_velocity = move_dir * speed;
        self.velocity = self.velocity.lerp(target_velocity, 0.2);

        let new_pos = vec3a_to_vec3(camera.position) + self.velocity * delta_time;
        camera.position = glam::Vec3A::new(new_pos.x, new_pos.y, new_pos.z);
    }

    pub fn update_direct(&mut self, camera: &mut Camera, input: &InputState, delta_time: f32) {
        if input.is_mouse_captured() {
            let (dx, dy) = input.mouse_delta();
            camera.yaw -= dx as f32 * self.config.look_sensitivity;
            camera.pitch -= dy as f32 * self.config.look_sensitivity;
            camera.pitch = camera.pitch.clamp(self.config.pitch_limits.0, self.config.pitch_limits.1);
            camera.normalize_yaw();
        }

        let forward = vec3a_to_vec3(camera.forward());
        let right = vec3a_to_vec3(camera.right());

        let base_speed = self.config.move_speed;
        let speed = if self.config.enable_sprint && input.is_held(GameAction::IncreaseSpeed) {
            base_speed * self.config.sprint_multiplier
        } else {
            base_speed
        };

        let move_amount = speed * delta_time;

        let pos = vec3a_to_vec3(camera.position);
        let mut movement = Vec3::ZERO;

        if input.is_held(GameAction::MoveForward) {
            movement += forward * move_amount;
        }
        if input.is_held(GameAction::MoveBackward) {
            movement -= forward * move_amount;
        }
        if input.is_held(GameAction::MoveRight) {
            movement += right * move_amount;
        }
        if input.is_held(GameAction::MoveLeft) {
            movement -= right * move_amount;
        }

        if self.config.fly_mode {
            if input.is_held(GameAction::MoveUp) {
                movement.y += move_amount;
            }
            if input.is_held(GameAction::MoveDown) {
                movement.y -= move_amount;
            }
        }

        let new_pos = pos + movement;
        camera.position = glam::Vec3A::new(new_pos.x, new_pos.y, new_pos.z);

        self.velocity = Vec3::ZERO;
        if input.is_held(GameAction::MoveForward) {
            self.velocity += forward;
        }
        if input.is_held(GameAction::MoveBackward) {
            self.velocity -= forward;
        }
        if input.is_held(GameAction::MoveRight) {
            self.velocity += right;
        }
        if input.is_held(GameAction::MoveLeft) {
            self.velocity -= right;
        }
        if self.config.fly_mode {
            if input.is_held(GameAction::MoveUp) {
                self.velocity.y += 1.0;
            }
            if input.is_held(GameAction::MoveDown) {
                self.velocity.y -= 1.0;
            }
        }
        self.velocity = self.velocity.normalize_or_zero() * speed;
    }
}

