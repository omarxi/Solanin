use crate::models::{AppState, ModCatalog};
use crate::paths::{detect_game_dir, LauncherPaths};
use std::path::Path;
use std::time::Duration;

pub fn load_state(paths: &LauncherPaths) -> AppState {
    let mut state: AppState = if paths.state_file.exists() {
        std::fs::read_to_string(&paths.state_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    } else {
        AppState::default()
    };

    if state.config.game_dir.is_none() {
        state.config.game_dir = detect_game_dir();
    }

    state
}

pub fn save_state(paths: &LauncherPaths, state: &AppState) -> Result<(), String> {
    let json = serde_json::to_string_pretty(state)
        .map_err(|e| format!("Failed to serialize state: {e}"))?;
    std::fs::write(&paths.state_file, json)
        .map_err(|e| format!("Failed to write state.json: {e}"))
}

pub fn load_cached_catalog(paths: &LauncherPaths) -> Option<ModCatalog> {
    if paths.catalog_cache_file.exists() {
        std::fs::read_to_string(&paths.catalog_cache_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    }
}

pub fn fetch_catalog(url: &str, paths: &LauncherPaths) -> Result<ModCatalog, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("Catalog URL is empty".to_string());
    }

    let body = if let Some(local_path) = trimmed.strip_prefix("file://") {
        std::fs::read_to_string(Path::new(local_path))
            .map_err(|e| format!("Failed to read local catalog file: {e}"))?
    } else if Path::new(trimmed).exists() {
        std::fs::read_to_string(Path::new(trimmed))
            .map_err(|e| format!("Failed to read local catalog file: {e}"))?
    } else {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("Solanin/0.1")
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;

        let resp = client
            .get(trimmed)
            .send()
            .map_err(|e| format!("Failed to fetch catalog: {e}"))?;

        if !resp.status().is_success() {
            return Err(format!("Catalog server returned HTTP {}", resp.status()));
        }

        resp.text()
            .map_err(|e| format!("Failed to read catalog response: {e}"))?
    };

    let catalog: ModCatalog =
        serde_json::from_str(&body).map_err(|e| format!("Invalid catalog JSON: {e}"))?;

    // Cache the valid catalog locally for offline use.
    let _ = std::fs::write(&paths.catalog_cache_file, &body);

    Ok(catalog)
}
