<p align="center">
  <img src="assets/icon.png" width="96" alt="Solanin Logo" />
</p>

<h1 align="center">Solanin</h1>

<p align="center">
  A lightweight, cross-platform plugin launcher for <strong>Ages of Conflict: World War Simulator</strong>, built in Rust (Linux AppImage &amp; Windows <code>.exe</code>).
</p>

<p align="center">
  <a href="https://omarxi.github.io/Solanin/"><strong>Website &amp; Mod Browser</strong></a> ·
  <a href="https://github.com/omarxi/Solanin/releases/latest"><strong>Latest Release</strong></a>
</p>

---

## Preview (v1.0.0)
> **Note:** This is **v1.0.0** — more mods are going to be added!

![Browse Plugins](assets/preview-browse.webp)

![Installed Plugins](assets/preview-installed.webp)

## Features
- **Automatic BepInEx 5 Setup**: Checks for `BepInEx` on startup and automatically installs the right build for Windows or Linux (`5.4.23.6`) into your Steam game folder.
- **Steam Auto-Detection**: Reads Steam's `libraryfolders.vdf` and `appmanifest_2186320.acf` to locate *Ages of Conflict World War Simulator* automatically.
- **Browse & Install Plugins**: Fetches `catalog.json` (with offline caching), previews plugin images and descriptions, and installs `.zip` or `.dll` plugins into `BepInEx/plugins`.
- **Enable / Disable Anytime**: Toggling a plugin off removes its files from `BepInEx/plugins` while keeping them staged locally for instant re-enabling.

## Download
Visit the **[Solanin Website](https://omarxi.github.io/Solanin/)** or grab the latest **`solanin-setup.exe`** / **`solanin-portable.exe`** (Windows), **`solanin-x86_64.AppImage`** (Linux), or **`Solanin-macos.zip`** (macOS, Apple Silicon & Intel) from [Releases](https://github.com/omarxi/Solanin/releases).

> **macOS note:** Solanin isn't notarized by Apple, so the first time you open it, right-click `Solanin.app` → **Open** (or run `xattr -cr Solanin.app`). Launch the game with Solanin's **Play** button, or set the game's Steam Launch Options to `"./run_bepinex.sh" %command%` so BepInEx loads.

## Building from Source
Requires [Rust](https://rustup.rs/):
```bash
cargo run --release
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

### macOS `.app` (on a Mac)
Requires Xcode Command Line Tools (`xcode-select --install`):
```bash
bash scripts/build-macos.sh          # Universal (Apple Silicon + Intel)
bash scripts/build-macos.sh --arm64  # Apple Silicon only
```
