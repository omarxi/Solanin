# Ages of Conflict Mod Launcher

A cross-platform Rust mod launcher (Linux AppImage + Windows `.exe`) for **Ages of Conflict: World War Simulator** (Steam AppID `2186320`).

## Features
- **Browse & Install Mods**: Fetches a remote `catalog.json` (with offline local cache) and downloads/extracts `.zip` or single-file mods.
- **Enable / Disable Installed Mods**: Mods are stored in the launcher's staging folder. Enabling copies their files into the game's target folder (`Custom Maps`, `Custom Scenarios`, `GameDir`, or `BepInEx/plugins`); disabling removes the deployed files without deleting your downloaded mod.
- **Web Catalog Manager (`web/index.html`)**: Open `web/index.html` in a browser (or host the repo on GitHub Pages) to visually add/edit/delete mods, write descriptions, and download or directly publish `catalog.json` to GitHub.
- **Auto-Detection**: Automatically looks for Steam's `Ages of Conflict` folder and Unity's `JoySparkGames/Ages of Conflict` data folder on Windows, Linux native, and Linux Proton.

## Quick Setup for Your Mod Website
1. Push this repo to GitHub (or create a separate repo for your mods).
2. Edit `DEFAULT_CATALOG_URL` in `src/models.rs` to point to your raw `catalog.json` URL (or set it in the launcher's **Settings** tab).
3. Enable **GitHub Pages** on your repo, then visit `https://<username>.github.io/<repo>/web/` to manage your mods and descriptions from your browser.

## Building Locally
Requires [Rust](https://rustup.rs/):
```bash
cargo run
```

### Linux AppImage
```bash
chmod +x scripts/build-appimage.sh
./scripts/build-appimage.sh
```

### Windows `.exe` (cross-compile from Linux)
Requires `mingw-w64` (`x86_64-w64-mingw32-gcc`):
```bash
chmod +x scripts/build-windows.sh
./scripts/build-windows.sh
```

### Automated Builds (GitHub Actions)
Push a tag like `v0.1.0` or trigger **Actions -> Build AppImage & Windows EXE** manually to download both the `.AppImage` and `.exe` artifacts.
