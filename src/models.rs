use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const STEAM_APP_ID: &str = "2186320";

pub const DEFAULT_CATALOG_URL: &str =
    "https://raw.githubusercontent.com/omarxi/solanin/main/catalog.json";

/// Where a plugin's files are placed when enabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InstallTarget {
    #[default]
    #[serde(rename = "bepinex_plugins", alias = "bep_in_ex_plugins")]
    BepInExPlugins,
    GameDir,
}

impl InstallTarget {
    pub fn label(&self) -> String {
        match self {
            InstallTarget::BepInExPlugins => "BepInEx/plugins".to_string(),
            InstallTarget::GameDir => "Game Folder".to_string(),
        }
    }
}

/// Remote plugin entry in `catalog.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    pub download_url: String,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub install_target: InstallTarget,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub updated_at: String,
}

/// Root structure of `catalog.json`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModCatalog {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub announcement: Option<String>,
    #[serde(default)]
    pub mods: Vec<ModEntry>,
}

/// Locally installed plugin record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub install_target: InstallTarget,
    pub enabled: bool,
    pub staging_dir: PathBuf,
    /// Exact file paths currently deployed into the game folder while enabled.
    #[serde(default)]
    pub deployed_files: Vec<PathBuf>,
}

/// User-editable launcher settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LauncherConfig {
    pub catalog_url: String,
    pub game_dir: Option<PathBuf>,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            catalog_url: DEFAULT_CATALOG_URL.to_string(),
            game_dir: None,
        }
    }
}

/// Persisted launcher state (`state.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppState {
    pub config: LauncherConfig,
    pub installed: BTreeMap<String, InstalledMod>,
}
