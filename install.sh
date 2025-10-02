#!/bin/bash
set -e

# bob-bar installer script
# This script builds bob-bar from source for your current machine

REPO="streed/bob-bar"
INSTALL_DIR="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/bob-bar"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "╔════════════════════════════════════════╗"
echo "║        bob-bar Installer               ║"
echo "╚════════════════════════════════════════╝"
echo ""

# Detect OS
OS_NAME=$(uname -s)
case "$OS_NAME" in
    Linux)
        PLATFORM="linux"
        ;;
    Darwin)
        PLATFORM="macos"
        ;;
    *)
        echo -e "${RED}Error: Unsupported OS: $OS_NAME${NC}"
        echo "Supported OS: Linux, macOS"
        exit 1
        ;;
esac

ARCH=$(uname -m)
echo -e "${GREEN}✓${NC} Detected platform: $PLATFORM, architecture: $ARCH"

# Check for required commands
if ! command -v git >/dev/null 2>&1; then
    echo -e "${RED}Error: git is required but not installed${NC}"
    echo "Install git and re-run."
    exit 1
fi
echo -e "${GREEN}✓${NC} Found git"

if ! command -v cargo >/dev/null 2>&1; then
    echo -e "${RED}Error: Rust/cargo is required but not installed${NC}"
    echo "Install Rust from https://rustup.rs and re-run."
    exit 1
fi
echo -e "${GREEN}✓${NC} Found cargo"

# Helper: create a simple default PNG icon if no custom icon is provided
ensure_default_icon() {
    DEFAULT_ICON_PATH="$CONFIG_DIR/bob-bar-default.png"
    if [ -f "$DEFAULT_ICON_PATH" ]; then
        echo "$DEFAULT_ICON_PATH"
        return 0
    fi
    mkdir -p "$CONFIG_DIR"
    # 1x1 transparent PNG base64 (will be resized later). Acts as a safe placeholder.
    ICON_B64="iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR4nGNgYAAAAAMAASsJTYQAAAAASUVORK5CYII="
    if command -v base64 >/dev/null 2>&1; then
        # Try GNU (-d), then macOS (-D)
        (echo "$ICON_B64" | base64 -d > "$DEFAULT_ICON_PATH" 2>/dev/null) \
          || (echo "$ICON_B64" | base64 -D > "$DEFAULT_ICON_PATH" 2>/dev/null) \
          || true
    fi
    if [ -f "$DEFAULT_ICON_PATH" ]; then
        echo "$DEFAULT_ICON_PATH"
    else
        echo ""  # Failed to create; caller will skip icon
    fi
}

# Helper: convert an SVG icon to PNG (best-effort), echo resulting PNG path or empty
convert_svg_to_png() {
    local svg_path="$1"
    local out_png="$2"
    local size="512"
    if command -v rsvg-convert >/dev/null 2>&1; then
        rsvg-convert -w "$size" -h "$size" -o "$out_png" "$svg_path" && echo "$out_png" && return 0
    fi
    if command -v inkscape >/dev/null 2>&1; then
        inkscape "$svg_path" --export-type=png --export-filename="$out_png" -w "$size" -h "$size" >/dev/null 2>&1 && echo "$out_png" && return 0
    fi
    if command -v convert >/dev/null 2>&1; then
        convert -background none -density 384 "$svg_path" -resize ${size}x${size} "$out_png" >/dev/null 2>&1 && echo "$out_png" && return 0
    fi
    echo ""
}

# Helper: find or generate an icon PNG path to use
resolve_icon_source() {
    # 1) Explicit env var
    if [ -n "${BOB_BAR_ICON:-}" ] && [ -f "$BOB_BAR_ICON" ]; then
        echo "$BOB_BAR_ICON"; return 0
    fi

    # 2) Repo PNGs
    for base in "${LOCAL_BUILD_DIR:-.}" .; do
        for p in \
            "$base/packaging/icons/bob-bar.png" \
            "$base/assets/icon.png"; do
            if [ -f "$p" ]; then echo "$p"; return 0; fi
        done
    done

    # 3) Repo SVGs -> convert to PNG
    TMP_PNG="$(mktemp).png"
    for base in "${LOCAL_BUILD_DIR:-.}" .; do
        for s in \
            "$base/packaging/icons/bob-bar.svg" \
            "$base/assets/icon.svg"; do
            if [ -f "$s" ]; then
                cvt="$(convert_svg_to_png "$s" "$TMP_PNG")"
                if [ -n "$cvt" ] && [ -f "$TMP_PNG" ]; then echo "$TMP_PNG"; return 0; fi
            fi
        done
    done

    # 4) Fallback placeholder
    echo "$(ensure_default_icon)"
}

# Create GUI launchers/helpers per platform
setup_macos_app_bundle() {
    echo ""
    echo "Setting up macOS app bundle..."
    APP_NAME="Bob Bar"
    APP_DIR="$HOME/Applications/$APP_NAME.app"
    CONTENTS_DIR="$APP_DIR/Contents"
    MACOS_DIR="$CONTENTS_DIR/MacOS"
    RESOURCES_DIR="$CONTENTS_DIR/Resources"

    mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"

    # Write Info.plist
    cat > "$CONTENTS_DIR/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>bob-bar</string>
    <key>CFBundleIconFile</key>
    <string>app.icns</string>
    <key>CFBundleIdentifier</key>
    <string>com.mudflap.bob-bar</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Bob Bar</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1.0.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
EOF

    # Copy executable into bundle
    cp "$INSTALL_DIR/bob-bar" "$MACOS_DIR/bob-bar"
    chmod +x "$MACOS_DIR/bob-bar"

    # Determine icon source (custom, repo PNG/SVG, or placeholder)
    ICON_SRC="$(resolve_icon_source)"

    if [ -n "$ICON_SRC" ] && command -v sips >/dev/null 2>&1 && command -v iconutil >/dev/null 2>&1; then
      TMP_ICONSET="$(mktemp -d)"/AppIcon.iconset
      mkdir -p "$TMP_ICONSET"
      # Generate required sizes from source
      for size in 16 32 64 128 256 512; do
        sips -z $size $size "$ICON_SRC" --out "$TMP_ICONSET/icon_${size}x${size}.png" >/dev/null 2>&1 || true
        dbl=$((size*2))
        sips -z $dbl $dbl "$ICON_SRC" --out "$TMP_ICONSET/icon_${size}x${size}@2x.png" >/dev/null 2>&1 || true
      done
      iconutil -c icns "$TMP_ICONSET" -o "$RESOURCES_DIR/app.icns" >/dev/null 2>&1 || true
      rm -rf "$(dirname "$TMP_ICONSET")"
      if [ -f "$RESOURCES_DIR/app.icns" ]; then
        echo -e "${GREEN}✓${NC} Added macOS app icon from: $ICON_SRC"
      else
        echo -e "${YELLOW}⚠${NC}  Could not generate .icns; continuing without custom icon"
      fi
    else
      if [ -n "$ICON_SRC" ]; then
        echo -e "${YELLOW}⚠${NC}  'sips' and/or 'iconutil' not found; skipping icon generation"
      fi
    fi

    echo -e "${GREEN}✓${NC} Created app bundle: $APP_DIR"
    echo "Open with: open \"$APP_DIR\""
}

setup_linux_desktop_entry() {
    echo ""
    echo "Setting up Linux desktop entry..."
    APPS_DIR="$HOME/.local/share/applications"
    DESKTOP_FILE="$APPS_DIR/bob-bar.desktop"
    mkdir -p "$APPS_DIR"

    # Determine icon source (custom, repo PNG/SVG, or placeholder)
    ICON_SRC="$(resolve_icon_source)"

    ICON_KEY="applications-utilities"
    if [ -n "$ICON_SRC" ]; then
      ICON_BASE_DIR="$HOME/.local/share/icons/hicolor"
      mkdir -p "$ICON_BASE_DIR/256x256/apps" "$ICON_BASE_DIR/128x128/apps" "$ICON_BASE_DIR/64x64/apps"
      # Prefer 'convert' if available, else try 'sips' via xdg-open? We'll just copy as-is and let the theme scale
      if command -v convert >/dev/null 2>&1; then
        convert "$ICON_SRC" -resize 256x256 "$ICON_BASE_DIR/256x256/apps/bob-bar.png" || true
        convert "$ICON_SRC" -resize 128x128 "$ICON_BASE_DIR/128x128/apps/bob-bar.png" || true
        convert "$ICON_SRC" -resize 64x64 "$ICON_BASE_DIR/64x64/apps/bob-bar.png" || true
      else
        cp "$ICON_SRC" "$ICON_BASE_DIR/256x256/apps/bob-bar.png" || true
      fi
      ICON_KEY="bob-bar"
      echo -e "${GREEN}✓${NC} Installed icon under hicolor theme"
    fi

    cat > "$DESKTOP_FILE" << EOF
[Desktop Entry]
Type=Application
Name=Bob Bar
Comment=Fast AI launcher
Exec=$INSTALL_DIR/bob-bar
Terminal=false
Icon=$ICON_KEY
Categories=Utility;Development;
EOF

    chmod 644 "$DESKTOP_FILE"
    echo -e "${GREEN}✓${NC} Installed desktop launcher: $DESKTOP_FILE"
    echo "You may need to run 'update-desktop-database' or re-log for menus to refresh."
}

# Helper: build bob-bar from source (preferring current repo)
build_from_source() {
    echo "Building from source..."

    # If we're already in the bob-bar repo, use it; otherwise try to clone
    if [ -f "Cargo.toml" ] && grep -q 'name *= *"bob-bar"' Cargo.toml; then
        echo -e "${GREEN}✓${NC} Detected local bob-bar repository"
        LOCAL_BUILD_DIR="$(pwd)"
    else
        echo "Local repo not detected." 
        echo "Cloning repository..."
        git clone "https://github.com/$REPO.git" /tmp/bob-bar-build || {
            echo -e "${RED}Error: Failed to clone repository${NC}"
            echo "Please check your network and try again."
            exit 1
        }
        LOCAL_BUILD_DIR="/tmp/bob-bar-build"
        cd "$LOCAL_BUILD_DIR"
    fi

    echo "Building bob-bar (this may take a few minutes)..."
    cargo build --release
    BINARY_PATH="$LOCAL_BUILD_DIR/target/release/bob-bar"
}

# Always build from source for the current machine
build_from_source

# Create installation directory
echo ""
echo "Installing bob-bar..."
mkdir -p "$INSTALL_DIR"

# Make binary executable and move to install directory
chmod +x "$BINARY_PATH"
cp "$BINARY_PATH" "$INSTALL_DIR/bob-bar"

echo -e "${GREEN}✓${NC} Installed to $INSTALL_DIR/bob-bar"

# Setup configuration directory
echo ""
echo "Setting up configuration..."
mkdir -p "$CONFIG_DIR"

# Check if config files already exist
CONFIG_EXISTS=false
if [ -f "$CONFIG_DIR/config.toml" ]; then
    CONFIG_EXISTS=true
    echo -e "${YELLOW}⚠${NC}  Configuration files already exist, skipping..."
else
    # Download example configs if available
    echo "Creating default configuration files..."

    # Create default config.toml
    cat > "$CONFIG_DIR/config.toml" << 'EOF'
# bob-bar Configuration

# Ollama server configuration
[ollama]
# Base URL for the Ollama server
# Default: http://localhost:11434
host = "http://localhost:11434"

# Model to use for generating responses
# Options: llama2, codellama, mistral, llama2:13b, etc.
# See available models: ollama list
model = "llama2"

# Vision model to use for analyzing screenshots
# Options: llama3.2-vision:11b, llava, llava:13b, bakllava, etc.
# See available models: ollama list
vision_model = "llama3.2-vision:11b"

# Maximum number of tool iterations per query
# This prevents infinite loops when chaining tools
# Default: 5
max_tool_turns = 5

# Window configuration
[window]
# Initial window dimensions (in pixels)
width = 1200
height = 1200

# Minimum window size (in pixels)
min_width = 400
min_height = 300
EOF

    # Create api_keys.toml template
    cat > "$CONFIG_DIR/api_keys.toml" << 'EOF'
# API Keys Configuration
# Add your actual API keys below

[keys]
# OpenWeather API key for weather tool
# Get your key at: https://openweathermap.org/api
# OPENWEATHER_API_KEY = "your_openweather_api_key_here"

# GitHub Personal Access Token for GitHub API
# Create at: https://github.com/settings/tokens
# GITHUB_TOKEN = "your_github_token_here"

# Brave Search API Key
# Get from: https://brave.com/search/api/
# BRAVE_API_KEY = "your_brave_api_key_here"
EOF

    # Create tools.json template
    cat > "$CONFIG_DIR/tools.json" << 'EOF'
{
  "tools": {
    "http": [],
    "mcp": []
  }
}
EOF

    echo -e "${GREEN}✓${NC} Created configuration files in $CONFIG_DIR"
fi

# Check if Ollama is installed
echo ""
echo "Checking dependencies..."
if ! command -v ollama &> /dev/null; then
    echo -e "${YELLOW}⚠${NC}  Ollama not found"
    echo "bob-bar requires Ollama to be installed and running."
    echo "Install from: https://ollama.ai"
else
    echo -e "${GREEN}✓${NC} Found Ollama"
fi

# Check if install directory is in PATH
# Check for screenshot tool
echo ""
echo "Checking for screenshot tool..."
if [ "$PLATFORM" = "macos" ]; then
    if command -v screencapture &> /dev/null; then
        echo -e "${GREEN}✓${NC} Found screencapture (macOS built-in)"
    else
        echo -e "${YELLOW}⚠${NC}  Could not find macOS 'screencapture' tool"
        echo "It should be built-in; ensure Xcode Command Line Tools are installed."
    fi
else
    if command -v grim &> /dev/null; then
        echo -e "${GREEN}✓${NC} Found grim (Wayland screenshot tool)"
    elif command -v scrot &> /dev/null; then
        echo -e "${GREEN}✓${NC} Found scrot (X11 screenshot tool)"
    else
        echo -e "${YELLOW}⚠${NC}  No screenshot tool found"
        echo "For screenshot analysis, install one of:"
        echo "  - Wayland: sudo apt install grim"
        echo "  - X11: sudo apt install scrot"
    fi
fi

echo ""
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${YELLOW}⚠${NC}  $INSTALL_DIR is not in your PATH"
    echo ""
    echo "Add it by adding this line to your ~/.bashrc or ~/.zshrc:"
    echo ""
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
    echo "Then run: source ~/.bashrc  (or ~/.zshrc)"
else
    echo -e "${GREEN}✓${NC} $INSTALL_DIR is in PATH"
fi

# Platform-specific GUI integration
if [ "$PLATFORM" = "macos" ]; then
    setup_macos_app_bundle || echo -e "${YELLOW}⚠${NC}  Failed to create macOS app bundle"
elif [ "$PLATFORM" = "linux" ]; then
    setup_linux_desktop_entry || echo -e "${YELLOW}⚠${NC}  Failed to create .desktop entry"
fi

# No temp files to clean up when building from source

echo ""
echo "╔════════════════════════════════════════╗"
echo "║     Installation Complete! 🎉          ║"
echo "╚════════════════════════════════════════╝"
echo ""
echo "Run bob-bar with:"
echo "    bob-bar"
echo ""
if [ "$PLATFORM" = "macos" ]; then
    echo "Or double-click:"
    echo "    ~/Applications/Bob Bar.app"
    echo ""
elif [ "$PLATFORM" = "linux" ]; then
    echo "It should now appear in your app launcher menu as 'Bob Bar'."
    echo ""
fi
if [ "$CONFIG_EXISTS" = false ]; then
    echo "Configuration files created at:"
    echo "    $CONFIG_DIR"
    echo ""
    echo "Edit config.toml to customize settings:"
    echo "    \$EDITOR $CONFIG_DIR/config.toml"
    echo ""
fi
echo "Documentation: https://github.com/$REPO"
echo ""
