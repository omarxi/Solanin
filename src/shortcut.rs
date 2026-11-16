#[cfg(target_os = "linux")]
pub fn ensure_appimage_desktop_shortcut() {
    let Ok(appimage_path) = std::env::var("APPIMAGE") else {
        return;
    };
    let appimage_path = appimage_path.trim();
    if appimage_path.is_empty() {
        return;
    }

    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            directories::BaseDirs::new().map(|b| b.home_dir().join(".local").join("share"))
        });

    let Some(data_home) = data_home else {
        return;
    };

    let apps_dir = data_home.join("applications");
    let icons_dir = data_home
        .join("icons")
        .join("hicolor")
        .join("256x256")
        .join("apps");

    let _ = std::fs::create_dir_all(&apps_dir);
    let _ = std::fs::create_dir_all(&icons_dir);

    let icon_path = icons_dir.join("solanin.png");
    if !icon_path.exists() {
        let _ = std::fs::write(&icon_path, include_bytes!("../assets/icon.png"));
    }

    let escaped_exec = appimage_path.replace('"', "\\\"");
    let desktop_content = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=Solanin\n\
         Comment=Plugin launcher for Ages of Conflict\n\
         Exec=\"{escaped_exec}\" %U\n\
         TryExec={appimage_path}\n\
         Icon=solanin\n\
         Categories=Game;\n\
         Terminal=false\n\
         StartupWMClass=Solanin\n"
    );

    let desktop_file = apps_dir.join("solanin.desktop");
    let needs_write = std::fs::read_to_string(&desktop_file)
        .map(|existing| existing != desktop_content)
        .unwrap_or(true);

    if needs_write {
        if std::fs::write(&desktop_file, desktop_content).is_ok() {
            let _ = std::process::Command::new("update-desktop-database")
                .arg(&apps_dir)
                .output();
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub fn ensure_appimage_desktop_shortcut() {}
