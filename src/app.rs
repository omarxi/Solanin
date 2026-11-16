use crate::catalog::{fetch_catalog, load_cached_catalog, load_state, save_state};
use crate::mod_manager::{disable_mod, enable_mod, install_mod, uninstall_mod};
use crate::models::{AppState, InstalledMod, ModCatalog, ModEntry, STEAM_APP_ID};
use crate::paths::{detect_data_dir, detect_game_dir, LauncherPaths};
use eframe::egui;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Browse,
    Installed,
    Settings,
}

enum WorkerMsg {
    CatalogFetched(Result<ModCatalog, String>),
    ModInstalled {
        id: String,
        result: Result<InstalledMod, String>,
    },
}

pub struct ModLauncherApp {
    paths: LauncherPaths,
    state: AppState,
    catalog: ModCatalog,
    active_tab: Tab,
    search_query: String,
    selected_catalog_id: Option<String>,
    status_message: String,
    busy: bool,
    tx: Sender<WorkerMsg>,
    rx: Receiver<WorkerMsg>,
    // Editable settings buffers
    catalog_url_input: String,
    game_dir_input: String,
    data_dir_input: String,
}

impl ModLauncherApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let paths = LauncherPaths::resolve().expect("Failed to resolve launcher data directory");
        let state = load_state(&paths);
        let catalog = load_cached_catalog(&paths).unwrap_or_default();
        let (tx, rx) = mpsc::channel();

        let catalog_url_input = state.config.catalog_url.clone();
        let game_dir_input = state
            .config
            .game_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let data_dir_input = state
            .config
            .data_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        let mut app = Self {
            paths,
            state,
            catalog,
            active_tab: Tab::Browse,
            search_query: String::new(),
            selected_catalog_id: None,
            status_message: "Ready.".to_string(),
            busy: false,
            tx,
            rx,
            catalog_url_input,
            game_dir_input,
            data_dir_input,
        };

        // Persist auto-detected paths on first run.
        let _ = save_state(&app.paths, &app.state);

        // Fetch latest catalog in the background on startup.
        app.trigger_fetch_catalog(cc.egui_ctx.clone());
        app
    }

    fn trigger_fetch_catalog(&mut self, ctx: egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status_message = "Fetching mod catalog...".to_string();
        let url = self.state.config.catalog_url.clone();
        let data_dir = self.paths.data_dir.clone();
        let tx = self.tx.clone();

        std::thread::spawn(move || {
            let paths = LauncherPaths {
                state_file: data_dir.join("state.json"),
                catalog_cache_file: data_dir.join("catalog_cache.json"),
                staging_root: data_dir.join("mods"),
                data_dir,
            };
            let res = fetch_catalog(&url, &paths);
            let _ = tx.send(WorkerMsg::CatalogFetched(res));
            ctx.request_repaint();
        });
    }

    fn trigger_install_mod(&mut self, entry: ModEntry, ctx: egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status_message = format!("Downloading & installing {}...", entry.name);
        let data_dir = self.paths.data_dir.clone();
        let config = self.state.config.clone();
        let existing = self.state.installed.get(&entry.id).cloned();
        let tx = self.tx.clone();
        let id = entry.id.clone();

        std::thread::spawn(move || {
            let paths = LauncherPaths {
                state_file: data_dir.join("state.json"),
                catalog_cache_file: data_dir.join("catalog_cache.json"),
                staging_root: data_dir.join("mods"),
                data_dir,
            };
            let result = install_mod(&entry, &paths, &config, existing.as_ref());
            let _ = tx.send(WorkerMsg::ModInstalled { id, result });
            ctx.request_repaint();
        });
    }

    fn poll_worker(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.busy = false;
            match msg {
                WorkerMsg::CatalogFetched(Ok(cat)) => {
                    let count = cat.mods.len();
                    self.catalog = cat;
                    self.status_message = format!("Catalog updated ({count} mods).");
                    if self.selected_catalog_id.is_none() {
                        self.selected_catalog_id = self.catalog.mods.first().map(|m| m.id.clone());
                    }
                }
                WorkerMsg::CatalogFetched(Err(err)) => {
                    self.status_message = format!("Catalog fetch failed: {err}");
                }
                WorkerMsg::ModInstalled { id, result } => match result {
                    Ok(installed) => {
                        let name = installed.name.clone();
                        self.state.installed.insert(id, installed);
                        if let Err(e) = save_state(&self.paths, &self.state) {
                            self.status_message = format!("Installed {name}, but failed to save state: {e}");
                        } else {
                            self.status_message = format!("Installed & enabled {name}.");
                        }
                    }
                    Err(err) => {
                        self.status_message = format!("Install failed: {err}");
                    }
                },
            }
        }
    }

    fn toggle_mod_enabled(&mut self, id: &str) {
        let Some(installed) = self.state.installed.get_mut(id) else {
            return;
        };
        let res = if installed.enabled {
            disable_mod(installed).map(|_| "Disabled")
        } else {
            enable_mod(installed, &self.state.config).map(|_| "Enabled")
        };

        match res {
            Ok(action) => {
                let name = installed.name.clone();
                let _ = save_state(&self.paths, &self.state);
                self.status_message = format!("{action} {name}.");
            }
            Err(err) => {
                self.status_message = format!("Error: {err}");
            }
        }
    }

    fn handle_uninstall(&mut self, id: &str) {
        if let Some(mut installed) = self.state.installed.remove(id) {
            match uninstall_mod(&mut installed) {
                Ok(()) => {
                    let _ = save_state(&self.paths, &self.state);
                    self.status_message = format!("Uninstalled {}.", installed.name);
                }
                Err(err) => {
                    self.state.installed.insert(id.to_string(), installed);
                    self.status_message = format!("Uninstall failed: {err}");
                }
            }
        }
    }

    fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("Ages of Conflict Mod Launcher");
                ui.separator();

                ui.selectable_value(&mut self.active_tab, Tab::Browse, "Browse Mods");
                ui.selectable_value(
                    &mut self.active_tab,
                    Tab::Installed,
                    format!("Installed ({})", self.state.installed.len()),
                );
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "Settings");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Launch Game (Steam)").clicked() {
                        let uri = format!("steam://rungameid/{STEAM_APP_ID}");
                        if let Err(e) = open::that(&uri) {
                            self.status_message = format!("Could not launch Steam: {e}");
                        }
                    }
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Refresh Catalog"))
                        .clicked()
                    {
                        self.trigger_fetch_catalog(ctx.clone());
                    }
                });
            });
            ui.add_space(4.0);
        });
    }

    fn draw_status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if self.busy {
                    ui.spinner();
                }
                ui.label(&self.status_message);
            });
        });
    }

    fn draw_browse_tab(&mut self, ctx: &egui::Context) {
        let query = self.search_query.to_lowercase();
        let filtered: Vec<ModEntry> = self
            .catalog
            .mods
            .iter()
            .filter(|m| {
                query.is_empty()
                    || m.name.to_lowercase().contains(&query)
                    || m.summary.to_lowercase().contains(&query)
                    || m.author.to_lowercase().contains(&query)
                    || m.tags.iter().any(|t| t.to_lowercase().contains(&query))
            })
            .cloned()
            .collect();

        egui::SidePanel::left("browse_list")
            .default_width(300.0)
            .show(ctx, |ui| {
                if let Some(ref msg) = self.catalog.announcement {
                    if !msg.trim().is_empty() {
                        ui.colored_label(egui::Color32::LIGHT_BLUE, msg);
                        ui.separator();
                    }
                }

                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.text_edit_singleline(&mut self.search_query);
                });
                ui.separator();

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if filtered.is_empty() {
                        ui.label("No mods found. Check your Catalog URL in Settings.");
                    }
                    for m in &filtered {
                        let selected = self.selected_catalog_id.as_deref() == Some(&m.id);
                        let installed = self.state.installed.get(&m.id);
                        let badge = match installed {
                            Some(inst) if inst.version != m.version => " [Update]",
                            Some(inst) if inst.enabled => " [Enabled]",
                            Some(_) => " [Disabled]",
                            None => "",
                        };
                        let label = format!("{}{}", m.name, badge);
                        if ui.selectable_label(selected, label).clicked() {
                            self.selected_catalog_id = Some(m.id.clone());
                        }
                    }
                });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let selected_mod = self
                .selected_catalog_id
                .as_ref()
                .and_then(|id| self.catalog.mods.iter().find(|m| &m.id == id))
                .cloned();

            let Some(m) = selected_mod else {
                ui.centered_and_justified(|ui| {
                    ui.label("Select a mod from the list to view its details.");
                });
                return;
            };

            ui.heading(&m.name);
            ui.horizontal(|ui| {
                ui.label(format!("v{}", m.version));
                if !m.author.is_empty() {
                    ui.separator();
                    ui.label(format!("by {}", m.author));
                }
                ui.separator();
                ui.label(format!("Target: {}", m.install_target.label()));
            });

            if !m.tags.is_empty() {
                ui.horizontal_wrapped(|ui| {
                    for tag in &m.tags {
                        ui.small(format!("#{tag}"));
                    }
                });
            }

            ui.separator();

            let installed = self.state.installed.get(&m.id).cloned();
            ui.horizontal(|ui| match installed {
                None => {
                    if ui
                        .add_enabled(!self.busy, egui::Button::new("Install Mod"))
                        .clicked()
                    {
                        self.trigger_install_mod(m.clone(), ctx.clone());
                    }
                }
                Some(inst) => {
                    if inst.version != m.version
                        && ui
                            .add_enabled(
                                !self.busy,
                                egui::Button::new(format!("Update to v{}", m.version)),
                            )
                            .clicked()
                    {
                        self.trigger_install_mod(m.clone(), ctx.clone());
                    }

                    let toggle_text = if inst.enabled { "Disable" } else { "Enable" };
                    if ui.button(toggle_text).clicked() {
                        self.toggle_mod_enabled(&m.id);
                    }

                    if ui.button("Uninstall").clicked() {
                        self.handle_uninstall(&m.id);
                    }
                }
            });

            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                if !m.summary.is_empty() {
                    ui.strong(&m.summary);
                    ui.add_space(6.0);
                }
                let desc = if m.description.is_empty() {
                    "No description provided."
                } else {
                    &m.description
                };
                ui.label(desc);
            });
        });
    }

    fn draw_installed_tab(&mut self, ctx: &egui::Context) {
        let installed_ids: Vec<String> = self.state.installed.keys().cloned().collect();

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Installed Mods");
            ui.separator();

            if installed_ids.is_empty() {
                ui.label("No mods installed yet. Go to Browse Mods to install some!");
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for id in installed_ids {
                    let Some(inst) = self.state.installed.get(&id).cloned() else {
                        continue;
                    };
                    ui.group(|ui| {
                        ui.horizontal(|ui| {
                            let mut enabled = inst.enabled;
                            if ui.checkbox(&mut enabled, "").changed() {
                                self.toggle_mod_enabled(&id);
                            }

                            ui.vertical(|ui| {
                                ui.strong(format!("{} (v{})", inst.name, inst.version));
                                ui.small(format!(
                                    "Status: {} | Target: {} | Files: {}",
                                    if inst.enabled { "Enabled" } else { "Disabled" },
                                    inst.install_target.label(),
                                    inst.deployed_files.len()
                                ));
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("Uninstall").clicked() {
                                        self.handle_uninstall(&id);
                                    }
                                    if ui.button("Open Folder").clicked() {
                                        let _ = open::that(&inst.staging_dir);
                                    }
                                },
                            );
                        });
                    });
                    ui.add_space(4.0);
                }
            });
        });
    }

    fn draw_settings_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Launcher Settings");
            ui.separator();

            ui.label("Mod Catalog URL (raw catalog.json URL or local file path):");
            ui.text_edit_singleline(&mut self.catalog_url_input);
            ui.add_space(8.0);

            ui.label("Ages of Conflict Steam Install Directory:");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.game_dir_input);
                if ui.button("Auto-Detect").clicked() {
                    if let Some(p) = detect_game_dir() {
                        self.game_dir_input = p.display().to_string();
                        self.status_message = "Detected game install directory.".to_string();
                    } else {
                        self.status_message = "Could not auto-detect game install directory.".to_string();
                    }
                }
            });
            ui.add_space(8.0);

            ui.label("Ages of Conflict Data Directory (Custom Maps / Custom Scenarios):");
            ui.horizontal(|ui| {
                ui.text_edit_singleline(&mut self.data_dir_input);
                if ui.button("Auto-Detect").clicked() {
                    if let Some(p) = detect_data_dir() {
                        self.data_dir_input = p.display().to_string();
                        self.status_message = "Detected game data directory.".to_string();
                    } else {
                        self.status_message = "Could not auto-detect game data directory.".to_string();
                    }
                }
            });
            ui.add_space(12.0);

            ui.horizontal(|ui| {
                if ui.button("Save Settings").clicked() {
                    self.state.config.catalog_url = self.catalog_url_input.trim().to_string();
                    self.state.config.game_dir = if self.game_dir_input.trim().is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(self.game_dir_input.trim()))
                    };
                    self.state.config.data_dir = if self.data_dir_input.trim().is_empty() {
                        None
                    } else {
                        Some(PathBuf::from(self.data_dir_input.trim()))
                    };
                    match save_state(&self.paths, &self.state) {
                        Ok(()) => self.status_message = "Settings saved.".to_string(),
                        Err(e) => self.status_message = format!("Failed to save settings: {e}"),
                    }
                }

                if ui.button("Open Launcher Data Folder").clicked() {
                    let _ = open::that(&self.paths.data_dir);
                }
            });
        });
    }
}

impl eframe::App for ModLauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker();
        self.draw_top_bar(ctx);
        self.draw_status_bar(ctx);

        match self.active_tab {
            Tab::Browse => self.draw_browse_tab(ctx),
            Tab::Installed => self.draw_installed_tab(ctx),
            Tab::Settings => self.draw_settings_tab(ctx),
        }
    }
}
