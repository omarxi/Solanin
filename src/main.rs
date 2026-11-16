mod app;
mod catalog;
mod mod_manager;
mod models;
mod paths;

use app::ModLauncherApp;
use eframe::egui;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 620.0])
            .with_min_inner_size([720.0, 480.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Ages of Conflict Mod Launcher",
        options,
        Box::new(|cc| Ok(Box::new(ModLauncherApp::new(cc)))),
    )
}
