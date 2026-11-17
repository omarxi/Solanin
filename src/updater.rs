use serde::Deserialize;
use std::time::Duration;

const RELEASES_LATEST_API: &str = "https://api.github.com/repos/omarxi/Solanin/releases/latest";

#[derive(Debug, Clone)]
pub struct AppUpdateInfo {
    pub version: String,
    pub html_url: String,
    pub appimage_url: Option<String>,
    pub exe_url: Option<String>,
    pub setup_url: Option<String>,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

fn parse_semver(v: &str) -> (u64, u64, u64) {
    let clean = v.trim().trim_start_matches('v');
    let mut parts = clean.split('.').map(|p| {
        p.chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u64>()
            .unwrap_or(0)
    });
    (
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
    )
}

pub fn check_for_update() -> Result<Option<AppUpdateInfo>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("Solanin/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client
        .get(RELEASES_LATEST_API)
        .send()
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        return Ok(None);
    }

    let rel: GhRelease = resp.json().map_err(|e| e.to_string())?;
    let current = parse_semver(env!("CARGO_PKG_VERSION"));
    let latest = parse_semver(&rel.tag_name);

    if latest <= current {
        return Ok(None);
    }

    let mut appimage_url = None;
    let mut exe_url = None;
    let mut setup_url = None;
    for a in rel.assets {
        let lower = a.name.to_lowercase();
        if lower.ends_with(".appimage") {
            appimage_url = Some(a.browser_download_url);
        } else if lower == "solanin-setup.exe" {
            setup_url = Some(a.browser_download_url);
        } else if lower == "solanin.exe" || lower == "solanin-portable.exe" {
            exe_url = Some(a.browser_download_url);
        }
    }

    Ok(Some(AppUpdateInfo {
        version: rel.tag_name,
        html_url: rel.html_url,
        appimage_url,
        exe_url,
        setup_url,
    }))
}

fn download_file(url: &str) -> Result<Vec<u8>, String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(180))
        .user_agent("Solanin/1.0")
        .build()
        .map_err(|e| e.to_string())?;

    let resp = client.get(url).send().map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }
    resp.bytes()
        .map(|b| b.to_vec())
        .map_err(|e| e.to_string())
}

pub fn apply_update_and_restart(info: &AppUpdateInfo) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    {
        if let Ok(appimage) = std::env::var("APPIMAGE") {
            let Some(ref url) = info.appimage_url else {
                let _ = open::that(&info.html_url);
                return Ok(());
            };
            let target = std::path::PathBuf::from(&appimage);
            let tmp = target.with_extension("AppImage.new");
            let bytes = download_file(url)?;
            std::fs::write(&tmp, bytes).map_err(|e| format!("Failed to write update: {e}"))?;

            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = std::fs::metadata(&tmp) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = std::fs::set_permissions(&tmp, perms);
            }

            std::fs::rename(&tmp, &target)
                .map_err(|e| format!("Failed to replace AppImage: {e}"))?;
            std::process::Command::new(&target)
                .spawn()
                .map_err(|e| format!("Failed to restart Solanin: {e}"))?;
            std::process::exit(0);
        } else {
            let _ = open::that(&info.html_url);
            return Ok(());
        }
    }

    #[cfg(target_os = "windows")]
    {
        let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_dir = current_exe
            .parent()
            .ok_or_else(|| "Invalid executable path".to_string())?;
        let is_installed = exe_dir.join("unins000.exe").exists();

        if is_installed {
            if let Some(ref setup_url) = info.setup_url {
                let setup_path = std::env::temp_dir().join("solanin-setup.exe");
                let bytes = download_file(setup_url)?;
                std::fs::write(&setup_path, bytes)
                    .map_err(|e| format!("Failed to save installer: {e}"))?;
                std::process::Command::new(&setup_path)
                    .arg("/SILENT")
                    .spawn()
                    .map_err(|e| format!("Failed to launch installer: {e}"))?;
                std::process::exit(0);
            }
        }

        if let Some(ref exe_url) = info.exe_url {
            let old_exe = current_exe.with_extension("exe.old");
            let _ = std::fs::remove_file(&old_exe);
            let bytes = download_file(exe_url)?;
            std::fs::rename(&current_exe, &old_exe)
                .map_err(|e| format!("Failed to stage update: {e}"))?;
            if let Err(e) = std::fs::write(&current_exe, bytes) {
                let _ = std::fs::rename(&old_exe, &current_exe);
                return Err(format!("Failed to write update: {e}"));
            }
            std::process::Command::new(&current_exe)
                .spawn()
                .map_err(|e| format!("Failed to restart Solanin: {e}"))?;
            std::process::exit(0);
        }

        let _ = open::that(&info.html_url);
        return Ok(());
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = open::that(&info.html_url);
        Ok(())
    }
}

pub fn cleanup_old_exe() {
    #[cfg(target_os = "windows")]
    if let Ok(current_exe) = std::env::current_exe() {
        let old_exe = current_exe.with_extension("exe.old");
        if old_exe.exists() {
            let _ = std::fs::remove_file(old_exe);
        }
    }
}
