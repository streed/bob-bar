# bob-bar

A fast, elegant AI launcher built with Rust and Iced. bob-bar provides instant access to AI assistance through a beautiful, always-on-top interface - think Spotlight or Rofi, but for AI queries.

![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)
![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)

## Features

- **⚡ Lightning Fast** - Native Rust performance with minimal resource usage
- **🎯 Launcher-Style Interface** - Always-on-top, centered window that appears instantly
- **🤖 Local AI Integration** - Connects to Ollama for private, local AI inference
- **🔧 Tool Support** - Extensible tool system with HTTP and MCP protocol support
- **📋 Copy Output** - One-click copy to clipboard
- **📸 Screenshot Analysis** - Capture and analyze screenshots with vision models
- **🎨 Beautiful UI** - Clean, modern interface with smooth animations and streaming responses
- **🔔 Desktop Notifications** - Get notified when queries complete
- **⌨️ Keyboard-First** - ESC to close, enter to submit - stay focused

## Quick Start

### Prerequisites

- [Ollama](https://ollama.ai) running locally
- For screenshot feature: `grim` (Wayland) or `scrot` (X11)

### Installation

**Quick Install (Linux)**

```bash
curl -fsSL https://raw.githubusercontent.com/streed/bob-bar/main/install.sh | bash
```

The install script will:
- Download the latest release for your architecture (x86_64 or aarch64)
- Install bob-bar to `~/.local/bin/bob-bar`
- Create default configuration files in `~/.config/bob-bar/`
- Check for required dependencies

**Manual Installation**

If you prefer to build from source:

```bash
# Prerequisites: Rust 1.70 or higher
# Install from: https://rustup.rs

# Clone the repository
git clone https://github.com/streed/bob-bar.git
cd bob-bar

# Build and install
cargo build --release
sudo cp target/release/bob-bar /usr/local/bin/

# Setup configuration
mkdir -p ~/.config/bob-bar
cp config.example.toml ~/.config/bob-bar/config.toml
cp api_keys.example.toml ~/.config/bob-bar/api_keys.toml
cp tools.example.json ~/.config/bob-bar/tools.json
```

## Configuration

bob-bar uses configuration files located in `~/.config/bob-bar/`:

### `~/.config/bob-bar/config.toml`

```toml
[ollama]
host = "http://localhost:11434"
model = "llama2"                      # Model for text queries
vision_model = "llama3.2-vision:11b"  # Model for screenshot analysis
max_tool_turns = 5

[window]
width = 1200
height = 1200
min_width = 400
min_height = 300
```

### `~/.config/bob-bar/api_keys.toml` (Optional)

For tools that require API keys:

```toml
[keys]
OPENWEATHER_API_KEY = "your_key_here"
GITHUB_TOKEN = "your_token_here"
BRAVE_API_KEY = "your_key_here"
```

### `~/.config/bob-bar/tools.json` (Optional)

Define custom HTTP tools and MCP servers:

```json
{
  "tools": {
    "http": [
      {
        "name": "weather",
        "description": "Get current weather for a location",
        "url": "https://api.openweathermap.org/data/2.5/weather",
        "method": "GET",
        "parameters": {
          "q": "{{city}}",
          "appid": "$OPENWEATHER_API_KEY"
        }
      }
    ],
    "mcp": []
  }
}
```

## Usage

**Starting bob-bar**

Normal mode:
```bash
bob-bar
```

Screenshot analysis mode:
```bash
bob-bar --screenshot
```

Debug mode (shows detailed logging):
```bash
bob-bar --debug
```

### Run Without Opening a Terminal

macOS (double-clickable app):

1) Build the binary
```bash
cargo build --release
```
2) Create the app bundle
```bash
./packaging/macos/make_app_bundle.sh
```
3) Open in Finder
```bash
open "dist/macos/Bob Bar.app"
```

Notes:
- The bundle includes a minimal `Info.plist` and runs without launching Terminal.
- To hide the Dock icon and menu bar, set `LSUIElement` to `true` in `packaging/macos/Info.plist`.

Make sure Ollama is running first:
```bash
ollama serve

# For screenshot analysis, pull a vision model:
ollama pull llama3.2-vision:11b
```

**Using bob-bar**

1. **Launch the app** - bob-bar appears centered on your screen
2. **Type your question** - The input field is auto-focused
3. **Watch responses stream** - See AI responses appear in real-time as they're generated
4. **Desktop notifications** - Get notified when long-running queries complete, even if window is in background
5. **Copy results** - Click the [Copy] button to copy output to clipboard
6. **Close quickly** - Press ESC to dismiss the window

### Screenshot Analysis

The `--screenshot` flag captures your current screen and analyzes it with a vision model:

```bash
bob-bar --screenshot
```

This will:
1. Capture a screenshot of your current desktop (supports Wayland and X11)
2. Send it to the vision model (configured as `vision_model` in config)
3. Display helpful insights about what's on screen
4. Identify issues, extract information, and suggest improvements

**Requirements:**
- Wayland: Install `grim` (`sudo apt install grim` or `sudo pacman -S grim`)
- X11: Install `scrot` (`sudo apt install scrot`)

Great for:
- Getting help with error messages
- Understanding complex UIs
- Extracting text from images
- Analyzing diagrams and charts

### Keyboard Shortcuts

- `Enter` - Submit query
- `Escape` - Close application
- `Cmd/Ctrl+C` - Copy from input field

## Architecture

bob-bar is built with:

- **[Iced](https://github.com/iced-rs/iced)** - Cross-platform GUI framework
- **[Tokio](https://tokio.rs/)** - Async runtime
- **[Reqwest](https://github.com/seanmonstar/reqwest)** - HTTP client
- **Ollama** - Local AI inference

### Project Structure

```
bob-bar/
├── src/
│   ├── main.rs      # UI and application logic
│   ├── ollama.rs    # Ollama API client
│   ├── tools.rs     # Tool execution system
│   └── config.rs    # Configuration management
├── config.example.toml      # Example configuration
├── api_keys.example.toml    # Example API keys
└── tools.example.json       # Example tool definitions

~/.config/bob-bar/           # User configuration directory
├── config.toml              # Main configuration
├── api_keys.toml            # API keys (not in repo)
└── tools.json               # Tool definitions (optional)
```

## Development

### Building

```bash
# Debug build
cargo build

# Release build (optimized)
cargo build --release

# Run with logging
RUST_LOG=debug cargo run
```

### Creating a Release

To create a new release:

```bash
# Tag the release
git tag v0.1.0
git push origin main
git push origin v0.1.0
```

GitHub Actions will automatically:
- Build binaries for x86_64 and aarch64 Linux
- Create a GitHub release with binaries
- Include the install script in the release

### Code Style

This project follows standard Rust conventions. Format code with:

```bash
cargo fmt
```

Run linter:

```bash
cargo clippy
```

## Troubleshooting

### Ollama Connection Issues

Ensure Ollama is running:

```bash
ollama serve
```

Verify connectivity:

```bash
curl http://localhost:11434/api/tags
```

### Screenshot Not Working

Install the appropriate screenshot tool:

**Wayland:**
```bash
sudo apt install grim    # Debian/Ubuntu
sudo pacman -S grim      # Arch
```

**X11:**
```bash
sudo apt install scrot   # Debian/Ubuntu
sudo pacman -S scrot     # Arch
```

### Font/Unicode Issues

bob-bar uses monospace fonts by default. If unicode characters don't display correctly, ensure your system has fonts with good unicode coverage installed (e.g., Noto Sans Mono, DejaVu Sans Mono).

### Window Not Centering

Some window managers may override window positioning. This is a known limitation on certain Linux desktop environments.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## License

This project is licensed under the GNU General Public License v3.0 - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [Iced](https://github.com/iced-rs/iced) for the excellent GUI framework
- [Ollama](https://ollama.ai) for making local AI accessible
- The Rust community for amazing tools and libraries

## Roadmap

- [ ] Global hotkey support
- [ ] Multi-model switching
- [ ] Conversation history
- [ ] Plugin system
- [ ] Theme customization
- [ ] Windows and macOS testing

---

Made with ❤️ and Rust
