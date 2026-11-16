use crate::catalog::{fetch_catalog, load_cached_catalog, load_state, save_state};
use crate::mod_manager::{disable_mod, enable_mod, install_mod, uninstall_mod};
use crate::models::{AppState, InstalledMod, ModCatalog, ModEntry, STEAM_APP_ID};
use crate::paths::{detect_game_dir, LauncherPaths};
use eframe::egui::{self, Color32, RichText, Rounding, Stroke};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

const BG_MAIN: Color32 = Color32::from_rgb(17, 19, 24);
const BG_PANEL: Color32 = Color32::from_rgb(23, 26, 33);
const BG_CARD: Color32 = Color32::from_rgb(30, 34, 43);
const BG_CARD_SELECTED: Color32 = Color32::from_rgb(36, 44, 61);
const BORDER_SUBTLE: Color32 = Color32::from_rgb(45, 50, 62);
const TEXT_PRIMARY: Color32 = Color32::from_rgb(234, 237, 243);
const TEXT_MUTED: Color32 = Color32::from_rgb(142, 150, 168);
const ACCENT_BLUE: Color32 = Color32::from_rgb(59, 130, 246);
const ACCENT_GREEN: Color32 = Color32::from_rgb(34, 197, 94);
const ACCENT_AMBER: Color32 = Color32::from_rgb(245, 158, 11);
const ACCENT_RED: Color32 = Color32::from_rgb(220, 38, 38);

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
    ImageLoaded {
        url: String,
        result: Result<egui::ColorImage, String>,
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
    catalog_url_input: String,
    game_dir_input: String,
    preview_textures: HashMap<String, Option<egui::TextureHandle>>,
    loading_images: HashSet<String>,
}

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = BG_MAIN;
    visuals.panel_fill = BG_PANEL;
    visuals.extreme_bg_color = Color32::from_rgb(13, 15, 19);
    visuals.faint_bg_color = BG_CARD;
    visuals.override_text_color = Some(TEXT_PRIMARY);
    visuals.selection.bg_fill = Color32::from_rgb(37, 99, 235);
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT_BLUE);

    visuals.widgets.noninteractive.bg_fill = BG_CARD;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    visuals.widgets.noninteractive.rounding = Rounding::same(6.0);

    visuals.widgets.inactive.bg_fill = Color32::from_rgb(38, 43, 54);
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, BORDER_SUBTLE);
    visuals.widgets.inactive.rounding = Rounding::same(6.0);

    visuals.widgets.hovered.bg_fill = Color32::from_rgb(49, 55, 70);
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(72, 80, 99));
    visuals.widgets.hovered.rounding = Rounding::same(6.0);

    visuals.widgets.active.bg_fill = ACCENT_BLUE;
    visuals.widgets.active.rounding = Rounding::same(6.0);

    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(10.0, 8.0);
        style.spacing.button_padding = egui::vec2(14.0, 7.0);
        style.spacing.window_margin = egui::Margin::same(14.0);
    });
}

fn card_frame(selected: bool) -> egui::Frame {
    egui::Frame::none()
        .fill(if selected { BG_CARD_SELECTED } else { BG_CARD })
        .stroke(Stroke::new(
            1.0_f32,
            if selected { ACCENT_BLUE } else { BORDER_SUBTLE },
        ))
        .rounding(Rounding::same(8.0))
        .inner_margin(egui::Margin::symmetric(12.0, 10.0))
}

fn primary_button(label: &str, fill: Color32) -> egui::Button<'_> {
    egui::Button::new(RichText::new(label).color(Color32::WHITE).strong()).fill(fill)
}

impl ModLauncherApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_theme(&cc.egui_ctx);

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

        let selected_catalog_id = catalog.mods.first().map(|m| m.id.clone());

        let mut app = Self {
            paths,
            state,
            catalog,
            active_tab: Tab::Browse,
            search_query: String::new(),
            selected_catalog_id,
            status_message: "Ready".to_string(),
            busy: false,
            tx,
            rx,
            catalog_url_input,
            game_dir_input,
            preview_textures: HashMap::new(),
            loading_images: HashSet::new(),
        };

        let _ = save_state(&app.paths, &app.state);
        app.trigger_fetch_catalog(cc.egui_ctx.clone());
        app
    }

    fn trigger_fetch_catalog(&mut self, ctx: egui::Context) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.status_message = "Refreshing plugin list...".to_string();
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
        self.status_message = format!("Installing {}...", entry.name);
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

    fn trigger_fetch_image(&mut self, url: String, ctx: egui::Context) {
        if self.loading_images.contains(&url) || self.preview_textures.contains_key(&url) {
            return;
        }
        self.loading_images.insert(url.clone());
        let tx = self.tx.clone();

        std::thread::spawn(move || {
            let result = (|| -> Result<egui::ColorImage, String> {
                let bytes = if let Some(local) = url.strip_prefix("file://") {
                    std::fs::read(local).map_err(|e| e.to_string())?
                } else {
                    let client = reqwest::blocking::Client::builder()
                        .timeout(Duration::from_secs(15))
                        .user_agent("Solanin/0.1")
                        .build()
                        .map_err(|e| e.to_string())?;
                    let resp = client.get(&url).send().map_err(|e| e.to_string())?;
                    if !resp.status().is_success() {
                        return Err(format!("HTTP {}", resp.status()));
                    }
                    resp.bytes().map_err(|e| e.to_string())?.to_vec()
                };
                let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                Ok(egui::ColorImage::from_rgba_unmultiplied(
                    size,
                    rgba.as_flat_samples().as_slice(),
                ))
            })();

            let _ = tx.send(WorkerMsg::ImageLoaded { url, result });
            ctx.request_repaint();
        });
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                WorkerMsg::CatalogFetched(Ok(cat)) => {
                    self.busy = false;
                    let count = cat.mods.len();
                    self.catalog = cat;
                    self.status_message = format!("{count} plugins loaded");
                    if self.selected_catalog_id.is_none() {
                        self.selected_catalog_id = self.catalog.mods.first().map(|m| m.id.clone());
                    }
                }
                WorkerMsg::CatalogFetched(Err(err)) => {
                    self.busy = false;
                    self.status_message = format!("Could not refresh list: {err}");
                }
                WorkerMsg::ModInstalled { id, result } => {
                    self.busy = false;
                    match result {
                        Ok(installed) => {
                            let name = installed.name.clone();
                            self.state.installed.insert(id, installed);
                            let _ = save_state(&self.paths, &self.state);
                            self.status_message = format!("Installed {name}");
                        }
                        Err(err) => {
                            self.status_message = format!("Install failed: {err}");
                        }
                    }
                }
                WorkerMsg::ImageLoaded { url, result } => {
                    self.loading_images.remove(&url);
                    let tex = result.ok().map(|color_img| {
                        ctx.load_texture(&url, color_img, egui::TextureOptions::LINEAR)
                    });
                    self.preview_textures.insert(url, tex);
                }
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
                self.status_message = format!("{action} {name}");
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
                    self.status_message = format!("Uninstalled {}", installed.name);
                }
                Err(err) => {
                    self.state.installed.insert(id.to_string(), installed);
                    self.status_message = format!("Uninstall failed: {err}");
                }
            }
        }
    }

    fn draw_top_bar(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::none()
                    .fill(BG_PANEL)
                    .inner_margin(egui::Margin::symmetric(16.0, 10.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Solanin").size(18.0).strong());
                    ui.label(RichText::new("Ages of Conflict").size(13.0).color(TEXT_MUTED));
                    ui.add_space(14.0);

                    self.nav_tab(ui, Tab::Browse, "Browse");
                    self.nav_tab(
                        ui,
                        Tab::Installed,
                        &format!("Installed ({})", self.state.installed.len()),
                    );
                    self.nav_tab(ui, Tab::Settings, "Settings");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(primary_button("▶  Play", ACCENT_GREEN)).clicked() {
                            let uri = format!("steam://rungameid/{STEAM_APP_ID}");
                            if let Err(e) = open::that(&uri) {
                                self.status_message = format!("Could not launch Steam: {e}");
                            }
                        }
                        if ui
                            .add_enabled(!self.busy, egui::Button::new("Refresh"))
                            .clicked()
                        {
                            self.trigger_fetch_catalog(ctx.clone());
                        }
                    });
                });
            });
    }

    fn nav_tab(&mut self, ui: &mut egui::Ui, tab: Tab, label: &str) {
        let active = self.active_tab == tab;
        let btn = egui::Button::new(
            RichText::new(label)
                .color(if active { Color32::WHITE } else { TEXT_MUTED })
                .strong(),
        )
        .fill(if active {
            ACCENT_BLUE
        } else {
            Color32::TRANSPARENT
        })
        .stroke(Stroke::NONE);

        if ui.add(btn).clicked() {
            self.active_tab = tab;
        }
    }

    fn draw_status_bar(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar")
            .frame(
                egui::Frame::none()
                    .fill(BG_PANEL)
                    .inner_margin(egui::Margin::symmetric(16.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.busy {
                        ui.spinner();
                    }
                    ui.label(RichText::new(&self.status_message).small().color(TEXT_MUTED));
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
            .resizable(false)
            .exact_width(300.0)
            .frame(
                egui::Frame::none()
                    .fill(BG_MAIN)
                    .inner_margin(egui::Margin::same(12.0)),
            )
            .show(ctx, |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.search_query)
                        .hint_text("Search plugins...")
                        .desired_width(f32::INFINITY),
                );
                ui.add_space(6.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if filtered.is_empty() {
                        ui.add_space(16.0);
                        ui.label(RichText::new("No plugins found.").color(TEXT_MUTED));
                        return;
                    }

                    for m in &filtered {
                        let selected = self.selected_catalog_id.as_deref() == Some(&m.id);
                        let installed = self.state.installed.get(&m.id);

                        let resp = card_frame(selected)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&m.name).strong().size(14.0));
                                    ui.with_layout(
                                        egui::Layout::right_to_left(egui::Align::Center),
                                        |ui| match installed {
                                            Some(inst) if inst.version != m.version => {
                                                ui.label(
                                                    RichText::new("Update")
                                                        .small()
                                                        .color(ACCENT_AMBER),
                                                );
                                            }
                                            Some(inst) if inst.enabled => {
                                                ui.label(
                                                    RichText::new("Enabled")
                                                        .small()
                                                        .color(ACCENT_GREEN),
                                                );
                                            }
                                            Some(_) => {
                                                ui.label(
                                                    RichText::new("Disabled")
                                                        .small()
                                                        .color(TEXT_MUTED),
                                                );
                                            }
                                            None => {}
                                        },
                                    );
                                });
                                let meta = if m.author.is_empty() {
                                    format!("v{}", m.version)
                                } else {
                                    format!("v{}  ·  {}", m.version, m.author)
                                };
                                ui.label(RichText::new(meta).small().color(TEXT_MUTED));
                            })
                            .response
                            .interact(egui::Sense::click());

                        if resp.clicked() {
                            self.selected_catalog_id = Some(m.id.clone());
                        }
                        ui.add_space(4.0);
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG_MAIN)
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0)),
            )
            .show(ctx, |ui| {
                let selected_mod = self
                    .selected_catalog_id
                    .as_ref()
                    .and_then(|id| self.catalog.mods.iter().find(|m| &m.id == id))
                    .cloned();

                let Some(m) = selected_mod else {
                    ui.centered_and_justified(|ui| {
                        ui.label(RichText::new("Select a plugin to view details.").color(TEXT_MUTED));
                    });
                    return;
                };

                card_frame(false).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&m.name).size(22.0).strong());
                            let sub = if m.author.is_empty() {
                                format!("v{}  ·  {}", m.version, m.install_target.label())
                            } else {
                                format!(
                                    "v{}  ·  by {}  ·  {}",
                                    m.version,
                                    m.author,
                                    m.install_target.label()
                                )
                            };
                            ui.label(RichText::new(sub).color(TEXT_MUTED));
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let installed = self.state.installed.get(&m.id).cloned();
                            match installed {
                                None => {
                                    if ui
                                        .add_enabled(!self.busy, primary_button("Install", ACCENT_BLUE))
                                        .clicked()
                                    {
                                        self.trigger_install_mod(m.clone(), ctx.clone());
                                    }
                                }
                                Some(inst) => {
                                    if ui.add(primary_button("Uninstall", ACCENT_RED)).clicked() {
                                        self.handle_uninstall(&m.id);
                                    }

                                    let (toggle_lbl, toggle_col) = if inst.enabled {
                                        ("Disable", Color32::from_rgb(55, 62, 78))
                                    } else {
                                        ("Enable", ACCENT_GREEN)
                                    };
                                    if ui.add(primary_button(toggle_lbl, toggle_col)).clicked() {
                                        self.toggle_mod_enabled(&m.id);
                                    }

                                    if inst.version != m.version
                                        && ui
                                            .add_enabled(
                                                !self.busy,
                                                primary_button("Update", ACCENT_AMBER),
                                            )
                                            .clicked()
                                    {
                                        self.trigger_install_mod(m.clone(), ctx.clone());
                                    }
                                }
                            }
                        });
                    });

                    if !m.tags.is_empty() {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            for tag in &m.tags {
                                egui::Frame::none()
                                    .fill(Color32::from_rgb(38, 43, 56))
                                    .rounding(Rounding::same(4.0))
                                    .inner_margin(egui::Margin::symmetric(8.0, 3.0))
                                    .show(ui, |ui| {
                                        ui.label(RichText::new(tag).small().color(TEXT_MUTED));
                                    });
                            }
                        });
                    }
                });

                ui.add_space(12.0);

                egui::ScrollArea::vertical().show(ui, |ui| {
                    if let Some(ref img_url) = m.image_url {
                        let trimmed = img_url.trim();
                        if !trimmed.is_empty() {
                            if let Some(Some(tex)) = self.preview_textures.get(trimmed) {
                                ui.add(
                                    egui::Image::new(tex)
                                        .max_width(ui.available_width().min(520.0))
                                        .max_height(260.0)
                                        .rounding(Rounding::same(8.0)),
                                );
                                ui.add_space(10.0);
                            } else if self.loading_images.contains(trimmed) {
                                ui.label(
                                    RichText::new("Loading preview...")
                                        .small()
                                        .color(TEXT_MUTED),
                                );
                                ui.add_space(8.0);
                            } else if !self.preview_textures.contains_key(trimmed) {
                                self.trigger_fetch_image(trimmed.to_string(), ctx.clone());
                            }
                        }
                    }

                    if !m.summary.is_empty() {
                        ui.label(RichText::new(&m.summary).size(15.0).strong());
                        ui.add_space(8.0);
                    }
                    let desc = if m.description.is_empty() {
                        "No description provided."
                    } else {
                        &m.description
                    };
                    ui.label(RichText::new(desc).size(14.0).color(TEXT_PRIMARY));
                });
            });
    }

    fn draw_installed_tab(&mut self, ctx: &egui::Context) {
        let installed_ids: Vec<String> = self.state.installed.keys().cloned().collect();

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG_MAIN)
                    .inner_margin(egui::Margin::symmetric(24.0, 18.0)),
            )
            .show(ctx, |ui| {
                ui.label(RichText::new("Installed Plugins").size(20.0).strong());
                ui.add_space(8.0);

                if installed_ids.is_empty() {
                    ui.label(
                        RichText::new("No plugins installed yet. Head to Browse to install one.")
                            .color(TEXT_MUTED),
                    );
                    return;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for id in installed_ids {
                        let Some(inst) = self.state.installed.get(&id).cloned() else {
                            continue;
                        };
                        card_frame(false).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(format!("{}  v{}", inst.name, inst.version))
                                            .size(15.0)
                                            .strong(),
                                    );
                                    let status_txt = if inst.enabled {
                                        format!("Enabled  ·  {} files", inst.deployed_files.len())
                                    } else {
                                        "Disabled".to_string()
                                    };
                                    ui.label(RichText::new(status_txt).small().color(TEXT_MUTED));
                                });

                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if ui.add(primary_button("Uninstall", ACCENT_RED)).clicked()
                                        {
                                            self.handle_uninstall(&id);
                                        }
                                        if ui.button("Folder").clicked() {
                                            let _ = open::that(&inst.staging_dir);
                                        }
                                        let (lbl, col) = if inst.enabled {
                                            ("Enabled", ACCENT_GREEN)
                                        } else {
                                            ("Disabled", Color32::from_rgb(55, 62, 78))
                                        };
                                        if ui.add(primary_button(lbl, col)).clicked() {
                                            self.toggle_mod_enabled(&id);
                                        }
                                    },
                                );
                            });
                        });
                        ui.add_space(6.0);
                    }
                });
            });
    }

    fn draw_settings_tab(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(BG_MAIN)
                    .inner_margin(egui::Margin::symmetric(24.0, 18.0)),
            )
            .show(ctx, |ui| {
                ui.set_max_width(640.0);
                ui.label(RichText::new("Settings").size(20.0).strong());
                ui.add_space(10.0);

                card_frame(false).show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.label(RichText::new("Plugin Catalog URL").strong());
                    ui.add(
                        egui::TextEdit::singleline(&mut self.catalog_url_input)
                            .desired_width(f32::INFINITY),
                    );
                    ui.add_space(10.0);

                    ui.label(RichText::new("Ages of Conflict Game Folder").strong());
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.game_dir_input)
                                .desired_width(ui.available_width() - 120.0),
                        );
                        if ui.button("Auto-Detect").clicked() {
                            if let Some(p) = detect_game_dir() {
                                self.game_dir_input = p.display().to_string();
                                self.status_message = "Detected game folder".to_string();
                            } else {
                                self.status_message = "Could not find game folder".to_string();
                            }
                        }
                    });
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        if ui.add(primary_button("Save Settings", ACCENT_BLUE)).clicked() {
                            self.state.config.catalog_url =
                                self.catalog_url_input.trim().to_string();
                            self.state.config.game_dir = if self.game_dir_input.trim().is_empty() {
                                None
                            } else {
                                Some(PathBuf::from(self.game_dir_input.trim()))
                            };
                            match save_state(&self.paths, &self.state) {
                                Ok(()) => self.status_message = "Settings saved".to_string(),
                                Err(e) => {
                                    self.status_message = format!("Failed to save settings: {e}")
                                }
                            }
                        }

                        if ui.button("Open Launcher Data").clicked() {
                            let _ = open::that(&self.paths.data_dir);
                        }
                    });
                });
            });
    }
}

impl eframe::App for ModLauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);
        self.draw_top_bar(ctx);
        self.draw_status_bar(ctx);

        match self.active_tab {
            Tab::Browse => self.draw_browse_tab(ctx),
            Tab::Installed => self.draw_installed_tab(ctx),
            Tab::Settings => self.draw_settings_tab(ctx),
        }
    }
}
