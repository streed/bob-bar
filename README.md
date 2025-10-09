# bob-bar

A fast, elegant AI launcher built with Rust and Iced. bob-bar provides instant access to AI assistance through a beautiful, always-on-top interface - think Spotlight or Rofi, but for AI queries.

![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)
![License](https://img.shields.io/badge/license-GPL--3.0-blue.svg)

## Features

- **⚡ Lightning Fast** - Native Rust performance with minimal resource usage
- **🎯 Launcher-Style Interface** - Always-on-top, centered window that appears instantly
- **🤖 Local AI Integration** - Connects to Ollama for private, local AI inference
- **🔬 Research Mode** - Multi-agent system for publication-quality research with citations and fact-checking
- **🔧 Tool Support** - Extensible tool system with HTTP and MCP protocol support
- **📋 Copy Output** - One-click copy to clipboard
- **📸 Screenshot Analysis** - Capture and analyze screenshots with vision models
- **🎨 Beautiful UI** - Clean, modern interface with smooth animations and streaming responses
- **🔔 Desktop Notifications** - Get notified when queries complete
- **⌨️ Keyboard-First** - ESC to close, enter to submit - stay focused
- **🗂 History Sidebar** - Browse previous queries and outputs; stored locally in SQLite

## Quick Start

### Prerequisites

- [Ollama](https://ollama.ai) running locally
- For screenshot feature: `grim` (Wayland) or `scrot` (X11)

### Installation

**Quick Install (Linux/macOS, builds from source)**

```bash
curl -fsSL https://raw.githubusercontent.com/streed/bob-bar/main/install.sh | bash
```

**Quick Install (Windows, builds from source)**

```powershell
irm https://raw.githubusercontent.com/streed/bob-bar/main/install.ps1 | iex
```

Requirements:
- `git` and Rust (`cargo`) installed. Install Rust from https://rustup.rs

The install script will:
- Clone the repository and build a native binary for your machine
- Install bob-bar to `~/.local/bin/bob-bar` (Linux/macOS) or `%USERPROFILE%\.local\bin` (Windows)
- Create default configuration files in `~/.config/bob-bar/` (Linux/macOS) or `%APPDATA%\bob-bar` (Windows)
- On macOS: create a double‑clickable app at `~/Applications/Bob Bar.app`
- On Linux: create a launcher at `~/.local/share/applications/bob-bar.desktop`
- On Windows: automatically add installation directory to PATH

Icon behavior (Linux/macOS)
- Default branded icon: included at `packaging/icons/bob-bar.svg`. The installer converts it to PNG when `rsvg-convert`, `inkscape`, or ImageMagick `convert` is available.
- Custom icon: provide `BOB_BAR_ICON=/path/to/icon.png` when running the installer to override the default.
- macOS: if `sips` and `iconutil` are available, the installer generates an `.icns` and embeds it in the app bundle.
- Linux: the icon is installed under `~/.local/share/icons/hicolor/...` and the launcher uses it (`Icon=bob-bar`).
- No converters available: falls back to a minimal placeholder PNG, so the app still has an icon.

**Manual Installation**

If you prefer to build from source:

Linux/macOS:
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

Windows (PowerShell):
```powershell
# Prerequisites: Rust 1.70 or higher
# Install from: https://rustup.rs

# Clone the repository
git clone https://github.com/streed/bob-bar.git
cd bob-bar

# Build and install
cargo build --release
mkdir -Force "$env:USERPROFILE\.local\bin"
copy target\release\bob-bar.exe "$env:USERPROFILE\.local\bin\"

# Setup configuration
mkdir -Force "$env:APPDATA\bob-bar"
copy config.example.toml "$env:APPDATA\bob-bar\config.toml"
copy api_keys.example.toml "$env:APPDATA\bob-bar\api_keys.toml"
copy tools.example.json "$env:APPDATA\bob-bar\tools.json"
```

## Configuration

bob-bar uses configuration files located in:
- Linux/macOS: `~/.config/bob-bar/`
- Windows: `%APPDATA%\bob-bar`

### `~/.config/bob-bar/config.toml`

```toml
[ollama]
host = "http://localhost:11434"
model = "llama2"                               # Model for text queries
vision_model = "llama3.2-vision:11b"           # Model for screenshot analysis
research_model = "llama2:70b"                  # Model for research mode (optional)
summarization_model = "llama2:7b"              # Model for summarization (optional)
embedding_model = "nomic-embed-text"           # Embedding model for vector search
embedding_dimensions = 768                     # Embedding vector dimensions
context_window = 128000                        # Context window size (tokens)
max_tool_turns = 5                             # Max tool iterations per query
max_refinement_iterations = 5                  # Critic-refiner loop iterations
max_document_iterations = 3                    # Document writing iterations
max_debate_rounds = 2                          # Multi-round debate rounds
max_plan_iterations = 3                        # Planning iterations
api_delay_ms = 100                             # Delay between API calls (ms)
summarization_threshold = 5000                 # Chat summarization threshold (chars)
summarization_threshold_research = 50000       # Research summarization threshold (chars)

[research]
min_worker_count = 3                           # Minimum parallel research workers
max_worker_count = 10                          # Maximum parallel research workers
export_memories = false                        # Export memory summary to document
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
6. **Browse history** - Use the left sidebar to load previous queries/answers
7. **Close quickly** - Press ESC to dismiss the window

### Research Mode

bob-bar includes a sophisticated multi-agent research system for producing publication-quality, well-sourced documents:

**Enable Research Mode:**
Click the `[Research: OFF]` button to toggle to `[Research: ON]`

**Example Query:**
```
Compare Python and Rust performance characteristics,
including benchmark data and real-world use cases.
```

**What Happens:**
1. **Query Decomposition** - Lead agent breaks query into 5-8 verifiable sub-questions
2. **Parallel Research** - Specialized workers research concurrently using web_search
3. **Multi-Round Debate** - Advocate and skeptic agents verify quality with fact-checking
4. **Iterative Refinement** - Refiner addresses gaps identified in debate
5. **Document Writing** - Professional document created with inline citations
6. **References Section** - Clickable URLs automatically extracted and listed

**Output Features:**
- Every claim cited with source URLs: `[Source: https://example.com]`
- Structured markdown with Executive Summary, Main Content, and References
- Independent verifiability - all claims traceable to sources
- Publication-ready quality standards

**Configuration:**
- Set dedicated research model in config: `research_model = "llama2:70b"`
- Customize agents in `~/.config/bob-bar/agents.json`
- Tune debate rounds, worker count, and iterations

See [RESEARCH_MODE.md](RESEARCH_MODE.md) for complete documentation.

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
│   ├── ollama.rs    # Ollama API client and tool integration
│   ├── tools.rs     # Tool execution system (HTTP, MCP, built-in)
│   ├── config.rs    # Configuration management
│   ├── history.rs   # SQLite-backed query/response history
│   └── research.rs  # Multi-agent research orchestration
├── config.example.toml      # Example configuration
├── agents.example.json      # Example research agent configuration
├── api_keys.example.toml    # Example API keys
├── tools.example.json       # Example tool definitions
├── README.md                # This file
└── RESEARCH_MODE.md         # Research mode documentation

~/.config/bob-bar/           # User configuration directory
├── config.toml              # Main configuration
├── agents.json              # Research agent configuration (optional)
├── api_keys.toml            # API keys (not in repo)
├── tools.json               # Tool definitions (optional)
└── history.sqlite           # Local history database (auto-created)
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
