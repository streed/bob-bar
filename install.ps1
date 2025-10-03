# bob-bar installer script for Windows
# This script builds bob-bar from source for your current machine

$ErrorActionPreference = "Stop"

$REPO = "streed/bob-bar"
$INSTALL_DIR = "$env:USERPROFILE\.local\bin"
$CONFIG_DIR = "$env:APPDATA\bob-bar"

# Helper function for colored output
function Write-ColorOutput {
    param(
        [string]$Message,
        [string]$Color = "White"
    )
    Write-Host $Message -ForegroundColor $Color
}

function Write-Success {
    param([string]$Message)
    Write-ColorOutput "✓ $Message" -Color Green
}

function Write-Warning {
    param([string]$Message)
    Write-ColorOutput "⚠ $Message" -Color Yellow
}

function Write-Error {
    param([string]$Message)
    Write-ColorOutput "✗ $Message" -Color Red
}

Write-Host "╔════════════════════════════════════════╗"
Write-Host "║        bob-bar Installer               ║"
Write-Host "╚════════════════════════════════════════╝"
Write-Host ""

# Detect platform and architecture
$ARCH = $env:PROCESSOR_ARCHITECTURE
Write-Success "Detected platform: Windows, architecture: $ARCH"

# Check for required commands
Write-Host ""
Write-Host "Checking prerequisites..."

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Error "git is required but not installed"
    Write-Host "Install git from: https://git-scm.com/download/win"
    exit 1
}
Write-Success "Found git"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Error "Rust/cargo is required but not installed"
    Write-Host "Install Rust from: https://rustup.rs"
    exit 1
}
Write-Success "Found cargo"

# Helper: build bob-bar from source (preferring current repo)
function Build-FromSource {
    Write-Host ""
    Write-Host "Building from source..."

    # If we're already in the bob-bar repo, use it; otherwise try to clone
    if ((Test-Path "Cargo.toml") -and (Select-String -Path "Cargo.toml" -Pattern 'name\s*=\s*"bob-bar"' -Quiet)) {
        Write-Success "Detected local bob-bar repository"
        $script:LOCAL_BUILD_DIR = (Get-Location).Path
    } else {
        Write-Host "Local repo not detected."
        Write-Host "Cloning repository..."
        
        $TMP_BUILD_DIR = Join-Path $env:TEMP "bob-bar-build"
        if (Test-Path $TMP_BUILD_DIR) {
            Remove-Item -Recurse -Force $TMP_BUILD_DIR
        }
        
        try {
            git clone "https://github.com/$REPO.git" $TMP_BUILD_DIR
            if ($LASTEXITCODE -ne 0) { throw "Git clone failed" }
        } catch {
            Write-Error "Failed to clone repository"
            Write-Host "Please check your network and try again."
            exit 1
        }
        
        $script:LOCAL_BUILD_DIR = $TMP_BUILD_DIR
        Set-Location $LOCAL_BUILD_DIR
    }

    Write-Host "Building bob-bar (this may take a few minutes)..."
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Build failed"
        exit 1
    }
    
    $script:BINARY_PATH = Join-Path $LOCAL_BUILD_DIR "target\release\bob-bar.exe"
}

# Build from source
Build-FromSource

# Create installation directory
Write-Host ""
Write-Host "Installing bob-bar..."
if (-not (Test-Path $INSTALL_DIR)) {
    New-Item -ItemType Directory -Path $INSTALL_DIR -Force | Out-Null
}

# Copy binary to install directory
Copy-Item -Path $BINARY_PATH -Destination "$INSTALL_DIR\bob-bar.exe" -Force
Write-Success "Installed to $INSTALL_DIR\bob-bar.exe"

# Setup configuration directory
Write-Host ""
Write-Host "Setting up configuration..."
if (-not (Test-Path $CONFIG_DIR)) {
    New-Item -ItemType Directory -Path $CONFIG_DIR -Force | Out-Null
}

# Check if config files already exist
$CONFIG_EXISTS = $false
if (Test-Path "$CONFIG_DIR\config.toml") {
    $CONFIG_EXISTS = $true
    Write-Warning "Configuration files already exist, skipping..."
} else {
    # Create default configuration files
    Write-Host "Creating default configuration files..."

    # Create default config.toml
    $configContent = @'
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
'@
    $configContent | Out-File -FilePath "$CONFIG_DIR\config.toml" -Encoding utf8

    # Create api_keys.toml template
    $apiKeysContent = @'
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
'@
    $apiKeysContent | Out-File -FilePath "$CONFIG_DIR\api_keys.toml" -Encoding utf8

    # Create tools.json template
    $toolsContent = @'
{
  "tools": {
    "http": [],
    "mcp": []
  }
}
'@
    $toolsContent | Out-File -FilePath "$CONFIG_DIR\tools.json" -Encoding utf8

    Write-Success "Created configuration files in $CONFIG_DIR"
}

# Check if Ollama is installed
Write-Host ""
Write-Host "Checking dependencies..."
if (-not (Get-Command ollama -ErrorAction SilentlyContinue)) {
    Write-Warning "Ollama not found"
    Write-Host "bob-bar requires Ollama to be installed and running."
    Write-Host "Install from: https://ollama.ai"
} else {
    Write-Success "Found Ollama"
}

# Check if install directory is in PATH
Write-Host ""
$currentPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($currentPath -notlike "*$INSTALL_DIR*") {
    Write-Warning "$INSTALL_DIR is not in your PATH"
    Write-Host ""
    Write-Host "Adding $INSTALL_DIR to your user PATH..."
    
    try {
        $newPath = if ($currentPath) { "$currentPath;$INSTALL_DIR" } else { $INSTALL_DIR }
        [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
        Write-Success "Added to PATH. You may need to restart your terminal."
    } catch {
        Write-Warning "Could not automatically add to PATH"
        Write-Host ""
        Write-Host "Please add it manually:"
        Write-Host "  1. Search for 'Environment Variables' in Windows settings"
        Write-Host "  2. Edit your user PATH variable"
        Write-Host "  3. Add: $INSTALL_DIR"
    }
} else {
    Write-Success "$INSTALL_DIR is in PATH"
}

# Installation complete
Write-Host ""
Write-Host "╔════════════════════════════════════════╗"
Write-Host "║     Installation Complete! 🎉          ║"
Write-Host "╚════════════════════════════════════════╝"
Write-Host ""
Write-Host "Run bob-bar with:"
Write-Host "    bob-bar"
Write-Host ""
Write-Host "If you just added the PATH, restart your terminal first."
Write-Host ""

if (-not $CONFIG_EXISTS) {
    Write-Host "Configuration files created at:"
    Write-Host "    $CONFIG_DIR"
    Write-Host ""
    Write-Host "Edit config.toml to customize settings:"
    Write-Host "    notepad `"$CONFIG_DIR\config.toml`""
    Write-Host ""
}

Write-Host "Documentation: https://github.com/$REPO"
Write-Host ""
