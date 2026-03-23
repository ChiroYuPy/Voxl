use crate::renderer::WgpuState;
use crate::raycast::{Ray, RAYCAST_DISTANCE, Raycast};
use crate::input::{InputManager, GameAction, KeyBindings};
use voxl_common::voxel::GlobalVoxelId;
use voxl_common::entities::GameMode;
use crate::debug::commands::{execute_command, CommandResult};
use crate::client_systems::{player_input_system, player_physics_system, jump_system};
use voxl_common::config::GameConfig;
use crate::game_state::GameState;
use crate::server_integration::ServerIntegration;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{WindowEvent, KeyEvent, MouseButton, DeviceEvent, Modifiers, ElementState},
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, CursorGrabMode},
};
use std::time::Instant;
use tracing::{info, warn, error, debug};

pub struct App {
    window: Option<Window>,
    wgpu_state: Option<WgpuState>,
    input: InputManager,
    config: GameConfig,
    is_closing: bool,
    modifiers: Modifiers,
    selected_block_id: GlobalVoxelId,  // Bloc actuellement sélectionné pour le placement
    was_chat_open: bool,  // État du chat à la frame précédente
    settings_open: bool,  // État du menu de paramètres
    last_frame_time: Instant,  // Pour le delta time frame-indépendant
    server_integration: ServerIntegration,  // Server integration
    tokio_runtime: Option<tokio::runtime::Runtime>,  // Runtime for async operations
    sequence_number: u32,  // Sequence number for packets
}

impl Default for App {
    fn default() -> Self {
        // Charger la configuration
        let config = GameConfig::load().unwrap_or_default();

        Self {
            window: None,
            wgpu_state: None,
            input: InputManager::with_bindings(KeyBindings::from_config_or_default(&config.keybindings)),
            config,
            is_closing: false,
            modifiers: Modifiers::default(),
            selected_block_id: 3, // Par défaut: stone (global_id 3)
            was_chat_open: false,  // Le chat est fermé au démarrage
            settings_open: false,  // Le menu de paramètres est fermé au démarrage
            last_frame_time: Instant::now(),
            server_integration: ServerIntegration::new(),
            tokio_runtime: None,
            sequence_number: 0,
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
        info!("Selected block: {} (id={})", name, block_id);
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

            // IMPORTANT: Check if chunk exists before allowing action
            let chunk_exists = if let Ok(world) = wgpu_state.world().read() {
                world.get_chunk_existing(pos.x >> 4, pos.y >> 4, pos.z >> 4).is_some()
            } else {
                false
            };

            if !chunk_exists {
                // Chunk doesn't exist, cannot interact
                return;
            }

            debug!("Breaking block at ({}, {}, {}) with id={}", pos.x, pos.y, pos.z, hit.block_type);

            // Apply change locally (optimistic update)
            wgpu_state.set_voxel(pos.x, pos.y, pos.z, None);

            // Send action to server (non-blocking)
            self.sequence_number = self.sequence_number.wrapping_add(1);
            let seq = self.sequence_number;

            use voxl_common::network::BlockActionType;
            self.server_integration.send_block_action(
                pos.x, pos.y, pos.z,
                BlockActionType::Break,
                seq
            );
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

            // IMPORTANT: Check if chunk exists before allowing action
            let chunk_exists = if let Ok(world) = wgpu_state.world().read() {
                world.get_chunk_existing(adjacent.x >> 4, adjacent.y >> 4, adjacent.z >> 4).is_some()
            } else {
                false
            };

            if !chunk_exists {
                // Chunk doesn't exist, cannot place block
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

            debug!("Placing block {} (id={}) at ({}, {}, {})",
                     block_name, self.selected_block_id, adjacent.x, adjacent.y, adjacent.z);
            wgpu_state.set_voxel(adjacent.x, adjacent.y, adjacent.z, Some(self.selected_block_id));

            // Send action to server (non-blocking)
            self.sequence_number = self.sequence_number.wrapping_add(1);
            let seq = self.sequence_number;

            use voxl_common::network::BlockActionType;
            self.server_integration.send_block_action(
                adjacent.x, adjacent.y, adjacent.z,
                BlockActionType::Place(self.selected_block_id as u32),
                seq
            );
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

            info!("Picked block: {} (id={})", block_name, hit.block_type);
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
                wgpu_state.teleport_player(pos);
                wgpu_state.add_chat_message(format!("§aTéléporté vers ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z), false);
            }
            CommandResult::TeleportRelative(pos) => {
                wgpu_state.teleport_player_relative(pos);
                wgpu_state.add_chat_message(format!("§aTéléporté relativement vers ({:.1}, {:.1}, {:.1})", pos.x, pos.y, pos.z), false);
            }
            CommandResult::None => {}
            CommandResult::ClearChat => {
                wgpu_state.clear_chat();
                wgpu_state.add_chat_message("Chat effacé".to_string(), false);
            }
            CommandResult::SetGameMode(mode) => {
                wgpu_state.set_game_mode(mode);
                let mode_name = mode.name();
                wgpu_state.add_chat_message(format!("§aMode de jeu changé: {}", mode_name), false);

                // Message spécial si on passe en spectateur
                if matches!(mode, GameMode::Spectator) {
                    wgpu_state.add_chat_message("§7Mode spectateur: vol activé, collisions désactivées".to_string(), false);
                }
            }
            CommandResult::ToggleFly => {
                if let Some(current_mode) = wgpu_state.get_game_mode() {
                    match current_mode {
                        GameMode::Spectator => {
                            wgpu_state.add_chat_message("§cImpossible de toggle le fly en mode spectateur!".to_string(), false);
                        }
                        GameMode::Creative { .. } => {
                            wgpu_state.toggle_fly();
                            let new_mode = wgpu_state.get_game_mode().unwrap();
                            let fly_status = if new_mode.is_flying() { "activé" } else { "désactivé" };
                            wgpu_state.add_chat_message(format!("§aMode vol {}", fly_status), false);
                        }
                    }
                }
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

            self.wgpu_state = Some(pollster::block_on(WgpuState::new(&window, &self.config)));
            self.window = Some(window);

            // Set the registry on server_integration for embedded mode
            if let Some(wgpu_state) = &self.wgpu_state {
                let registry = {
                    let world = wgpu_state.world().read().unwrap();
                    world.registry().clone()
                };
                self.server_integration.game_state.set_registry(registry);
                info!("[App] Set shared registry for embedded server mode");
            }

            // Create tokio runtime for async operations
            self.tokio_runtime = Some(tokio::runtime::Runtime::new()
                .expect("Failed to create tokio runtime"));

            // Start game and connect to server
            let server_config = self.config.server.clone();
            let mode = server_config.mode;
            let address = server_config.address;
            let port = server_config.port;

            info!("[App] Starting game in {:?} mode", mode);

            if let Some(runtime) = &self.tokio_runtime {
                let result = runtime.block_on(async {
                    self.server_integration.start(mode, address, port, "Player").await
                });

                if let Err(e) = result {
                    error!("[App] Failed to connect to server: {}", e);
                    // Continue anyway - we'll show a connection error in UI
                }
            }
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
                        error!("Render error: {e}");
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
            // Start frame timing
            if let Some(wgpu_state) = &mut self.wgpu_state {
                wgpu_state.performance_collector_mut().begin_frame();
            }

            // Vérifier si le chat ou les paramètres sont ouverts avant de traiter les inputs
            let (chat_open, settings_open) = if let Some(state) = &self.wgpu_state {
                (state.is_chat_open(), state.is_settings_open())
            } else {
                (false, false)
            };

            let ui_open = chat_open || settings_open;

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

            // Process network events (non-blocking, runs in background task!)
            let networking_start = Instant::now();
            if let Some(wgpu_state) = &mut self.wgpu_state {
                let world = wgpu_state.world().clone();
                let entities = wgpu_state.entity_world().clone();

                self.server_integration.process_network_events(&world, &entities);

                // Request meshes for chunks loaded from server
                let chunks_to_mesh = self.server_integration.chunk_tracker.get_chunks_to_mesh(50);
                if !chunks_to_mesh.is_empty() {
                    debug!("[App] Requesting meshes for {} chunks from server", chunks_to_mesh.len());
                    for (cx, cy, cz) in &chunks_to_mesh {
                        wgpu_state.request_immediate_rebuild(*cx, *cy, *cz);
                    }
                }

                // Check if chunks have meshes and mark them as clean (EVERY frame)
                let marked_count = self.server_integration.chunk_tracker.check_and_mark_meshed(|pos| {
                    wgpu_state.has_chunk_mesh(pos.0, pos.1, pos.2)
                });
                if marked_count > 0 {
                    debug!("[App] Marked {} chunks as meshed (have meshes)", marked_count);
                }
            }
            let networking_duration = networking_start.elapsed();
            if let Some(wgpu_state) = &mut self.wgpu_state {
                wgpu_state.performance_collector_mut().record_networking_time(networking_duration);
            }

            // Update ECS systems FIRST (before clearing input state)
            // Skip movement if chat or settings are open
            let world_update_start = Instant::now();
            let ecs_start = Instant::now();
            if let Some(wgpu_state) = &mut self.wgpu_state {
                if !wgpu_state.is_chat_open() && !wgpu_state.is_settings_open() {
                    let voxel_world_ref = wgpu_state.world().clone();

                    // Système d'input joueur: met à jour vélocité et direction du regard
                    let system_start = Instant::now();
                    player_input_system(wgpu_state.entity_world_mut().world(), self.input.state(), delta_time);
                    let system_duration = system_start.elapsed();
                    wgpu_state.performance_collector_mut().record_system_time("player_input", system_duration);

                    // Système de saut
                    let system_start = Instant::now();
                    {
                        let world_read = voxel_world_ref.read().unwrap();
                        jump_system(wgpu_state.entity_world_mut().world(), self.input.state(), &world_read);
                    }
                    let system_duration = system_start.elapsed();
                    wgpu_state.performance_collector_mut().record_system_time("jump", system_duration);

                    // Système de physique: applique gravité et collisions
                    let system_start = Instant::now();
                    {
                        let world_read = voxel_world_ref.read().unwrap();
                        player_physics_system(wgpu_state.entity_world_mut().world(), &world_read, delta_time);
                    }
                    let system_duration = system_start.elapsed();
                    wgpu_state.performance_collector_mut().record_system_time("physics", system_duration);

                    // Synchroniser la caméra avec la position du joueur
                    wgpu_state.update_camera_from_player();

                    // Send player update to server (if connected)
                    let entity_world = wgpu_state.entity_world();

                    // Get player position and look direction from ECS
                    let (player_pos, yaw, pitch, on_ground) = {
                        let mut pos = None;
                        let mut yaw = 0.0;
                        let mut pitch = 0.0;
                        let mut ground = false;

                        for (position, look, physics) in entity_world.world_read().query::<(
                            &voxl_common::entities::Position,
                            &voxl_common::entities::LookDirection,
                            &voxl_common::entities::PhysicsAffected
                        )>().iter() {
                            pos = Some((position.x, position.y, position.z));
                            yaw = look.yaw;
                            pitch = look.pitch;
                            ground = physics.on_ground;
                            break;  // Only first player
                        }

                        (pos, yaw, pitch, ground)
                    };

                    if let Some((x, y, z)) = player_pos {
                        self.sequence_number = self.sequence_number.wrapping_add(1);
                        let seq = self.sequence_number;

                        // Non-blocking send
                        self.server_integration.send_player_update(x, y, z, yaw, pitch, on_ground, seq);
                    }
                }

                // Mesh processing timing
                let mesh_start = Instant::now();
                wgpu_state.process_mesh_updates();
                let mesh_duration = mesh_start.elapsed();
                wgpu_state.performance_collector_mut().record_mesh_processing_time(mesh_duration);
            }
            let ecs_duration = ecs_start.elapsed();
            let world_update_duration = world_update_start.elapsed();

            // Record CPU time and world update time
            if let Some(wgpu_state) = &mut self.wgpu_state {
                wgpu_state.performance_collector_mut().record_cpu_time(ecs_duration);
                wgpu_state.performance_collector_mut().record_world_update_time(world_update_duration);
            }

            // Update memory stats (separate block to avoid borrow conflicts)
            if let Some(wgpu_state) = &mut self.wgpu_state {
                // Collect data first
                let (loaded_chunks, loaded_meshes, pending_requests) = {
                    let world = wgpu_state.world();
                    if let Ok(world_read) = world.read() {
                        (
                            world_read.chunk_count(),
                            wgpu_state.loaded_meshes_count(),
                            wgpu_state.pending_mesh_requests_count(),
                        )
                    } else {
                        (0, 0, 0)
                    }
                };

                // Estimate memory usage
                let voxel_memory_mb = (loaded_chunks * std::mem::size_of::<voxl_common::voxel::VoxelChunk>()) as f64 / (1024.0 * 1024.0);
                let mesh_memory_mb = (loaded_meshes * 1024) as f64 / (1024.0 * 1024.0);

                let memory_stats = crate::performance::MemoryStats {
                    loaded_chunks,
                    loaded_meshes,
                    pending_mesh_requests: pending_requests,
                    voxel_memory_mb,
                    mesh_memory_mb,
                };

                wgpu_state.performance_collector_mut().update_memory_stats(|| memory_stats);
            }

            // Handle one-shot actions (skip if chat or settings are open)
            if !ui_open {
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

                // Toggle debug UI (F3)
                if self.input.state().just_pressed(GameAction::ToggleDebugUI) {
                    if let Some(state) = &mut self.wgpu_state {
                        state.toggle_debug_ui();
                        debug!("Debug UI: {}", state.is_debug_ui_enabled());
                    }
                }

                // Dump stats to file (F7)
                if self.input.state().just_pressed(GameAction::DumpStats) {
                    if let Some(state) = &mut self.wgpu_state {
                        state.request_dump_stats();
                    }
                }

                // Toggle fly mode (F5)
                if self.input.state().just_pressed(GameAction::ToggleFly) {
                    if let Some(state) = &mut self.wgpu_state {
                        if let Some(current_mode) = state.get_game_mode() {
                            match current_mode {
                                GameMode::Spectator => {
                                    state.add_chat_message("§cImpossible de toggle le fly en mode spectateur!".to_string(), false);
                                }
                                GameMode::Creative { .. } => {
                                    state.toggle_fly();
                                    let new_mode = state.get_game_mode().unwrap();
                                    let fly_status = if new_mode.is_flying() { "activé" } else { "désactivé" };
                                    state.add_chat_message(format!("§aMode vol {}", fly_status), false);
                                }
                            }
                        }
                    }
                }

                // Toggle chunk borders (F6)
                if self.input.state().just_pressed(GameAction::ToggleChunkBorders) {
                    if let Some(state) = &mut self.wgpu_state {
                        state.toggle_chunk_borders();
                    }
                }

                // Open settings menu (F4)
                if self.input.state().just_pressed(GameAction::OpenSettings) {
                    if let Some(state) = &mut self.wgpu_state {
                        state.egui_state_mut().toggle_settings();
                        // Release mouse when opening settings
                        if self.input.state().is_mouse_captured() {
                            self.input.set_mouse_captured(false);
                            if let Some(window) = &self.window {
                                Self::release_mouse(window);
                            }
                        }
                    }
                }

                // Cycle gamemode (G ou molette)
                if self.input.state().just_pressed(GameAction::CycleGameMode) {
                    if let Some(state) = &mut self.wgpu_state {
                        let new_mode = match state.get_game_mode() {
                            Some(GameMode::Creative { .. }) => {
                                GameMode::Spectator
                            }
                            Some(GameMode::Spectator) => {
                                GameMode::Creative { fly_enabled: true }
                            }
                            None => GameMode::Creative { fly_enabled: true },
                        };
                        state.set_game_mode(new_mode);
                        let mode_name = new_mode.name();
                        state.add_chat_message(format!("§aMode de jeu changé: {}", mode_name), false);
                        if matches!(new_mode, GameMode::Spectator) {
                            state.add_chat_message("§7Mode spectateur: vol activé, collisions désactivées".to_string(), false);
                        }
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

            // Check for stats dump request (F7)
            if let Some(wgpu_state) = &mut self.wgpu_state {
                wgpu_state.check_dump_stats();
            }

            // Update input state LAST (clears delta for next frame)
            let input_start = Instant::now();
            self.input.update();
            let input_duration = input_start.elapsed();
            if let Some(wgpu_state) = &mut self.wgpu_state {
                wgpu_state.performance_collector_mut().record_input_time(input_duration);
            }

            // FPS limiting
            if let Some(state) = &self.wgpu_state {
                if let Some(min_frame_time) = state.min_frame_time() {
                    let elapsed = self.last_frame_time.elapsed();
                    if elapsed < min_frame_time {
                        std::thread::sleep(min_frame_time - elapsed);
                    }
                }
            }

            // End frame timing
            if let Some(wgpu_state) = &mut self.wgpu_state {
                wgpu_state.performance_collector_mut().end_frame();
            }

            // Request redraw
            if let Some(window) = &self.window {
                window.request_redraw();
            }
        }
    }
}

fn build_title_string() -> String {
    "Voxl Reborn".to_string()
}

pub fn run() {
    use tracing_subscriber::prelude::*;

    let subscriber = tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"))
        )
        .with(tracing_subscriber::fmt::layer());

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    // Initialize all required directories
    if let Err(e) = voxl_common::paths::init_directories() {
        warn!("Failed to create directories: {}", e);
    }

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App::default();

    if let Err(e) = event_loop.run_app(&mut app) {
        error!("Event loop error: {e}");
    }
}
