pub mod commands;

use egui::{Context, ScrollArea, TextEdit, Align2};
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::Write as IoWrite;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH, Instant};
use crate::performance::PerformanceSnapshot;
use voxl_common::voxel::VoxelFace;

/// 3x3 grid anchor positions for debug text
#[derive(Debug, Clone, Copy)]
pub enum TextAnchor {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl TextAnchor {
    /// Get the egui Align2 and offset for this anchor
    fn to_egui_anchor(&self) -> (Align2, [f32; 2]) {
        let align = match self {
            TextAnchor::TopLeft => Align2::LEFT_TOP,
            TextAnchor::TopCenter => Align2::CENTER_TOP,
            TextAnchor::TopRight => Align2::RIGHT_TOP,
            TextAnchor::CenterLeft => Align2::LEFT_CENTER,
            TextAnchor::Center => Align2::CENTER_CENTER,
            TextAnchor::CenterRight => Align2::RIGHT_CENTER,
            TextAnchor::BottomLeft => Align2::LEFT_BOTTOM,
            TextAnchor::BottomCenter => Align2::CENTER_BOTTOM,
            TextAnchor::BottomRight => Align2::RIGHT_BOTTOM,
        };

        let offset = match self {
            TextAnchor::TopLeft => [10.0, 10.0],
            TextAnchor::TopCenter => [0.0, 10.0],
            TextAnchor::TopRight => [-15.0, 10.0],  // More padding for right side
            TextAnchor::CenterLeft => [10.0, 0.0],
            TextAnchor::Center => [0.0, 0.0],
            TextAnchor::CenterRight => [-15.0, 0.0],
            TextAnchor::BottomLeft => [10.0, -10.0],
            TextAnchor::BottomCenter => [0.0, -10.0],
            TextAnchor::BottomRight => [-15.0, -10.0],
        };

        (align, offset)
    }

    /// Returns the horizontal direction from this anchor
    fn horizontal_direction(&self) -> HorizontalDirection {
        match self {
            TextAnchor::TopLeft | TextAnchor::CenterLeft | TextAnchor::BottomLeft => HorizontalDirection::Left,
            TextAnchor::TopCenter | TextAnchor::Center | TextAnchor::BottomCenter => HorizontalDirection::Center,
            TextAnchor::TopRight | TextAnchor::CenterRight | TextAnchor::BottomRight => HorizontalDirection::Right,
        }
    }
}

/// Direction for text alignment relative to anchor
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HorizontalDirection {
    Left,
    Center,
    Right,
}

/// Text alignment for a single line
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// A formatted text line with alignment
#[derive(Debug, Clone)]
struct FormattedLine {
    text: String,
    align: TextAlign,
}

impl FormattedLine {
    fn left(text: impl Into<String>) -> Self {
        Self { text: text.into(), align: TextAlign::Left }
    }

    fn center(text: impl Into<String>) -> Self {
        Self { text: text.into(), align: TextAlign::Center }
    }

    fn right(text: impl Into<String>) -> Self {
        Self { text: text.into(), align: TextAlign::Right }
    }
}

/// Builder for formatted debug text with per-line alignment
pub struct DebugText {
    lines: Vec<FormattedLine>,
    line_spacing: f32,
}

impl DebugText {
    /// Create a new empty DebugText
    pub fn new() -> Self {
        Self {
            lines: Vec::new(),
            line_spacing: 16.0, // Default line spacing in pixels
        }
    }

    /// Set custom line spacing
    pub fn with_line_spacing(mut self, spacing: f32) -> Self {
        self.line_spacing = spacing;
        self
    }

    /// Add a left-aligned line
    pub fn line_left(&mut self, text: impl Into<String>) -> &mut Self {
        self.lines.push(FormattedLine::left(text));
        self
    }

    /// Add a center-aligned line
    pub fn line_center(&mut self, text: impl Into<String>) -> &mut Self {
        self.lines.push(FormattedLine::center(text));
        self
    }

    /// Add a right-aligned line
    pub fn line_right(&mut self, text: impl Into<String>) -> &mut Self {
        self.lines.push(FormattedLine::right(text));
        self
    }

    /// Add a line with custom alignment
    pub fn line(&mut self, text: impl Into<String>, align: TextAlign) -> &mut Self {
        self.lines.push(FormattedLine { text: text.into(), align });
        self
    }

    /// Add an empty line (inherits no specific alignment)
    pub fn empty(&mut self) -> &mut Self {
        self.lines.push(FormattedLine::left(""));
        self
    }

    /// Clear all lines
    pub fn clear(&mut self) {
        self.lines.clear();
    }

    /// Check if text is empty
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Get the base anchor for this text (for internal use)
    fn get_anchor_and_offset(&self, anchor: TextAnchor) -> (Align2, [f32; 2], egui::Vec2) {
        let (egui_align, offset) = anchor.to_egui_anchor();
        // Starting position offset based on anchor
        let start_offset = match anchor.horizontal_direction() {
            HorizontalDirection::Left => egui::Vec2::new(0.0, 0.0),
            HorizontalDirection::Center => egui::Vec2::new(0.0, 0.0),
            HorizontalDirection::Right => egui::Vec2::new(0.0, 0.0),
        };
        (egui_align, offset, start_offset)
    }
}

impl Default for DebugText {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to draw formatted text at an anchor position
fn draw_formatted_text(ctx: &Context, id: &str, text: &DebugText, anchor: TextAnchor) {
    if text.is_empty() {
        return;
    }

    let (base_align, base_offset, _) = text.get_anchor_and_offset(anchor);

    egui::Area::new(egui::Id::new(id))
        .anchor(base_align, base_offset)
        .interactable(false)
        .show(ctx, |ui| {
            ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);
            ui.spacing_mut().item_spacing = egui::Vec2::new(0.0, text.line_spacing - 16.0);

            for line in &text.lines {
                match line.align {
                    TextAlign::Left => {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.label(&line.text);
                        });
                    }
                    TextAlign::Center => {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.label(&line.text);
                        });
                    }
                    TextAlign::Right => {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.label(&line.text);
                        });
                    }
                }
            }
        });
}

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

    // Settings menu
    pub settings_open: bool,

    // Stats logging (F7)
    dump_stats_requested: bool,
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

            settings_open: false,

            dump_stats_requested: false,
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

    /// Calculate which face the camera is facing based on direction vector
    fn camera_facing_face(forward: (f32, f32, f32)) -> VoxelFace {
        let (x, y, z) = forward;
        let ax = x.abs();
        let ay = y.abs();
        let az = z.abs();

        // Find dominant axis and direction
        if ax >= ay && ax >= az {
            if x > 0.0 { VoxelFace::East } else { VoxelFace::West }
        } else if ay >= ax && ay >= az {
            if y > 0.0 { VoxelFace::Top } else { VoxelFace::Bottom }
        } else {
            if z > 0.0 { VoxelFace::South } else { VoxelFace::North }
        }
    }

    /// Format position as compact "X Y Z Yaw Pitch"
    fn format_player_pos(x: f32, y: f32, z: f32, yaw: f32, pitch: f32) -> String {
        format!("XYZ: {:.1}, {:.1}, {:.1} | Yaw: {:.0}° Pitch: {:.0}°", x, y, z, yaw, pitch)
    }

    /// Format target block info with face
    fn format_target_info(target: Option<(i32, i32, i32, VoxelFace)>, block_name: &str) -> String {
        if let Some((x, y, z, face)) = target {
            let face_name = match face {
                VoxelFace::Top => "↑",
                VoxelFace::Bottom => "↓",
                VoxelFace::North => "N",
                VoxelFace::South => "S",
                VoxelFace::East => "E",
                VoxelFace::West => "W",
            };
            format!("Target: {} ({},{},{}) [{}]", block_name, x, y, z, face_name)
        } else {
            format!("Target: None")
        }
    }

    /// Format camera facing
    fn format_camera_facing(face: VoxelFace) -> &'static str {
        match face {
            VoxelFace::Top => "↑ Top",
            VoxelFace::Bottom => "↓ Bottom",
            VoxelFace::North => "N North",
            VoxelFace::South => "S South",
            VoxelFace::East => "E East",
            VoxelFace::West => "W West",
        }
    }

    pub fn show_debug_ui(
        &mut self,
        camera_pos: (f32, f32, f32),
        camera_forward: (f32, f32, f32),
        camera_yaw: f32,
        camera_pitch: f32,
        chunk_coords: (i32, i32, i32),
        target_block: Option<(i32, i32, i32, VoxelFace, String)>,
        selected_block: (u32, String),
        block_count: usize,
        perf_snapshot: &PerformanceSnapshot,
        visible_chunks: usize,
        total_chunks: usize,
    ) {
        if !self.enabled {
            return;
        }

        // Calculate camera facing face
        let camera_face = Self::camera_facing_face(camera_forward);

        // Build each section's text
        let text_tl = self.build_top_left_text(perf_snapshot);
        let text_tr = self.build_top_right_text(
            camera_pos, camera_yaw, camera_pitch, chunk_coords,
            target_block, selected_block, block_count, camera_face,
        );
        let text_bl = self.build_bottom_left_text(perf_snapshot, visible_chunks, total_chunks);
        let text_br = "[F7] Dump Stats";

        // Draw each text at its anchor position
        self.draw_text("debug_tl", &text_tl, TextAnchor::TopLeft);
        self.draw_text("debug_tr", &text_tr, TextAnchor::TopRight);
        self.draw_text("debug_bl", &text_bl, TextAnchor::BottomLeft);
        self.draw_text("debug_br", &text_br, TextAnchor::BottomRight);
    }

    /// Draw text at a specific anchor position
    fn draw_text(&self, id: &str, text: &str, anchor: TextAnchor) {
        let (align, offset) = anchor.to_egui_anchor();
        egui::Area::new(egui::Id::new(id))
            .anchor(align, offset)
            .interactable(false)
            .show(&self.ctx, |ui| {
                ui.style_mut().override_text_style = Some(egui::TextStyle::Monospace);

                // For right-side anchors, use right_to_left layout to align from right edge
                // For left-side anchors, just label normally
                // For center, use centered layout
                match anchor.horizontal_direction() {
                    HorizontalDirection::Right => {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.label(text);
                        });
                    }
                    HorizontalDirection::Center => {
                        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.label(text);
                        });
                    }
                    HorizontalDirection::Left => {
                        ui.label(text);
                    }
                }
            });
    }

    /// Draw custom text at a specific anchor (for plugins/extensions)
    pub fn draw_text_at(&self, id: &str, text: &str, anchor: TextAnchor) {
        if !self.enabled {
            return;
        }
        self.draw_text(id, text, anchor);
    }

    /// Draw formatted text with per-line alignment at a specific anchor
    pub fn draw_formatted_at(&self, id: &str, text: &DebugText, anchor: TextAnchor) {
        if !self.enabled || text.is_empty() {
            return;
        }
        draw_formatted_text(&self.ctx, id, text, anchor);
    }

    /// Build top-left text: FPS, Performance, ECS
    fn build_top_left_text(&self, perf: &PerformanceSnapshot) -> String {
        let mut text = String::with_capacity(512);

        writeln!(text, "FPS: {:.1}", self.fps).ok();
        writeln!(text, "Frame: {}", self.frame_count).ok();
        text.push_str("──────────────\n");

        text.push_str("PERFORMANCE (ms)\n");
        writeln!(text, "  Frame:  {:.2}", perf.frame_timing.as_ms()).ok();
        writeln!(text, "  CPU:    {:.2}", perf.cpu_timing.as_ms()).ok();
        writeln!(text, "  GPU:    {:.2}", perf.gpu_timing.as_ms()).ok();

        if perf.networking_timing.avg().as_nanos() > 0 {
            writeln!(text, "  Net:    {:.2}", perf.networking_timing.as_ms()).ok();
        }
        if perf.world_update_timing.avg().as_nanos() > 0 {
            writeln!(text, "  World:  {:.2}", perf.world_update_timing.as_ms()).ok();
        }

        // Top 3 ECS systems
        if !perf.system_timings.is_empty() {
            text.push_str("\nECS (ms)\n");
            let mut systems: Vec<_> = perf.system_timings.iter().collect();
            systems.sort_by(|a, b| b.1.as_ms().partial_cmp(&a.1.as_ms()).unwrap_or(std::cmp::Ordering::Equal));
            for (name, timing) in systems.iter().take(3) {
                writeln!(text, "  {}: {:.2}", name, timing.as_ms()).ok();
            }
        }

        text
    }

    /// Build top-right text: Compact player info, target, camera facing
    fn build_top_right_text(
        &self,
        camera_pos: (f32, f32, f32),
        camera_yaw: f32,
        camera_pitch: f32,
        chunk_coords: (i32, i32, i32),
        target_block: Option<(i32, i32, i32, VoxelFace, String)>,
        selected_block: (u32, String),
        block_count: usize,
        camera_face: VoxelFace,
    ) -> String {
        let mut text = String::with_capacity(512);

        // Player info (compact)
        writeln!(text, "{}", Self::format_player_pos(
            camera_pos.0, camera_pos.1, camera_pos.2,
            camera_yaw, camera_pitch
        )).ok();

        text.push_str("\n");

        // Target block info
        let target_str = if let Some((x, y, z, face, name)) = target_block {
            let face_sym = match face {
                VoxelFace::Top => "↑",
                VoxelFace::Bottom => "↓",
                VoxelFace::North => "N",
                VoxelFace::South => "S",
                VoxelFace::East => "E",
                VoxelFace::West => "W",
            };
            format!("Target: {} ({},{},{}) [{}]", name, x, y, z, face_sym)
        } else {
            "Target: None".to_string()
        };
        writeln!(text, "{}", target_str).ok();

        // Camera facing
        writeln!(text, "Facing: {}", Self::format_camera_facing(camera_face)).ok();

        text.push_str("\n");

        // Selected block (for placement)
        writeln!(text, "Held: {} ({})", selected_block.1, selected_block.0).ok();
        writeln!(text, "Total Blocks: {}", block_count - 1).ok();

        text
    }

    /// Build bottom-left text: Memory, Chunks
    fn build_bottom_left_text(
        &self,
        perf: &PerformanceSnapshot,
        visible_chunks: usize,
        total_chunks: usize,
    ) -> String {
        let mut text = String::with_capacity(256);

        writeln!(text, "CHUNKS {}/{}", visible_chunks, total_chunks).ok();
        writeln!(text, "Loaded: {}", perf.memory.loaded_chunks).ok();
        writeln!(text, "Meshes: {}", perf.memory.loaded_meshes).ok();

        text.push_str("\nMEMORY\n");
        writeln!(text, "  Voxel: {:.1} MB", perf.memory.voxel_memory_mb).ok();
        writeln!(text, "  Mesh:  {:.1} MB", perf.memory.mesh_memory_mb).ok();

        text
    }

    /// Request stats dump to file (triggered by F7)
    pub fn request_dump_stats(&mut self) {
        self.dump_stats_requested = true;
    }

    /// Check if stats dump was requested and dump if so
    /// Returns true if a dump was performed
    pub fn check_dump_stats(
        &mut self,
        camera_pos: (f32, f32, f32),
        camera_yaw: f32,
        camera_pitch: f32,
        chunk_coords: (i32, i32, i32),
        selected_block: (u32, String),
        block_count: usize,
        perf: &PerformanceSnapshot,
        visible_chunks: usize,
        total_chunks: usize,
    ) -> bool {
        if !self.dump_stats_requested {
            return false;
        }

        self.dump_stats_requested = false;
        self.do_dump_stats(
            camera_pos, camera_yaw, camera_pitch,
            chunk_coords, selected_block, block_count,
            perf, visible_chunks, total_chunks,
        );
        true
    }

    /// Actually dump stats to file
    fn do_dump_stats(
        &self,
        camera_pos: (f32, f32, f32),
        camera_yaw: f32,
        camera_pitch: f32,
        chunk_coords: (i32, i32, i32),
        selected_block: (u32, String),
        block_count: usize,
        perf: &PerformanceSnapshot,
        visible_chunks: usize,
        total_chunks: usize,
    ) {
        // Ensure data/logs directory exists
        let log_dir: PathBuf = ["data", "logs"].iter().collect();
        fs::create_dir_all(&log_dir).ok();

        // Create filename with timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let filename = log_dir.join(format!("stats_{}.txt", timestamp));

        // Build the full stats report
        let mut report = String::with_capacity(2048);
        writeln!(report, "=== VOXL STATS DUMP ===").ok();
        writeln!(report, "Timestamp: {}", timestamp).ok();
        writeln!(report, "FPS: {:.1}", self.fps).ok();
        writeln!(report, "Frame: {}", self.frame_count).ok();
        writeln!(report).ok();

        // Performance section
        writeln!(report, "=== PERFORMANCE ===").ok();
        writeln!(report, "Frame Time:  {:.2} ms", perf.frame_timing.as_ms()).ok();
        writeln!(report, "CPU Time:    {:.2} ms", perf.cpu_timing.as_ms()).ok();
        writeln!(report, "GPU Submit:  {:.2} ms", perf.gpu_timing.as_ms()).ok();
        if perf.networking_timing.avg().as_nanos() > 0 {
            writeln!(report, "Network:     {:.2} ms", perf.networking_timing.as_ms()).ok();
        }
        if perf.input_timing.avg().as_nanos() > 0 {
            writeln!(report, "Input:       {:.2} ms", perf.input_timing.as_ms()).ok();
        }
        if perf.world_update_timing.avg().as_nanos() > 0 {
            writeln!(report, "World Upd:   {:.2} ms", perf.world_update_timing.as_ms()).ok();
        }
        if perf.render_prep_timing.avg().as_nanos() > 0 {
            writeln!(report, "Render Prep: {:.2} ms", perf.render_prep_timing.as_ms()).ok();
        }
        if perf.ui_timing.avg().as_nanos() > 0 {
            writeln!(report, "UI:          {:.2} ms", perf.ui_timing.as_ms()).ok();
        }

        // ECS systems
        if !perf.system_timings.is_empty() {
            writeln!(report).ok();
            writeln!(report, "=== ECS SYSTEMS ===").ok();
            let mut systems: Vec<_> = perf.system_timings.iter().collect();
            systems.sort_by(|a, b| b.1.as_ms().partial_cmp(&a.1.as_ms()).unwrap_or(std::cmp::Ordering::Equal));
            for (name, timing) in systems.iter().take(10) {
                writeln!(report, "  {:20} {:.2} ms", name, timing.as_ms()).ok();
            }
        }

        // Memory section
        writeln!(report).ok();
        writeln!(report, "=== MEMORY ===").ok();
        writeln!(report, "Visible Chunks: {}/{}", visible_chunks, total_chunks).ok();
        writeln!(report, "Loaded Chunks:  {}", perf.memory.loaded_chunks).ok();
        writeln!(report, "Loaded Meshes:  {}", perf.memory.loaded_meshes).ok();
        writeln!(report, "Voxel Memory:   {:.1} MB", perf.memory.voxel_memory_mb).ok();
        writeln!(report, "Mesh Memory:    {:.1} MB", perf.memory.mesh_memory_mb).ok();

        // Position section
        writeln!(report).ok();
        writeln!(report, "=== POSITION ===").ok();
        writeln!(report, "X:     {:.1}", camera_pos.0).ok();
        writeln!(report, "Y:     {:.1}", camera_pos.1).ok();
        writeln!(report, "Z:     {:.1}", camera_pos.2).ok();
        writeln!(report, "Chunk: {} {} {}", chunk_coords.0, chunk_coords.1, chunk_coords.2).ok();
        writeln!(report, "Yaw:   {:.1}°", camera_yaw).ok();
        writeln!(report, "Pitch: {:.1}°", camera_pitch).ok();

        // Block section
        writeln!(report).ok();
        writeln!(report, "=== BLOCK ===").ok();
        writeln!(report, "Selected: {} ({})", selected_block.1, selected_block.0).ok();
        writeln!(report, "Total:    {}", block_count - 1).ok();

        // Write to file
        match File::create(&filename) {
            Ok(mut file) => {
                if file.write_all(report.as_bytes()).is_ok() {
                    println!("[Debug] Stats dumped to {}", filename.display());
                } else {
                    println!("[Debug] Failed to write stats to file");
                }
            }
            Err(e) => {
                println!("[Debug] Failed to create stats file: {}", e);
            }
        }
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

    /// Ouvre le menu des paramètres
    pub fn open_settings(&mut self) {
        self.settings_open = true;
    }

    /// Ferme le menu des paramètres
    pub fn close_settings(&mut self) {
        self.settings_open = false;
    }

    /// Bascule l'état du menu des paramètres
    pub fn toggle_settings(&mut self) {
        self.settings_open = !self.settings_open;
    }

    /// Indique si le menu des paramètres consomme les entrées clavier
    pub fn wants_input(&self) -> bool {
        self.chat_open || self.settings_open
    }
}
