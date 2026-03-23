use voxl_common::config::GameConfig;
use crate::input::GameAction;
use tracing::error;

/// État pour les onglets
#[derive(Default, Copy, Clone, PartialEq)]
enum SettingsTab {
    #[default]
    Graphics,
    Controls,
    About,
}

/// Affiche le menu de paramètres
pub fn settings_menu(ctx: &egui::Context, config: &mut GameConfig, open: &mut bool) {
    let mut current_tab = SettingsTab::default();

    if !*open {
        return;
    }

    egui::Window::new("Paramètres")
        .resizable(true)
        .default_width(600.0)
        .show(ctx, |ui| {
            // Sélection d'onglet
            ui.horizontal(|ui| {
                ui.selectable_value(&mut current_tab, SettingsTab::Graphics, "Graphismes");
                ui.selectable_value(&mut current_tab, SettingsTab::Controls, "Contrôles");
                ui.selectable_value(&mut current_tab, SettingsTab::About, "À propos");
            });
            ui.separator();

            match current_tab {
                SettingsTab::Graphics => {
                    ui.heading("Paramètres Graphiques");
                    ui.separator();

                    ui.add_space(10.0);

                    // Distance de rendu
                    ui.horizontal(|ui| {
                        ui.label("Distance de rendu:");
                        ui.add(egui::Slider::new(&mut config.graphics.render_distance, 2..=32)
                            .text("chunks")
                            .step_by(1.0));
                    });
                    ui.label("Nombre de chunks à charger autour du joueur.");

                    ui.add_space(10.0);

                    // Ambient Occlusion
                    ui.horizontal(|ui| {
                        ui.label("Ambient Occlusion:");
                        ui.add(egui::Slider::new(&mut config.graphics.ao_intensity, 0.0..=1.0)
                            .text("intensité")
                            .step_by(0.05));
                    });
                    ui.label("Effet d'ombrage dans les coins. 0.0 = désactivé, 1.0 = maximum.");

                    ui.add_space(10.0);

                    // VSync
                    ui.checkbox(&mut config.graphics.vsync, "VSync (Synchronisation verticale)");
                    ui.label("Synchronise les FPS avec le taux de rafraîchissement.");

                    ui.add_space(10.0);

                    // FPS max (seulement si VSync est désactivé)
                    if !config.graphics.vsync {
                        ui.horizontal(|ui| {
                            ui.label("FPS Maximum:");

                            let is_unlimited = config.graphics.max_fps <= 0;

                            if is_unlimited {
                                ui.label("Illimité");
                            } else {
                                ui.add(egui::Slider::new(&mut config.graphics.max_fps, 0..=360)
                                    .text("fps")
                                    .step_by(10.0));
                                ui.label("(0 = Illimité)");
                            }

                            if ui.button(if is_unlimited { "Limiter" } else { "Illimité" }).clicked() {
                                if is_unlimited {
                                    config.graphics.max_fps = 144;
                                } else {
                                    config.graphics.max_fps = 0;
                                }
                            }
                        });
                        ui.label("Limite le nombre d'images par seconde (0 = illimité).");
                    }

                    ui.add_space(20.0);
                }

                SettingsTab::Controls => {
                    ui.heading("Configuration des touches");
                    ui.separator();

                    ui.add_space(10.0);

                    ui.label("Cliquez sur un bouton pour rebind la touche correspondante.");
                    ui.label("Appuyez sur Échap pour annuler.");

                    ui.add_space(10.0);

                    // Afficher les bindings par catégorie
                    controls_section(ui, "Déplacement", &[
                        GameAction::MoveForward,
                        GameAction::MoveBackward,
                        GameAction::MoveLeft,
                        GameAction::MoveRight,
                        GameAction::MoveUp,
                        GameAction::MoveDown,
                    ], config);

                    controls_section(ui, "Interactions", &[
                        GameAction::BreakBlock,
                        GameAction::PlaceBlock,
                        GameAction::PickBlock,
                        GameAction::NextBlockType,
                        GameAction::PreviousBlockType,
                    ], config);

                    controls_section(ui, "Contrôles", &[
                        GameAction::ToggleMouseCapture,
                        GameAction::ReleaseMouse,
                        GameAction::IncreaseSpeed,
                        GameAction::DecreaseSpeed,
                    ], config);

                    controls_section(ui, "Interface", &[
                        GameAction::ToggleDebugUI,
                        GameAction::OpenChat,
                        GameAction::OpenSettings,
                    ], config);

                    controls_section(ui, "Modes de jeu", &[
                        GameAction::ToggleFly,
                        GameAction::CycleGameMode,
                        GameAction::ToggleChunkBorders,
                    ], config);

                    ui.add_space(20.0);
                }

                SettingsTab::About => {
                    ui.heading("Voxl - Jeu Voxel");
                    ui.separator();

                    ui.add_space(10.0);

                    ui.label("Version 0.1.0");
                    ui.label("Un moteur de jeu voxel en Rust utilisant wgpu.");

                    ui.add_space(20.0);

                    ui.heading("Contrôles par défaut:");
                    ui.label("• Z/Q/S/D ou W/A/S/D: Déplacement");
                    ui.label("• Espace: Monter | Shift: Descendre");
                    ui.label("• Clic gauche: Casser un bloc");
                    ui.label("• Clic droit: Placer un bloc");
                    ui.label("• Molette: Changer de bloc");
                    ui.label("• F3: Interface de debug");
                    ui.label("• F4: Paramètres");
                    ui.label("• T: Chat");

                    ui.add_space(20.0);

                    ui.label("Fichier de configuration: config.toml");
                }
            }

            ui.separator();

            ui.horizontal(|ui| {
                if ui.button("Réinitialiser par défaut").clicked() {
                    *config = GameConfig::default();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Annuler").clicked() {
                        *open = false;
                    }
                    if ui.button("Sauvegarder").clicked() {
                        if let Err(e) = config.save() {
                            error!("Erreur lors de la sauvegarde: {}", e);
                        } else {
                            *open = false;
                        }
                    }
                });
            });
        });
}

/// Affiche une section de contrôles
fn controls_section(
    ui: &mut egui::Ui,
    title: &str,
    actions: &[GameAction],
    config: &GameConfig,
) {
    ui.heading(title);
    ui.add_space(5.0);

    egui::Grid::new(format!("controls_{}", title))
        .num_columns(3)
        .spacing([10.0, 5.0])
        .show(ui, |ui| {
            for action in actions {
                let display_name = format_action_name(*action);

                ui.label(display_name);

                // Afficher les touches actuelles
                let key_strings = get_keys_for_action(*action, config);
                ui.label(key_strings);

                // Bouton Rebind (désactivé pour l'instant)
                ui.button("Rebind").on_disabled_hover_text("Fonctionnalité à venir");

                ui.end_row();
            }
        });

    ui.add_space(10.0);
}

/// Formate le nom d'une action pour l'affichage
fn format_action_name(action: GameAction) -> String {
    match action {
        GameAction::MoveForward => "Avancer",
        GameAction::MoveBackward => "Reculer",
        GameAction::MoveLeft => "Gauche",
        GameAction::MoveRight => "Droite",
        GameAction::MoveUp => "Monter",
        GameAction::MoveDown => "Descendre",
        GameAction::LookUp => "Regarder en haut",
        GameAction::LookDown => "Regarder en bas",
        GameAction::LookLeft => "Regarder à gauche",
        GameAction::LookRight => "Regarder à droite",
        GameAction::BreakBlock => "Casser un bloc",
        GameAction::PlaceBlock => "Placer un bloc",
        GameAction::PickBlock => "Prendre un bloc",
        GameAction::NextBlockType => "Bloc suivant",
        GameAction::PreviousBlockType => "Bloc précédent",
        GameAction::ToggleMouseCapture => "Capturer souris",
        GameAction::ReleaseMouse => "Libérer souris",
        GameAction::IncreaseSpeed => "Augmenter vitesse",
        GameAction::DecreaseSpeed => "Diminuer vitesse",
        GameAction::ToggleDebugUI => "Interface debug",
        GameAction::DumpStats => "Sauver stats",
        GameAction::OpenChat => "Ouvrir chat",
        GameAction::ToggleFly => "Mode vol",
        GameAction::CycleGameMode => "Changer mode",
        GameAction::ToggleChunkBorders => "Bordures chunks",
        GameAction::OpenSettings => "Paramètres",
    }.to_string()
}

/// Récupère les touches bindées pour une action
fn get_keys_for_action(action: GameAction, config: &GameConfig) -> String {
    let action_name = format!("{:?}", action);
    if let Some(keys) = config.keybindings.bindings.get(&action_name) {
        keys.join(", ")
    } else {
        "Non assigné".to_string()
    }
}
