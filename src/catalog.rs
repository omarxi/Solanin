use crate::models::{AppState, InstallTarget, ModCatalog, ModEntry, ReleaseNote};
use crate::paths::{detect_game_dir, LauncherPaths};
use serde::Deserialize;
use std::path::Path;
use std::time::Duration;

#[derive(Deserialize)]
struct CommunityModRow {
    id: String,
    name: String,
    version: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    instructions: String,
    download_url: String,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    previews: Vec<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    virustotal_url: Option<String>,
    #[serde(default)]
    changelog: Vec<ReleaseNote>,
}

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

    let mut catalog: ModCatalog =
        serde_json::from_str(&body).map_err(|e| format!("Invalid catalog JSON: {e}"))?;

    // Cache the base catalog locally for offline use.
    let _ = std::fs::write(&paths.catalog_cache_file, &body);

    // Fetch optional user-submitted community mods from Supabase if configured.
    if let (Some(sb_url), Some(sb_key)) = (&catalog.supabase_url, &catalog.supabase_anon_key) {
        let sb_url = sb_url.trim().trim_end_matches('/');
        let sb_key = sb_key.trim();
        if !sb_url.is_empty() && !sb_key.is_empty() {
            if let Ok(client) = reqwest::blocking::Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("Solanin/1.0")
                .build()
            {
                let endpoint = format!(
                    "{sb_url}/rest/v1/community_mods?select=*&order=created_at.desc"
                );
                if let Ok(resp) = client
                    .get(&endpoint)
                    .header("apikey", sb_key)
                    .header("Authorization", format!("Bearer {sb_key}"))
                    .send()
                {
                    if resp.status().is_success() {
                        if let Ok(rows) = resp.json::<Vec<CommunityModRow>>() {
                            for r in rows {
                                if !catalog.mods.iter().any(|m| m.id == r.id) {
                                    catalog.mods.push(ModEntry {
                                        id: r.id,
                                        name: r.name,
                                        version: r.version,
                                        author: r.author,
                                        summary: r.summary,
                                        description: r.description,
                                        instructions: r.instructions,
                                        download_url: r.download_url,
                                        icon_url: r.icon_url,
                                        previews: r.previews,
                                        image_url: None,
                                        video_url: None,
                                        sha256: r.sha256,
                                        install_target: InstallTarget::BepInExPlugins,
                                        tags: vec!["community".to_string()],
                                        updated_at: String::new(),
                                        changelog: r.changelog,
                                        user_submitted: true,
                                        virustotal_url: r.virustotal_url,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(catalog)
}
