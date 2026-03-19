use crate::renderer::WgpuState;
use crate::raycast::{Ray, RAYCAST_DISTANCE, Raycast};
use crate::input::{InputManager, PlayerController, GameAction};
use crate::voxel::GlobalVoxelId;
use crate::debug::commands::{execute_command, CommandResult};
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{WindowEvent, KeyEvent, MouseButton, DeviceEvent, Modifiers, ElementState},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, CursorGrabMode},
};
use std::time::Instant;

pub struct App {
    window: Option<Window>,
    wgpu_state: Option<WgpuState>,
    player: PlayerController,
    input: InputManager,
    is_closing: bool,
    modifiers: Modifiers,
    selected_block_id: GlobalVoxelId,  // Bloc actuellement sélectionné pour le placement
    was_chat_open: bool,  // État du chat à la frame précédente
    last_frame_time: Instant,  // Pour le delta time frame-indépendant
}

impl Default for App {
    fn default() -> Self {
        Self {
            window: None,
            wgpu_state: None,
            player: PlayerController::default(),
            input: InputManager::default(),
            is_closing: false,
            modifiers: Modifiers::default(),
            selected_block_id: 3, // Par défaut: stone (global_id 3)
            was_chat_open: false,  // Le chat est fermé au démarrage
            last_frame_time: Instant::now(),
        }
    }
}

impl App {
    fn toggle_mouse_capture(window: &Window) {
        let _ = window.set_cursor_grab(CursorGrabMode::Confined);
        window.set_cursor_visible(false);
    }

    fn release_mouse(window: &Window) {
        let _ = window.set_cursor_grab(CursorGrabMode::None);
        window.set_cursor_visible(true);
    }

    /// Change le bloc sélectionné et log le changement
    fn set_selected_block(&mut self, block_id: GlobalVoxelId, name: &str) {
        self.selected_block_id = block_id;
        println!("Selected block: {} (id={})", name, block_id);
    }

    fn handle_action_break_block(&mut self) {
        let wgpu_state = if let Some(s) = &mut self.wgpu_state { s } else { return };
        let camera = wgpu_state.camera();
        let ray = Ray::new(camera.position.into(), camera.forward().into());

        let hit = {
            if let Ok(world) = wgpu_state.world().read() {
                ray.cast_blocks(RAYCAST_DISTANCE, |pos| {
                    world.get_voxel_opt(pos.x, pos.y, pos.z)
                })
            } else {
                None
            }
        };

        if let Some(hit) = hit {
            let pos = hit.block_pos;
            println!("Breaking block at ({}, {}, {}) with id={}", pos.x, pos.y, pos.z, hit.block_type);
            wgpu_state.set_voxel(pos.x, pos.y, pos.z, None);
        }
    }

    fn handle_action_place_block(&mut self) {
        let wgpu_state = if let Some(s) = &mut self.wgpu_state { s } else { return };
        let camera = wgpu_state.camera();
        let ray = Ray::new(camera.position.into(), camera.forward().into());

        let hit = {
            if let Ok(world) = wgpu_state.world().read() {
                ray.cast_blocks(RAYCAST_DISTANCE, |pos| {
                    world.get_voxel_opt(pos.x, pos.y, pos.z)
                })
            } else {
                None
            }
        };

        if let Some(hit) = hit {
            let adjacent = hit.adjacent_pos();

            let player_pos = camera.position;
            let block_center = glam::Vec3::new(
                adjacent.x as f32 + 0.5,
                adjacent.y as f32 + 0.5,
                adjacent.z as f32 + 0.5,
            );

            let dx = (player_pos.x - block_center.x).abs();
            let dy = (player_pos.y - block_center.y).abs();
            let dz = (player_pos.z - block_center.z).abs();

            if dx < 0.8 && dz < 0.8 && dy < 1.8 {
                return;
            }

            // Récupérer le nom du bloc sélectionné pour le log
            let block_name = {
                let world_guard = wgpu_state.world().read().unwrap();
                let registry = world_guard.registry();
                registry.get(self.selected_block_id)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string())
            };

            println!("Placing block {} (id={}) at ({}, {}, {})",
                     block_name, self.selected_block_id, adjacent.x, adjacent.y, adjacent.z);
            wgpu_state.set_voxel(adjacent.x, adjacent.y, adjacent.z, Some(self.selected_block_id));
        }
    }

    fn handle_action_pick_block(&mut self) {
        let wgpu_state = if let Some(s) = &mut self.wgpu_state { s } else { return };
        let camera = wgpu_state.camera();
        let ray = Ray::new(camera.position.into(), camera.forward().into());

        let hit = {
            if let Ok(world) = wgpu_state.world().read() {
                ray.cast_blocks(RAYCAST_DISTANCE, |pos| {
                    world.get_voxel_opt(pos.x, pos.y, pos.z)
                })
            } else {
                None
            }
        };

        if let Some(hit) = hit {
            // Récupérer le nom du bloc pour le log
            let block_name = {
                let world_guard = wgpu_state.world().read().unwrap();
                let registry = world_guard.registry();
                registry.get(hit.block_type)
                    .map(|def| def.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string())
            };

            println!("Picked block: {} (id={})", block_name, hit.block_type);
            self.selected_block_id = hit.block_type;
        }
    }

    fn handle_action_next_block(&mut self) {
        let wgpu_state = if let Some(s) = &self.wgpu_state { s } else { return };

        // Récupérer tous les blocs disponibles
        let block_count = {
            let world_guard = wgpu_state.world().read().unwrap();
            let registry = world_guard.registry();
            registry.len()
        };

        // Passer au bloc suivant (en ignorant l'air qui est id=0)
        let next_id = if self.selected_block_id + 1 >= block_count {
            1  // Retour au premier bloc après l'air
        } else {
            self.selected_block_id + 1
        };

        // Ignorer l'air (id=0)
        let final_id = if next_id == 0 { 1 } else { next_id };

        let block_name = {
            let world_guard = wgpu_state.world().read().unwrap();
            let registry = world_guard.registry();
            registry.get(final_id)
                .map(|def| def.name.clone())
                .unwrap_or_else(|| "Unknown".to_string())
        };

        self.set_selected_block(final_id, &block_name);
    }

    fn handle_action_previous_block(&mut self) {
        let wgpu_state = if let Some(s) = &self.wgpu_state { s } else { return };

        let block_count = {
            let world_guard = wgpu_state.world().read().unwrap();
            let registry = world_guard.registry();
            registry.len()
        };

        // Bloc précédent (en ignorant l'air qui est id=0)
        let prev_id = if self.selected_block_id <= 1 {
            block_count - 1  // Dernier bloc
        } else {
            self.selected_block_id - 1
        };

        let final_id = if prev_id == 0 { block_count - 1 } else { prev_id };

        let block_name = {
            let world_guard = wgpu_state.world().read().unwrap();
            let registry = world_guard.registry();
            registry.get(final_id)
                .map(|def| def.name.clone())
                .unwrap_or_else(|| "Unknown".to_string())
        };

        self.set_selected_block(final_id, &block_name);
    }

    fn handle_chat_command(&mut self, command: String) {
        let wgpu_state = if let Some(s) = &mut self.wgpu_state { s } else { return };
        let current_pos = wgpu_state.camera().position;

        let result = execute_command(&command, current_pos.into());

        match result {
            CommandResult::Success(msg) => {
                wgpu_state.add_chat_message(msg, false);
            }
            CommandResult::Error(msg) => {
                wgpu_state.add_chat_message(format!("§cErreur: {}", msg), false);
            }
            CommandResult::Teleport(pos) => {
                wgpu_state.camera_mut().position = pos.into();
                wgpu_state.add_chat_message(format!("§aTéléporté vers ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z), false);
            }
            CommandResult::TeleportRelative(pos) => {
                wgpu_state.camera_mut().position = pos.into();
                wgpu_state.add_chat_message(format!("§aTéléporté relativement vers ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z), false);
            }
            CommandResult::None => {}
            CommandResult::ClearChat => {
                wgpu_state.clear_chat();
                wgpu_state.add_chat_message("Chat effacé".to_string(), false);
            }
        }
    }

    fn update_block_highlight(&mut self) {
        let wgpu_state = if let Some(s) = &mut self.wgpu_state { s } else { return };

        if !self.input.state().is_mouse_captured() {
            wgpu_state.set_highlight_target(None);
            return;
        }

        let camera = wgpu_state.camera();
        let ray = Ray::new(camera.position.into(), camera.forward().into());

        let hit = {
            if let Ok(world) = wgpu_state.world().read() {
                ray.cast_blocks(RAYCAST_DISTANCE, |pos| {
                    world.get_voxel_opt(pos.x, pos.y, pos.z)
                })
            } else {
                wgpu_state.set_highlight_target(None);
                return;
            }
        };

        if let Some(hit) = hit {
            let pos = hit.block_pos;
            wgpu_state.set_highlight_target(Some(crate::renderer::HighlightTarget {
                x: pos.x,
                y: pos.y,
                z: pos.z,
                face: hit.face,
            }));
        } else {
            wgpu_state.set_highlight_target(None);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let window = event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title(build_title_string())
                        .with_inner_size(LogicalSize::new(1280, 720))
                        .with_visible(true),
                )
                .expect("Failed to create window");

            self.wgpu_state = Some(pollster::block_on(WgpuState::new(&window)));
            self.window = Some(window);
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        match event {
            DeviceEvent::MouseMotion { delta } => {
                self.input.on_mouse_motion(delta.0, delta.1);
            }
            _ => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        if self.is_closing {
            return;
        }

        if let WindowEvent::ModifiersChanged(modifiers) = event {
            self.modifiers = modifiers;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.is_closing = true;
                self.wgpu_state.take();
                self.window.take();
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                if let Some(state) = &mut self.wgpu_state {
                    state.resize(new_size);
                }
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key, state, .. },
                ..
            } => {
                // Passer l'event à egui si le chat est ouvert
                let chat_open = self.wgpu_state.as_ref().map_or(false, |s| s.is_chat_open());
                if chat_open {
                    if let Some(wgpu_state) = &mut self.wgpu_state {
                        wgpu_state.handle_key_event(&logical_key, state == ElementState::Pressed);
                    }
                }

                match state {
                    ElementState::Pressed => {
                        self.input.on_key_press(logical_key.clone());
                    }
                    ElementState::Released => {
                        self.input.on_key_release(&logical_key);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // Vérifier si le chat est ouvert
                let chat_open = self.wgpu_state.as_ref().map_or(false, |s| s.is_chat_open());

                if chat_open {
                    // Passer le clic à egui
                    let winit_btn = match button {
                        MouseButton::Left => Some(1),
                        MouseButton::Middle => Some(2),
                        MouseButton::Right => Some(3),
                        _ => None,
                    };
                    if let Some(btn) = winit_btn {
                        if let Some(wgpu_state) = &mut self.wgpu_state {
                            wgpu_state.handle_mouse_click(state == ElementState::Pressed, btn);
                        }
                    }
                    return; // Ne pas traiter par le système d'input
                }

                if state == ElementState::Pressed {
                    // Capture mouse on first click if not already captured
                    if !self.input.state().is_mouse_captured() {
                        if let Some(window) = &self.window {
                            self.input.set_mouse_captured(true);
                            Self::toggle_mouse_capture(window);
                        }
                    }

                    let winit_btn = match button {
                        MouseButton::Left => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 3,
                        MouseButton::Back => 4,
                        MouseButton::Forward => 5,
                        _ => return,
                    };
                    self.input.on_mouse_press(winit_btn);
                } else {
                    let winit_btn = match button {
                        MouseButton::Left => 1,
                        MouseButton::Middle => 2,
                        MouseButton::Right => 3,
                        MouseButton::Back => 4,
                        MouseButton::Forward => 5,
                        _ => return,
                    };
                    self.input.on_mouse_release(winit_btn);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                // Vérifier si le chat est ouvert
                let chat_open = self.wgpu_state.as_ref().map_or(false, |s| s.is_chat_open());
                if chat_open {
                    return; // Bloquer la molette quand le chat est ouvert
                }

                // Gérer la molette pour changer de bloc
                let delta_y: f32 = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => y,
                    winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };

                if delta_y > 0.0 {
                    self.input.on_mouse_press(4); // Molette haut = bouton 4
                    self.input.on_mouse_release(4);
                } else if delta_y < 0.0 {
                    self.input.on_mouse_press(5); // Molette bas = bouton 5
                    self.input.on_mouse_release(5);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Passer la position à egui pour le chat, mais ne pas traiter les mouvements de caméra
                if let Some(wgpu_state) = &mut self.wgpu_state {
                    wgpu_state.handle_mouse_move(position.x, position.y);
                }

                let chat_open = self.wgpu_state.as_ref().map_or(false, |s| s.is_chat_open());
                if !chat_open {
                    self.input.on_mouse_move(position.x, position.y);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(state) = &mut self.wgpu_state {
                    let block_name = {
                        let world_guard = state.world().read().unwrap();
                        let registry = world_guard.registry();
                        registry.get(self.selected_block_id)
                            .map(|def| def.name.clone())
                            .unwrap_or_else(|| "Unknown".to_string())
                    };
                    if let Err(e) = state.render((self.selected_block_id as u32, block_name)) {
                        eprintln!("Render error: {e}");
                    }
                }
            }
            WindowEvent::Focused(focused) => {
                if !focused && self.input.state().is_mouse_captured() {
                    self.input.set_mouse_captured(false);
                    if let Some(window) = &self.window {
                        Self::release_mouse(window);
                    }
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if !self.is_closing {
            // Vérifier si le chat est ouvert avant de traiter les inputs
            let chat_open = if let Some(state) = &self.wgpu_state {
                state.is_chat_open()
            } else {
                false
            };

            // Open chat action - vérifier que le chat n'était PAS ouvert à la frame précédente
            // pour éviter de réouvrir immédiatement après fermeture
            if self.input.state().just_pressed(GameAction::OpenChat) {
                if !chat_open && !self.was_chat_open {
                    if let Some(state) = &mut self.wgpu_state {
                        state.open_chat();
                        // Release mouse when opening chat
                        if self.input.state().is_mouse_captured() {
                            self.input.set_mouse_captured(false);
                            if let Some(window) = &self.window {
                                Self::release_mouse(window);
                            }
                        }
                    }
                }
            }

            // Handle Escape to close chat
            if chat_open && self.input.state().just_pressed(GameAction::ReleaseMouse) {
                if let Some(state) = &mut self.wgpu_state {
                    state.close_chat();
                }
            }

            // Détecter si le chat vient de se fermer pour recapturer la souris
            if self.was_chat_open && !chat_open {
                if let Some(window) = &self.window {
                    self.input.set_mouse_captured(true);
                    Self::toggle_mouse_capture(window);
                }
            }

            // Calculate delta time for frame-independent movement
            let now = Instant::now();
            let delta_time = now.duration_since(self.last_frame_time).as_secs_f32();
            self.last_frame_time = now;

            // Cap delta time to prevent huge jumps (e.g. if window was dragged)
            let delta_time = delta_time.min(0.1);

            // Update player/camera FIRST (before clearing input state)
            // Skip movement if chat is open
            if let Some(wgpu_state) = &mut self.wgpu_state {

                if !wgpu_state.is_chat_open() {
                    self.player.update_direct(wgpu_state.camera_mut(), self.input.state(), delta_time);
                }
                wgpu_state.process_mesh_updates();
            }

            // Handle one-shot actions (skip if chat is open)
            if !chat_open {
                if self.input.state().just_pressed(GameAction::ToggleMouseCapture) {
                    if let Some(window) = &self.window {
                        let captured = !self.input.state().is_mouse_captured();
                        self.input.set_mouse_captured(captured);
                        if captured {
                            Self::toggle_mouse_capture(window);
                        } else {
                            Self::release_mouse(window);
                        }
                    }
                }

                if self.input.state().just_pressed(GameAction::ReleaseMouse) {
                    if let Some(window) = &self.window {
                        self.input.set_mouse_captured(false);
                        Self::release_mouse(window);
                    }
                }

                // Handle continuous actions (block breaking/placing while held)
                if self.input.state().just_pressed(GameAction::BreakBlock) {
                    if self.input.state().is_mouse_captured() {
                        self.handle_action_break_block();
                    }
                }

                if self.input.state().just_pressed(GameAction::PlaceBlock) {
                    if self.input.state().is_mouse_captured() {
                        self.handle_action_place_block();
                    }
                }

                // Pick block avec middle click
                if self.input.state().just_pressed(GameAction::PickBlock) {
                    if self.input.state().is_mouse_captured() {
                        self.handle_action_pick_block();
                    }
                }

                // Changer de bloc avec la molette
                if self.input.state().just_pressed(GameAction::NextBlockType) {
                    self.handle_action_next_block();
                }

                if self.input.state().just_pressed(GameAction::PreviousBlockType) {
                    self.handle_action_previous_block();
                }

                // Toggle debug UI
                if self.input.state().just_pressed(GameAction::ToggleDebugUI) {
                    if let Some(state) = &mut self.wgpu_state {
                        state.toggle_debug_ui();
                        println!("Debug UI: {}", state.is_debug_ui_enabled());
                    }
                }

                // Update highlight
                self.update_block_highlight();
            }

            // Check for chat commands from egui
            let submitted_command = if let Some(wgpu_state) = &mut self.wgpu_state {
                wgpu_state.get_submitted_command()
            } else {
                None
            };

            if let Some(command) = submitted_command {
                self.handle_chat_command(command);
            }

            // Check si le chat vient de se fermer (après envoi message)
            if let Some(wgpu_state) = &mut self.wgpu_state {
                if wgpu_state.take_chat_just_closed() {
                    if let Some(window) = &self.window {
                        self.input.set_mouse_captured(true);
                        Self::toggle_mouse_capture(window);
                    }
                }
            }

            // Mettre à jour was_chat_open pour la prochaine frame
            self.was_chat_open = chat_open;

            // Update input state LAST (clears delta for next frame)
            self.input.update();

            // Request redraw
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

/// Build the window title with keybind hints.
fn build_title_string() -> String {
    format!(
        "Voxl Reborn | \
        ZQSD/WASD: Move | \
        Space/Shift: Up/Down | \
        Click: Capture | \
        LMB: Break | \
        RMB: Place | \
        MMB: Pick Block | \
        Scroll: Select Block | \
        Esc: Release"
    )
}

/// Run the application.
pub fn run() {
    env_logger::init();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::default();

    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("Event loop error: {e}");
    }
}
