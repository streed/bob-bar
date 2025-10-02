#!/bin/bash
set -e

# bob-bar installer script
# This script downloads and installs bob-bar from GitHub releases

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

# Detect architecture and determine candidate asset names
ARCH=$(uname -m)
ASSET_CANDIDATES=()
case "$PLATFORM" in
    linux)
        case "$ARCH" in
            x86_64)
                ASSET_CANDIDATES+=("bob-bar-linux-x86_64" "bob-bar-x86_64-unknown-linux-gnu")
                ;;
            aarch64|arm64)
                # Many Linux arm64 systems report aarch64; include both
                ASSET_CANDIDATES+=("bob-bar-linux-aarch64" "bob-bar-aarch64-unknown-linux-gnu")
                ;;
            *)
                echo -e "${RED}Error: Unsupported Linux architecture: $ARCH${NC}"
                echo "Supported architectures: x86_64, aarch64/arm64"
                exit 1
                ;;
        esac
        ;;
    macos)
        case "$ARCH" in
            x86_64)
                ASSET_CANDIDATES+=(
                  "bob-bar-macos-x86_64"
                  "bob-bar-darwin-x86_64"
                  "bob-bar-x86_64-apple-darwin"
                )
                ;;
            arm64)
                ASSET_CANDIDATES+=(
                  "bob-bar-macos-arm64"
                  "bob-bar-darwin-arm64"
                  "bob-bar-aarch64-apple-darwin"
                )
                ;;
            *)
                echo -e "${RED}Error: Unsupported macOS architecture: $ARCH${NC}"
                echo "Supported architectures: x86_64, arm64"
                exit 1
                ;;
        esac
        ;;
esac

echo -e "${GREEN}✓${NC} Detected platform: $PLATFORM, architecture: $ARCH"

# Check for required commands
if ! command -v curl &> /dev/null; then
    echo -e "${RED}Error: curl is required but not installed${NC}"
    if [ "$PLATFORM" = "macos" ]; then
        echo "Install it with: brew install curl (or install Xcode Command Line Tools)"
    else
        echo "Install it with: sudo apt install curl"
    fi
    exit 1
fi

echo -e "${GREEN}✓${NC} Found curl"

# Helper: build bob-bar from source (preferring current repo)
build_from_source() {
    echo "Building from source..."

    # If we're already in the bob-bar repo, use it; otherwise try to clone
    if [ -f "Cargo.toml" ] && grep -q 'name *= *"bob-bar"' Cargo.toml; then
        echo -e "${GREEN}✓${NC} Detected local bob-bar repository"
        LOCAL_BUILD_DIR="$(pwd)"
    else
        echo "Local repo not detected."
        echo "Attempting to clone repository..."
        if command -v git >/dev/null 2>&1; then
            git clone "https://github.com/$REPO.git" /tmp/bob-bar-build || {
                echo -e "${YELLOW}⚠${NC}  Clone failed or network unavailable."
                echo "To build manually, run:"
                echo "  git clone https://github.com/$REPO.git"
                echo "  cd bob-bar && cargo build --release"
                exit 1
            }
            LOCAL_BUILD_DIR="/tmp/bob-bar-build"
            cd "$LOCAL_BUILD_DIR"
        else
            echo -e "${RED}Error: git not available to clone source${NC}"
            echo "Please clone the repo manually and re-run this installer from within it."
            exit 1
        fi
    fi

    # Check for Rust toolchain
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}Error: Rust/cargo not found${NC}"
        echo "Install Rust from: https://rustup.rs"
        exit 1
    fi

    echo "Building bob-bar (this may take a few minutes)..."
    cargo build --release
    BINARY_PATH="$LOCAL_BUILD_DIR/target/release/bob-bar"
}

# Get latest release info from GitHub
echo ""
echo "Fetching latest release..."
RELEASE_URL="https://api.github.com/repos/$REPO/releases/latest"
RELEASE_INFO=$(curl -s "$RELEASE_URL" || true)

# Check if we got a valid response
if [ -z "$RELEASE_INFO" ] || echo "$RELEASE_INFO" | grep -q "Not Found"; then
    echo -e "${YELLOW}⚠${NC}  Could not fetch releases or none available"
    build_from_source
else
    # Extract download URL for our platform/architecture by scanning candidates
    DOWNLOAD_URL=""
    for CANDIDATE in "${ASSET_CANDIDATES[@]}"; do
        URL=$(echo "$RELEASE_INFO" | grep "browser_download_url" | grep -E "$CANDIDATE(\.|$)" | cut -d '"' -f 4 | head -n 1)
        if [ -n "$URL" ]; then
            DOWNLOAD_URL="$URL"
            break
        fi
    done

    if [ -z "$DOWNLOAD_URL" ]; then
        echo -e "${YELLOW}⚠${NC}  No matching release asset found for $PLATFORM/$ARCH"
        echo "Searched candidates: ${ASSET_CANDIDATES[*]}"
        echo "Falling back to local source build if available..."
        build_from_source
    else

        VERSION=$(echo "$RELEASE_INFO" | grep '"tag_name"' | cut -d '"' -f 4)
        echo -e "${GREEN}✓${NC} Found version: $VERSION"

        # Download binary
        echo ""
        echo "Downloading bob-bar..."
        TEMP_FILE=$(mktemp)
        curl -L -o "$TEMP_FILE" "$DOWNLOAD_URL"

        if [ ! -s "$TEMP_FILE" ]; then
            echo -e "${RED}Error: Download failed${NC}"
            exit 1
        fi

        echo -e "${GREEN}✓${NC} Downloaded successfully"

        BINARY_PATH="$TEMP_FILE"
    fi
fi

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

    # Check if Ollama is running
    if curl -s http://localhost:11434/api/tags &> /dev/null; then
        echo -e "${GREEN}✓${NC} Ollama is running"
    else
        echo -e "${YELLOW}⚠${NC}  Ollama is not running"
        echo "Start it with: ollama serve"
    fi
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

# Cleanup
if [ -n "$TEMP_FILE" ] && [ -f "$TEMP_FILE" ]; then
    rm -f "$TEMP_FILE"
fi

echo ""
echo "╔════════════════════════════════════════╗"
echo "║     Installation Complete! 🎉          ║"
echo "╚════════════════════════════════════════╝"
echo ""
echo "Run bob-bar with:"
echo "    bob-bar"
echo ""
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
