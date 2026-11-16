use crate::models::{InstallTarget, LauncherConfig, STEAM_APP_ID};
use directories::{BaseDirs, ProjectDirs};
use std::path::{Path, PathBuf};

pub struct LauncherPaths {
    pub data_dir: PathBuf,
    pub state_file: PathBuf,
    pub catalog_cache_file: PathBuf,
    pub staging_root: PathBuf,
}

impl LauncherPaths {
    pub fn resolve() -> Result<Self, String> {
        let proj = ProjectDirs::from("com", "Solanin", "Solanin")
            .ok_or_else(|| "Could not determine application data directory".to_string())?;
        let data_dir = proj.data_local_dir().to_path_buf();
        let staging_root = data_dir.join("plugins");
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

/// Extracts the quoted value for a VDF key like `"path"  "/some/dir"` or `"installdir"  "Name"`.
fn extract_vdf_values(content: &str, key: &str) -> Vec<String> {
    let mut results = Vec::new();
    let needle = format!("\"{key}\"");
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix(&needle) {
            let rest = rest.trim();
            if let Some(inner) = rest.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
                let unescaped = inner.replace("\\\\", "\\");
                if !unescaped.is_empty() {
                    results.push(unescaped);
                }
            }
        }
    }
    results
}

fn find_in_steam_library(lib_root: &Path) -> Option<PathBuf> {
    let steamapps = lib_root.join("steamapps");
    let manifest = steamapps.join(format!("appmanifest_{STEAM_APP_ID}.acf"));
    if let Ok(content) = std::fs::read_to_string(&manifest) {
        for install_dir in extract_vdf_values(&content, "installdir") {
            let candidate = steamapps.join("common").join(install_dir);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    for folder in [
        "Ages of Conflict World War Simulator",
        "Ages of Conflict",
    ] {
        let candidate = steamapps.join("common").join(folder);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// Attempts to locate the Steam installation folder for Ages of Conflict.
pub fn detect_game_dir() -> Option<PathBuf> {
    let mut steam_roots = Vec::new();

    #[cfg(target_os = "windows")]
    {
        if let Ok(pf86) = std::env::var("ProgramFiles(x86)") {
            steam_roots.push(PathBuf::from(pf86).join("Steam"));
        }
        if let Ok(pf) = std::env::var("ProgramFiles") {
            steam_roots.push(PathBuf::from(pf).join("Steam"));
        }
        for drive in ["C:", "D:", "E:", "F:"] {
            steam_roots.push(PathBuf::from(format!("{drive}\\SteamLibrary")));
            steam_roots.push(PathBuf::from(format!("{drive}\\Program Files (x86)\\Steam")));
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(base) = BaseDirs::new() {
        let home = base.home_dir();
        steam_roots.push(home.join(".local/share/Steam"));
        steam_roots.push(home.join(".steam/steam"));
        steam_roots.push(home.join(".steam/root"));
        steam_roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
    }

    let mut libraries = Vec::new();
    for root in steam_roots {
        if !root.exists() {
            continue;
        }
        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        if let Ok(content) = std::fs::read_to_string(&vdf) {
            for lib_path in extract_vdf_values(&content, "path") {
                libraries.push(PathBuf::from(lib_path));
            }
        }
        libraries.push(root);
    }

    for lib in libraries {
        if let Some(found) = find_in_steam_library(&lib) {
            return Some(found);
        }
    }

    None
}

/// Resolves the destination directory for a given `InstallTarget`.
pub fn resolve_target_dir(
    target: &InstallTarget,
    config: &LauncherConfig,
) -> Result<PathBuf, String> {
    let game = config
        .game_dir
        .as_ref()
        .ok_or_else(|| "Ages of Conflict folder is not set (check Settings)".to_string())?;

    match target {
        InstallTarget::BepInExPlugins => Ok(game.join("BepInEx").join("plugins")),
        InstallTarget::GameDir => Ok(game.clone()),
    }
}
