#!/usr/bin/env bash
# Builds Solanin.app for macOS and zips it to target/Solanin-macos.zip
#
# Usage:
#   bash scripts/build-macos.sh            # Universal (Apple Silicon + Intel), recommended
#   bash scripts/build-macos.sh --arm64    # Apple Silicon only (if low on disk space)
#   bash scripts/build-macos.sh --intel    # Intel only
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="${1:-universal}"
case "$MODE" in
  universal|--universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin); ZIP_NAME="Solanin-macos.zip" ;;
  --arm64)               TARGETS=(aarch64-apple-darwin);                     ZIP_NAME="Solanin-macos-arm64.zip" ;;
  --intel)               TARGETS=(x86_64-apple-darwin);                      ZIP_NAME="Solanin-macos-intel.zip" ;;
  *) echo "Unknown option: $MODE (use --arm64, --intel, or no argument)"; exit 1 ;;
esac

# Strip local usernames/paths (e.g. /Users/<name>/.cargo/...) from the compiled binary
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~ --remap-path-prefix=$ROOT=solanin"

BINARIES=()
for t in "${TARGETS[@]}"; do
  rustup target add "$t"
  cargo build --release --target "$t"
  BINARIES+=("$ROOT/target/$t/release/solanin")
done

APP_DIR="$ROOT/target/Solanin.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

if [ "${#BINARIES[@]}" -gt 1 ]; then
  lipo -create "${BINARIES[@]}" -output "$APP_DIR/Contents/MacOS/solanin"
else
  cp "${BINARIES[0]}" "$APP_DIR/Contents/MacOS/solanin"
fi
chmod +x "$APP_DIR/Contents/MacOS/solanin"

# Build macOS .icns icon from assets/icon.png
ICONSET="$ROOT/target/Solanin.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z $size $size "$ROOT/assets/icon.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  if [ $double -le 512 ]; then
    sips -z $double $double "$ROOT/assets/icon.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
  fi
done
iconutil -c icns "$ICONSET" -o "$APP_DIR/Contents/Resources/icon.icns"
rm -rf "$ICONSET"

VERSION="$(grep -m1 '^version' "$ROOT/Cargo.toml" | sed -E 's/.*"(.*)".*/\1/')"

cat > "$APP_DIR/Contents/Info.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Solanin</string>
  <key>CFBundleDisplayName</key>
  <string>Solanin</string>
  <key>CFBundleIdentifier</key>
  <string>com.solanin.launcher</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>CFBundleExecutable</key>
  <string>solanin</string>
  <key>CFBundleIconFile</key>
  <string>icon</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
EOF

# Ad-hoc code sign so Apple Silicon Macs allow launching the binary
codesign --force --deep --sign - "$APP_DIR"

rm -f "$ROOT/target/$ZIP_NAME"
# ditto preserves permissions, symlinks and the code signature
(cd "$ROOT/target" && ditto -c -k --keepParent "Solanin.app" "$ZIP_NAME")

echo ""
echo "Built $ROOT/target/$ZIP_NAME"
lipo -archs "$APP_DIR/Contents/MacOS/solanin" | sed 's/^/Architectures: /'
if [ "$MODE" = "--arm64" ]; then
  echo "WARNING: Apple Silicon only - Intel Macs are NOT supported by this build."
elif [ "$MODE" = "--intel" ]; then
  echo "WARNING: Intel only - Apple Silicon Macs will need Rosetta 2 to run this build."
fi
