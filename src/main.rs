#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod catalog;
mod mod_manager;
mod models;
mod paths;
mod shortcut;
mod updater;

use app::ModLauncherApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    updater::cleanup_old_exe();
    shortcut::ensure_appimage_desktop_shortcut();
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).ok();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([960.0, 620.0])
        .with_min_inner_size([720.0, 480.0]);
    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Solanin",
        options,
        Box::new(|cc| Ok(Box::new(ModLauncherApp::new(cc)))),
    )
}
