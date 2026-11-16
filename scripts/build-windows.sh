#!/usr/bin/env bash
set -euo pipefail

TARGET="x86_64-pc-windows-gnu"
rustup target add "$TARGET"
cargo build --release --target "$TARGET"

echo "Built target/${TARGET}/release/ages-of-conflict-mod-launcher.exe"
