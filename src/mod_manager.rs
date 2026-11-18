use crate::models::{AppState, InstallTarget, InstalledMod, LauncherConfig, ModCatalog, ModEntry};
use crate::paths::{resolve_target_dir, LauncherPaths};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Cursor;
use std::path::{Component, Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;
use zip::ZipArchive;

fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn download_bytes(url: &str) -> Result<Vec<u8>, String> {
    if let Some(local) = url.strip_prefix("file://") {
        return std::fs::read(local)
            .map_err(|e| format!("Failed to read local mod file '{local}': {e}"));
    }

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .user_agent("Solanin/0.1")
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let resp = client
        .get(url)
        .send()
        .map_err(|e| format!("Download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("Download failed with HTTP {}", resp.status()));
    }

    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| format!("Failed to read download stream: {e}"))
}

fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<(), String> {
    let expected = expected_hex.trim().to_lowercase();
    if expected.is_empty() {
        return Ok(());
    }
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let actual = hex::encode(hasher.finalize());
    if actual != expected {
        return Err(format!(
            "SHA-256 mismatch: expected {expected}, got {actual}"
        ));
    }
    Ok(())
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

fn extract_zip(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|e| format!("Invalid zip archive: {e}"))?;

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("Failed to read zip entry: {e}"))?;
        let Some(rel_path) = file.enclosed_name().map(|p| p.to_path_buf()) else {
            continue;
        };
        if !is_safe_relative_path(&rel_path) {
            continue;
        }

        let out_path = dest.join(&rel_path);
        if file.is_dir() {
            std::fs::create_dir_all(&out_path)
                .map_err(|e| format!("Failed to create directory: {e}"))?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to create parent directory: {e}"))?;
            }
            let mut out_file = std::fs::File::create(&out_path)
                .map_err(|e| format!("Failed to create extracted file: {e}"))?;
            std::io::copy(&mut file, &mut out_file)
                .map_err(|e| format!("Failed to write extracted file: {e}"))?;
        }
    }

    Ok(())
}

fn filename_from_url(url: &str, fallback_id: &str) -> String {
    let without_query = url.split('?').next().unwrap_or(url);
    without_query
        .rsplit(&['/', '\\'][..])
        .next()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("{fallback_id}.bin"))
}

pub fn install_mod(
    entry: &ModEntry,
    paths: &LauncherPaths,
    config: &LauncherConfig,
    existing: Option<&InstalledMod>,
) -> Result<InstalledMod, String> {
    // If updating/reinstalling an already-enabled mod, remove old deployed files first.
    if let Some(prev) = existing {
        if prev.enabled {
            let mut prev_clone = prev.clone();
            disable_mod(&mut prev_clone)?;
        }
    }

    let bytes = download_bytes(&entry.download_url)?;
    if let Some(ref sha) = entry.sha256 {
        verify_sha256(&bytes, sha)?;
    }

    let safe_id = sanitize_id(&entry.id);
    let staging_dir = paths.staging_root.join(&safe_id);
    if staging_dir.exists() {
        std::fs::remove_dir_all(&staging_dir)
            .map_err(|e| format!("Failed to clean staging dir: {e}"))?;
    }
    std::fs::create_dir_all(&staging_dir)
        .map_err(|e| format!("Failed to create staging dir: {e}"))?;

    let is_zip = bytes.starts_with(b"PK\x03\x04")
        || entry
            .download_url
            .split('?')
            .next()
            .unwrap_or("")
            .to_lowercase()
            .ends_with(".zip");

    if is_zip {
        extract_zip(&bytes, &staging_dir)?;
    } else {
        let fname = filename_from_url(&entry.download_url, &safe_id);
        let dest = staging_dir.join(fname);
        std::fs::write(&dest, &bytes)
            .map_err(|e| format!("Failed to write staged mod file: {e}"))?;
    }

    let mut installed = InstalledMod {
        id: entry.id.clone(),
        name: entry.name.clone(),
        version: entry.version.clone(),
        author: entry.author.clone(),
        description: entry.description.clone(),
        instructions: entry.instructions.clone(),
        install_target: entry.install_target.clone(),
        enabled: false,
        staging_dir,
        deployed_files: Vec::new(),
    };

    enable_mod(&mut installed, config)?;
    Ok(installed)
}

pub fn enable_mod(installed: &mut InstalledMod, config: &LauncherConfig) -> Result<(), String> {
    if installed.enabled {
        return Ok(());
    }

    let target_dir = resolve_target_dir(&installed.install_target, config)?;
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("Failed to create target directory '{}': {e}", target_dir.display()))?;

    let mut deployed: Vec<PathBuf> = Vec::new();

    for entry in WalkDir::new(&installed.staging_dir).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(&installed.staging_dir)
            .map_err(|e| format!("Path strip error: {e}"))?;
        let dest = target_dir.join(rel);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create target subdir: {e}"))?;
        }
        std::fs::copy(entry.path(), &dest).map_err(|e| {
            format!(
                "Failed to copy '{}' to '{}': {e}",
                entry.path().display(),
                dest.display()
            )
        })?;
        deployed.push(dest);
    }

    installed.deployed_files = deployed;
    installed.enabled = true;
    Ok(())
}

pub fn disable_mod(installed: &mut InstalledMod) -> Result<(), String> {
    for path in &installed.deployed_files {
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|e| format!("Failed to remove '{}': {e}", path.display()))?;
        }
    }
    installed.deployed_files.clear();
    installed.enabled = false;
    Ok(())
}

const BEPINEX_LINUX_URL: &str =
    "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.6/BepInEx_linux_x64_5.4.23.6.zip";
const BEPINEX_WIN_URL: &str =
    "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.6/BepInEx_win_x64_5.4.23.6.zip";

pub fn is_bepinex_installed(game_dir: &Path) -> bool {
    game_dir
        .join("BepInEx")
        .join("core")
        .join("BepInEx.dll")
        .exists()
}

pub fn install_bepinex(game_dir: &Path) -> Result<(), String> {
    if !game_dir.exists() {
        return Err(format!("Game folder does not exist: {}", game_dir.display()));
    }

    let is_linux_native = game_dir.join("Ages of Conflict.x86_64").exists();
    let url = if is_linux_native {
        BEPINEX_LINUX_URL
    } else {
        BEPINEX_WIN_URL
    };

    let bytes = download_bytes(url)?;
    extract_zip(&bytes, game_dir)?;

    let plugins_dir = game_dir.join("BepInEx").join("plugins");
    std::fs::create_dir_all(&plugins_dir)
        .map_err(|e| format!("Failed to create BepInEx/plugins: {e}"))?;

    // Unity 6 (6000.x) destroys BepInEx_Manager on first scene load unless HideManagerGameObject = true.
    let config_dir = game_dir.join("BepInEx").join("config");
    std::fs::create_dir_all(&config_dir)
        .map_err(|e| format!("Failed to create BepInEx/config: {e}"))?;
    let bepinex_cfg = config_dir.join("BepInEx.cfg");
    if bepinex_cfg.exists() {
        if let Ok(content) = std::fs::read_to_string(&bepinex_cfg) {
            let updated = content.replace("HideManagerGameObject = false", "HideManagerGameObject = true");
            let _ = std::fs::write(&bepinex_cfg, updated);
        }
    } else {
        let _ = std::fs::write(&bepinex_cfg, "[Chainloader]\nHideManagerGameObject = true\n");
    }

    if is_linux_native {
        let run_sh = game_dir.join("run_bepinex.sh");
        if run_sh.exists() {
            if let Ok(content) = std::fs::read_to_string(&run_sh) {
                let updated = content.replace(
                    "executable_name=\"\"",
                    "executable_name=\"Ages of Conflict.x86_64\"",
                );
                let _ = std::fs::write(&run_sh, updated);
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&run_sh) {
                    let mut perms = meta.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&run_sh, perms);
                }
            }
        }
    }

    Ok(())
}

pub fn uninstall_mod(installed: &mut InstalledMod) -> Result<(), String> {
    disable_mod(installed)?;
    if installed.staging_dir.exists() {
        std::fs::remove_dir_all(&installed.staging_dir)
            .map_err(|e| format!("Failed to delete staged mod files: {e}"))?;
    }
    Ok(())
}

/// Scans `BepInEx/plugins` for existing `.dll` files not yet tracked by Solanin,
/// matching them to catalog entries by filename when possible.
pub fn import_untracked_plugins(
    paths: &LauncherPaths,
    state: &mut AppState,
    catalog: &ModCatalog,
) -> usize {
    let Some(ref game_dir) = state.config.game_dir else {
        return 0;
    };
    let plugins_dir = game_dir.join("BepInEx").join("plugins");
    if !plugins_dir.exists() {
        return 0;
    }

    let tracked: HashSet<PathBuf> = state
        .installed
        .values()
        .flat_map(|m| m.deployed_files.iter().cloned())
        .collect();

    let Ok(entries) = std::fs::read_dir(&plugins_dir) else {
        return 0;
    };

    let mut imported = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let is_dll = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("dll"))
            .unwrap_or(false);
        if !is_dll || tracked.contains(&path) {
            continue;
        }

        let Some(fname) = path.file_name().and_then(|s| s.to_str()).map(str::to_string) else {
            continue;
        };
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("plugin")
            .to_string();

        // Try matching by filename against catalog entries (handling URL-encoded %20)
        let matched = catalog.mods.iter().find(|m| {
            let cat_fname = filename_from_url(&m.download_url, &m.id).replace("%20", " ");
            !state.installed.contains_key(&m.id) && cat_fname.eq_ignore_ascii_case(&fname)
        });

        let (id, name, version, author, description, instructions, install_target) =
            if let Some(m) = matched {
                (
                    m.id.clone(),
                    m.name.clone(),
                    m.version.clone(),
                    m.author.clone(),
                    m.description.clone(),
                    m.instructions.clone(),
                    m.install_target.clone(),
                )
            } else {
                let local_id = format!("local-{}", sanitize_id(&stem).to_lowercase());
                if state.installed.contains_key(&local_id) {
                    continue;
                }
                (
                    local_id,
                    stem,
                    "local".to_string(),
                    "Local".to_string(),
                    String::new(),
                    String::new(),
                    InstallTarget::BepInExPlugins,
                )
            };

        let staging_dir = paths.staging_root.join(sanitize_id(&id));
        if std::fs::create_dir_all(&staging_dir).is_err() {
            continue;
        }
        let staged_file = staging_dir.join(&fname);
        if std::fs::copy(&path, &staged_file).is_err() {
            continue;
        }

        state.installed.insert(
            id.clone(),
            InstalledMod {
                id,
                name,
                version,
                author,
                description,
                instructions,
                install_target,
                enabled: true,
                staging_dir,
                deployed_files: vec![path],
            },
        );
        imported += 1;
    }

    imported
}

fn normalize_match_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Finds a catalog entry matching an unlinked (e.g. `local-*`) installed mod,
/// preferring official (`!user_submitted`) entries so users can switch to its updates.
pub fn find_catalog_match<'a>(
    inst: &InstalledMod,
    catalog: &'a ModCatalog,
) -> Option<&'a ModEntry> {
    if catalog.mods.iter().any(|m| m.id == inst.id) {
        return None;
    }

    let mut local_files: Vec<String> = inst
        .deployed_files
        .iter()
        .filter_map(|p| p.file_name().and_then(|s| s.to_str()).map(str::to_string))
        .collect();
    if local_files.is_empty() {
        if let Ok(entries) = std::fs::read_dir(&inst.staging_dir) {
            for e in entries.flatten() {
                if let Some(name) = e.file_name().to_str() {
                    local_files.push(name.to_string());
                }
            }
        }
    }

    let inst_name_norm = normalize_match_key(&inst.name);
    let matches_entry = |m: &ModEntry| -> bool {
        let cat_fname = filename_from_url(&m.download_url, &m.id).replace("%20", " ");
        if local_files.iter().any(|f| f.eq_ignore_ascii_case(&cat_fname)) {
            return true;
        }
        let cat_name_norm = normalize_match_key(&m.name);
        let cat_id_norm = normalize_match_key(&m.id);
        if !inst_name_norm.is_empty()
            && (inst_name_norm == cat_name_norm || inst_name_norm == cat_id_norm)
        {
            return true;
        }
        local_files.iter().any(|f| {
            let stem = Path::new(f)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("");
            let stem_norm = normalize_match_key(stem);
            !stem_norm.is_empty() && (stem_norm == cat_name_norm || stem_norm == cat_id_norm)
        })
    };

    catalog
        .mods
        .iter()
        .find(|m| !m.user_submitted && matches_entry(m))
        .or_else(|| catalog.mods.iter().find(|m| matches_entry(m)))
}
