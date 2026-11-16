#[cfg(target_os = "linux")]
use crate::models::STEAM_APP_ID;
use crate::models::{InstallTarget, LauncherConfig};
use directories::{BaseDirs, ProjectDirs};
use std::path::PathBuf;

pub struct LauncherPaths {
    pub data_dir: PathBuf,
    pub state_file: PathBuf,
    pub catalog_cache_file: PathBuf,
    pub staging_root: PathBuf,
}

impl LauncherPaths {
    pub fn resolve() -> Result<Self, String> {
        let proj = ProjectDirs::from("com", "AgesOfConflict", "ModLauncher")
            .ok_or_else(|| "Could not determine application data directory".to_string())?;
        let data_dir = proj.data_local_dir().to_path_buf();
        let staging_root = data_dir.join("mods");
        std::fs::create_dir_all(&staging_root)
            .map_err(|e| format!("Failed to create launcher directory: {e}"))?;

        Ok(Self {
            state_file: data_dir.join("state.json"),
            catalog_cache_file: data_dir.join("catalog_cache.json"),
            staging_root,
            data_dir,
        })
    }
}

/// Attempts to locate the Steam installation folder for Ages of Conflict.
pub fn detect_game_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(pf86).join("Steam/steamapps/common/Ages of Conflict"));
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            candidates.push(PathBuf::from(pf).join("Steam/steamapps/common/Ages of Conflict"));
        }
        for drive in ["C:", "D:", "E:", "F:"] {
            candidates.push(PathBuf::from(format!(
                "{drive}\\SteamLibrary\\steamapps\\common\\Ages of Conflict"
            )));
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(base) = BaseDirs::new() {
        let home = base.home_dir();
        candidates.push(home.join(".local/share/Steam/steamapps/common/Ages of Conflict"));
        candidates.push(home.join(".steam/steam/steamapps/common/Ages of Conflict"));
        candidates.push(
            home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common/Ages of Conflict"),
        );
    }

    candidates.into_iter().find(|p| p.exists())
}

/// Attempts to locate the Unity LocalLow/unity3d folder for JoySparkGames/Ages of Conflict.
pub fn detect_data_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(target_os = "windows")]
    if let Some(base) = BaseDirs::new() {
        if let Some(local_app_data) = base.data_local_dir().parent() {
            candidates.push(
                local_app_data
                    .join("LocalLow")
                    .join("JoySparkGames")
                    .join("Ages of Conflict"),
            );
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(base) = BaseDirs::new() {
        let home = base.home_dir();
        // Native Unity path
        candidates.push(home.join(".config/unity3d/JoySparkGames/Ages of Conflict"));
        // Steam Proton compatdata path
        candidates.push(home.join(format!(
            ".local/share/Steam/steamapps/compatdata/{STEAM_APP_ID}/pfx/drive_c/users/steamuser/AppData/LocalLow/JoySparkGames/Ages of Conflict"
        )));
        candidates.push(home.join(format!(
            ".var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/compatdata/{STEAM_APP_ID}/pfx/drive_c/users/steamuser/AppData/LocalLow/JoySparkGames/Ages of Conflict"
        )));
    }

    candidates.into_iter().find(|p| p.exists())
}

/// Resolves the destination directory for a given `InstallTarget`.
pub fn resolve_target_dir(
    target: &InstallTarget,
    config: &LauncherConfig,
) -> Result<PathBuf, String> {
    match target {
        InstallTarget::CustomMaps => {
            let data = config
                .data_dir
                .as_ref()
                .ok_or_else(|| "Game Data Directory is not configured (check Settings)".to_string())?;
            Ok(data.join("Custom Maps"))
        }
        InstallTarget::CustomScenarios => {
            let data = config
                .data_dir
                .as_ref()
                .ok_or_else(|| "Game Data Directory is not configured (check Settings)".to_string())?;
            Ok(data.join("Custom Scenarios"))
        }
        InstallTarget::GameDir => {
            let game = config
                .game_dir
                .as_ref()
                .ok_or_else(|| "Game Install Directory is not configured (check Settings)".to_string())?;
            Ok(game.clone())
        }
        InstallTarget::BepInExPlugins => {
            let game = config
                .game_dir
                .as_ref()
                .ok_or_else(|| "Game Install Directory is not configured (check Settings)".to_string())?;
            Ok(game.join("BepInEx").join("plugins"))
        }
        InstallTarget::RelativeToGame(rel) => {
            let game = config
                .game_dir
                .as_ref()
                .ok_or_else(|| "Game Install Directory is not configured (check Settings)".to_string())?;
            Ok(game.join(rel))
        }
    }
}
