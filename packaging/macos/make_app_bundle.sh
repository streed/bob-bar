#!/usr/bin/env bash
set -euo pipefail

# Create a macOS .app bundle for bob-bar so it runs without opening Terminal.

APP_NAME="Bob Bar"
BINARY_NAME="bob-bar"
ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
BIN_PATH="$ROOT_DIR/target/release/$BINARY_NAME"
DIST_DIR="$ROOT_DIR/dist/macos"
APP_DIR="$DIST_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RESOURCES_DIR="$CONTENTS_DIR/Resources"

echo "==> Packaging $APP_NAME"

if [[ ! -x "$BIN_PATH" ]]; then
  echo "Error: $BIN_PATH not found or not executable."
  echo "Build first: cargo build --release"
  exit 1
fi

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

# Copy Info.plist
cp "$ROOT_DIR/packaging/macos/Info.plist" "$CONTENTS_DIR/Info.plist"

# Copy executable
cp "$BIN_PATH" "$MACOS_DIR/$BINARY_NAME"
chmod +x "$MACOS_DIR/$BINARY_NAME"

echo "==> Created: $APP_DIR"
echo "Open it in Finder or run: open '$APP_DIR'"

