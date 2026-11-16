#!/usr/bin/env bash
set -euo pipefail

APP_NAME="ages-of-conflict-mod-launcher"
APPDIR="target/AppDir"

cargo build --release

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"

cp "target/release/$APP_NAME" "$APPDIR/usr/bin/$APP_NAME"
cp "assets/$APP_NAME.desktop" "$APPDIR/$APP_NAME.desktop"
cp "assets/$APP_NAME.desktop" "$APPDIR/usr/share/applications/$APP_NAME.desktop"
cp "assets/icon.svg" "$APPDIR/$APP_NAME.svg"
cp "assets/icon.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APP_NAME.svg"

cat > "$APPDIR/AppRun" <<'EOF'
#!/usr/bin/env bash
HERE="$(dirname "$(readlink -f "${0}")")"
exec "$HERE/usr/bin/ages-of-conflict-mod-launcher" "$@"
EOF
chmod +x "$APPDIR/AppRun"

if ! command -v appimagetool >/dev/null 2>&1; then
  echo "appimagetool not found in PATH. Download it from https://github.com/AppImage/appimagetool/releases"
  exit 1
fi

ARCH=x86_64 appimagetool "$APPDIR" "target/${APP_NAME}-x86_64.AppImage"
echo "Built target/${APP_NAME}-x86_64.AppImage"
