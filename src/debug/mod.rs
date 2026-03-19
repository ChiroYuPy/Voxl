pub mod commands;

use egui::{Context, Window, Grid, ScrollArea, TextEdit};
use std::time::Instant;

#[derive(Clone)]
pub struct ChatMessage {
    pub text: String,
    pub is_command: bool,
    pub timestamp: Instant,
}

pub struct EguiState {
    pub ctx: Context,
    pub enabled: bool,
    fps_frames: u32,
    fps_time: Instant,
    fps: f32,
    frame_count: u64,
    pub pixels_per_point: f32,

    // Chat system
    pub chat_open: bool,
    chat_input: String,
    chat_messages: Vec<ChatMessage>,
    pub chat_focused: bool,
    chat_focus_requested: bool,
}

impl EguiState {
    pub fn new(window: &winit::window::Window) -> Self {
        let ctx = Context::default();

        // Set pixels_per_point from window scale factor
        let pixels_per_point = window.scale_factor() as f32;

        // Enable dark mode
        ctx.set_visuals(egui::Visuals::dark());

        Self {
            ctx,
            enabled: false,
            fps_frames: 0,
            fps_time: Instant::now(),
            fps: 0.0,
            frame_count: 0,
            pixels_per_point,

            chat_open: false,
            chat_input: String::new(),
            chat_messages: Vec::new(),
            chat_focused: false,
            chat_focus_requested: false,
        }
    }

    pub fn update_pixels_per_point(&mut self, pixels_per_point: f32) {
        self.pixels_per_point = pixels_per_point;
        self.ctx.set_pixels_per_point(pixels_per_point);
    }

    pub fn update_fps(&mut self) {
        self.fps_frames += 1;
        let now = Instant::now();
        let elapsed = now.duration_since(self.fps_time).as_secs_f32();

        if elapsed >= 0.5 {
            self.fps = self.fps_frames as f32 / elapsed;
            self.fps_frames = 0;
            self.fps_time = now;
        }
        self.frame_count += 1;
    }

    pub fn open_chat(&mut self) {
        self.chat_open = true;
        self.chat_focused = true;
        self.chat_input.clear();
        self.chat_focus_requested = true; // Demander le focus sur le champ de texte
    }

    pub fn close_chat(&mut self) {
        self.chat_open = false;
        self.chat_focused = false;
        self.chat_input.clear();
        self.chat_focus_requested = false;
    }

    pub fn toggle_chat(&mut self) {
        if self.chat_open {
            self.close_chat();
        } else {
            self.open_chat();
        }
    }

    pub fn add_chat_message(&mut self, text: String, is_command: bool) {
        self.chat_messages.push(ChatMessage {
            text,
            is_command,
            timestamp: Instant::now(),
        });
        // Garder seulement les 100 derniers messages
        if self.chat_messages.len() > 100 {
            self.chat_messages.remove(0);
        }
    }

    pub fn clear_chat(&mut self) {
        self.chat_messages.clear();
    }

    pub fn show_debug_ui(
        &mut self,
        camera_pos: (f32, f32, f32),
        camera_yaw: f32,
        camera_pitch: f32,
        chunk_coords: (i32, i32, i32),
        selected_block: (u32, String),
        block_count: usize,
    ) {
        if !self.enabled {
            return;
        }

        // Calculate cardinal direction from yaw
        let cardinal_direction = match camera_yaw as i32 % 360 {
            y if y < -135 => "West",
            y if y < -45 => "North",
            y if y < 45 => "East",
            y if y < 135 => "South",
            _ => "West",
        };

        // Create a floating window instead of using CentralPanel
        Window::new("Debug Info")
            .resizable(true)
            .collapsible(false)
            .default_width(250.0)
            .show(&self.ctx, |ui| {
                Grid::new("debug_grid")
                    .num_columns(2)
                    .spacing([40.0, 6.0])
                    .show(ui, |ui| {
                        // FPS
                        ui.label("FPS:");
                        ui.label(format!("{:.1}", self.fps));
                        ui.end_row();

                        ui.label("Frame:");
                        ui.label(format!("{}", self.frame_count));
                        ui.end_row();

                        ui.separator();
                        ui.end_row();

                        // Position
                        ui.label("Position:");
                        ui.label(format!(
                            "X: {:.1}\nY: {:.1}\nZ: {:.1}",
                            camera_pos.0, camera_pos.1, camera_pos.2
                        ));
                        ui.end_row();

                        // Chunk
                        ui.label("Chunk:");
                        ui.label(format!(
                            "{} {} {}",
                            chunk_coords.0, chunk_coords.1, chunk_coords.2
                        ));
                        ui.end_row();

                        ui.separator();
                        ui.end_row();

                        // Rotation
                        ui.label("Facing:");
                        ui.label(format!("{}\n({:.1}°)", cardinal_direction, camera_yaw));
                        ui.end_row();

                        ui.label("Pitch:");
                        ui.label(format!("{:.1}°", camera_pitch));
                        ui.end_row();

                        ui.separator();
                        ui.end_row();

                        // Selected block
                        ui.label("Selected:");
                        ui.label(format!("{} ({})", selected_block.1, selected_block.0));
                        ui.end_row();

                        ui.label("Blocks:");
                        ui.label(format!("{}", block_count - 1));
                        ui.end_row();
                    });
            });
    }

    /// Affiche l'interface du chat
    /// Retourne Some(command) si une commande a été exécutée, None sinon
    pub fn show_chat_ui(&mut self) -> Option<String> {
        if !self.chat_open {
            return None;
        }

        let mut submitted_command = None;
        let mut should_close = false;
        let mut input_text = String::new();
        let focus_requested = self.chat_focus_requested;
        if focus_requested {
            self.chat_focus_requested = false;
        }

        let chat_messages = self.chat_messages.clone();

        egui::Area::new(egui::Id::new("chat_area"))
            .anchor(egui::Align2::LEFT_BOTTOM, [10.0, -10.0])
            .show(&self.ctx, |ui| {
                ui.set_width(400.0);
                ui.set_max_height(250.0);

                // Zone de messages avec scroll
                ScrollArea::vertical()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;

                        for msg in &chat_messages {
                            if msg.is_command {
                                ui.colored_label(egui::Rgba::from_rgb(150.0, 150.0, 255.0), &msg.text);
                            } else {
                                ui.label(&msg.text);
                            }
                        }
                    });

                ui.separator();

                // Champ de saisie
                let mut temp_input = self.chat_input.clone();
                let response = ui.add_sized(
                    [400.0, 25.0],
                    TextEdit::singleline(&mut temp_input)
                        .id(egui::Id::new("chat_input"))
                        .hint_text("Taper une commande ou un message...")
                        .desired_width(f32::INFINITY)
                );

                // Demander le focus si nécessaire
                if focus_requested {
                    ui.memory_mut(|mem| mem.request_focus(egui::Id::new("chat_input")));
                }

                // Mettre à jour l'input
                self.chat_input = temp_input;
                self.chat_focused = response.has_focus();

                // Gérer la soumission avec Entrée
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    input_text = self.chat_input.clone();
                    should_close = true;
                } else if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    should_close = true;
                }
            });

        // Traiter les résultats après la closure
        if should_close {
            if !input_text.is_empty() {
                if input_text.starts_with('/') {
                    // C'est une commande
                    submitted_command = Some(input_text.clone());
                    self.add_chat_message(format!("> {}", input_text), true);
                } else {
                    // C'est un message de chat
                    self.add_chat_message(format!("<Player> {}", input_text), false);
                }
                self.chat_input.clear();
            }
            self.close_chat();
        }

        submitted_command
    }

    pub fn context(&self) -> &Context {
        &self.ctx
    }

    pub fn context_mut(&mut self) -> &mut Context {
        &mut self.ctx
    }

    pub fn is_chat_open(&self) -> bool {
        self.chat_open
    }

    /// Indique si le chat consomme les entrées clavier
    pub fn wants_input(&self) -> bool {
        self.chat_open
    }
}
