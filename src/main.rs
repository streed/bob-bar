#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod config;
mod progress;
mod history;
mod ollama;
mod tools;
mod screenshot;
mod research;
mod embeddings;
mod shared_memory;
mod dynamic_context;

use iced::{
    widget::{column, row, container, scrollable, text, text_input, button, text_input::Id, rich_text, span, text_editor, Space},
    Element, Length, Task, Theme, Font, Subscription,
    time, clipboard,
    keyboard::{self, Key},
    event::{self, Event as IcedEvent},
    alignment, Padding,
    window::{self, Level, settings::PlatformSpecific},
    Color,
};
use iced::widget::text as text_widget;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use pulldown_cmark::{Parser, Event as MarkdownEvent, Tag, HeadingLevel, Options, Alignment};
use notify_rust::Notification;
use iced::widget::scrollable::{Direction, Scrollbar};
use std::sync::atomic::{AtomicBool, Ordering};
use unicode_width::{UnicodeWidthStr, UnicodeWidthChar};
use once_cell::sync::Lazy;
use std::sync::Mutex as StdMutex;

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);
const ENABLE_NOTIFICATIONS: bool = false;
const TABLE_MAX_COL_WIDTH: usize = 80; // clamp overly wide columns to reduce horizontal scrolling
const INPUT_HEIGHT: f32 = 48.0; // approximate height to align sidebar header with input
const TABLE_PLAIN_TEXT_RULES: &str = "When including Markdown tables in your response: 1) do not apply any styling (no bold, italics, code formatting) to table headers or table cell values; 2) do not use Unicode symbols or emoji inside any table cells — use plain ASCII text only (letters, numbers, basic punctuation).";

// Global research progress store updated from background task, polled by Tick
static RESEARCH_PROGRESS_GLOBAL: Lazy<StdMutex<Option<String>>> = Lazy::new(|| StdMutex::new(None));

fn extract_hostname(url: &str) -> String {
    // Trim leading/trailing whitespace
    let u = url.trim();
    // Strip scheme
    let without_scheme = if let Some(pos) = u.find("://") {
        &u[pos + 3..]
    } else {
        u
    };
    // Take up to first path/query/fragment separator
    let host = without_scheme
        .split(|c| c == '/' || c == '?' || c == '#')
        .next()
        .unwrap_or(without_scheme);
    // Remove credentials if present and port
    let host = if let Some(at) = host.rfind('@') { &host[at + 1..] } else { host };
    let host = host.split(':').next().unwrap_or(host);
    // Remove common www. prefix for brevity
    let host = host.strip_prefix("www.").unwrap_or(host);
    host.to_string()
}

fn render_markdown(markdown: String) -> Element<'static, Message> {
    let mut md_options = Options::empty();
    md_options.insert(Options::ENABLE_TABLES);
    let parser = Parser::new_ext(&markdown, md_options);
    let mut spans = Vec::new();
    let mut blocks: Vec<Element<'static, Message>> = Vec::new();
    let flush_spans = |spans: &mut Vec<_>, blocks: &mut Vec<Element<'static, Message>>| {
        if !spans.is_empty() {
            blocks.push(rich_text(spans.clone()).width(Length::Fill).into());
            spans.clear();
        }
    };
    let mut current_text = String::new();
    let mut in_code_block = false;
    let mut code_block_content = String::new();
    let mut in_bold = false;
    let mut _in_italic = false;
    let mut heading_level: Option<HeadingLevel> = None;
    let mut in_list = false;
    // Table state
    let mut in_table = false;
    let mut in_table_head = false;
    let mut current_row: Vec<String> = Vec::new();
    let mut current_cell = String::new();
    let mut header_rows: Vec<Vec<String>> = Vec::new();
    let mut body_rows: Vec<Vec<String>> = Vec::new();
    let mut table_alignments: Vec<Alignment> = Vec::new();

    for event in parser {
        match event {
            MarkdownEvent::Start(tag) => {
                match tag {
                    Tag::Table(aligns) => {
                        // Flush any pending inline content before starting a table
                        flush_spans(&mut spans, &mut blocks);
                        in_table = true;
                        in_table_head = false;
                        header_rows.clear();
                        body_rows.clear();
                        table_alignments = aligns;
                    }
                    Tag::TableHead => {
                        in_table_head = true;
                    }
                    Tag::TableRow => {
                        current_row.clear();
                    }
                    Tag::TableCell => {
                        current_cell.clear();
                    }
                    Tag::Heading(level, _, _) => {
                        // Flush current text before heading
                        if !current_text.is_empty() {
                            spans.push(span(current_text.clone()));
                            current_text.clear();
                        }
                        // Add newline before heading if there's already content
                        if !spans.is_empty() {
                            spans.push(span("\n\n"));
                        }
                        heading_level = Some(level);
                    }
                    Tag::CodeBlock(_) => {
                        // Flush current text before code block
                        if !current_text.is_empty() {
                            spans.push(span(current_text.clone()));
                            current_text.clear();
                        }
                        in_code_block = true;
                    }
                    Tag::Strong => {
                        // Flush text before bold starts
                        if !current_text.is_empty() {
                            spans.push(span(current_text.clone()).size(15));
                            current_text.clear();
                        }
                        in_bold = true;
                    }
                    Tag::Emphasis => {
                        _in_italic = true;
                    }
                    Tag::Paragraph => {
                        // Flush text at paragraph start
                        if !current_text.is_empty() {
                            let mut text_span = span(current_text.clone()).size(15);
                            if in_bold {
                                text_span = text_span.color(Color::from_rgb(1.0, 1.0, 1.0));
                            }
                            spans.push(text_span);
                            current_text.clear();
                        }
                    }
                    Tag::List(_) => {
                        // Add spacing before list only if not nested
                        if !in_list {
                            if !current_text.is_empty() {
                                spans.push(span(current_text.clone()));
                                current_text.clear();
                            }
                            if !spans.is_empty() {
                                spans.push(span("\n"));
                            }
                        }
                        in_list = true;
                    }
                    Tag::Item => {
                        // Flush any pending text
                        if !current_text.is_empty() {
                            let mut text_span = span(current_text.clone()).size(15);
                            if in_bold {
                                text_span = text_span.color(Color::from_rgb(1.0, 1.0, 1.0));
                            }
                            spans.push(text_span);
                            current_text.clear();
                        }
                    }
                    _ => {}
                }
            }
            MarkdownEvent::End(tag) => {
                match tag {
                    Tag::Table(_) => {
                        // Close any in-progress cell/row
                        if !current_cell.is_empty() {
                            current_row.push(current_cell.clone());
                            current_cell.clear();
                        }
                        if !current_row.is_empty() {
                            if in_table_head {
                                header_rows.push(current_row.clone());
                            } else {
                                body_rows.push(current_row.clone());
                            }
                            current_row.clear();
                        }

                        // Build a monospaced table rendering (bordered)
                        let mut rows = Vec::new();
                        rows.extend(header_rows.iter().cloned());
                        rows.extend(body_rows.iter().cloned());

                        let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
                        let mut col_widths = vec![0usize; cols];
                        for r in &rows {
                            for (i, cell) in r.iter().enumerate() {
                                let len = UnicodeWidthStr::width(cell.as_str()).min(TABLE_MAX_COL_WIDTH);
                                col_widths[i] = col_widths[i].max(len);
                            }
                        }
                        let eff_widths = col_widths.clone();

                        const CELL_PAD: usize = 1;

                        let pad_cell = |s: &str, width: usize, align: Alignment| -> String {
                            let len = UnicodeWidthStr::width(s);
                            if len >= width { return s.to_string(); }
                            let pad = width - len;
                            match align {
                                Alignment::Left | Alignment::None => format!("{}{}", s, " ".repeat(pad)),
                                Alignment::Right => format!("{}{}", " ".repeat(pad), s),
                                Alignment::Center => {
                                    let left = pad / 2;
                                    let right = pad - left;
                                    format!("{}{}{}", " ".repeat(left), s, " ".repeat(right))
                                }
                            }
                        };

                        let make_border = |left: char, mid: char, right: char, horiz: char| {
                            let mut s = String::new();
                            s.push(left);
                            for i in 0..cols {
                                let seg = eff_widths[i] + CELL_PAD * 2;
                                for _ in 0..seg.max(3) { s.push(horiz); }
                                if i + 1 < cols { s.push(mid); } else { s.push(right); }
                            }
                            s
                        };

                        let top_border = make_border('┌', '┬', '┐', '─');
                        let header_sep = make_border('╞', '╪', '╡', '═');
                        let row_sep = make_border('├', '┼', '┤', '─');
                        let bottom_border = make_border('└', '┴', '┘', '─');

                        let truncate_to = |s: &str, width: usize| -> String {
                            let w = UnicodeWidthStr::width(s);
                            if w <= width { return s.to_string(); }
                            if width == 0 { return String::new(); }
                            let target = width.saturating_sub(1);
                            let mut acc = String::new();
                            let mut used = 0usize;
                            for ch in s.chars() {
                                let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                                if used + cw > target { break; }
                                acc.push(ch);
                                used += cw;
                            }
                            acc.push('…');
                            acc
                        };

                        let mut table_lines: Vec<(String, &'static str)> = Vec::new();

                        table_lines.push((top_border.clone(), "border"));
                        for r in &header_rows {
                            let mut line = String::new();
                            line.push('│');
                            for i in 0..cols {
                                let raw = r.get(i).map(|s| s.as_str()).unwrap_or("");
                                let s = truncate_to(raw, eff_widths[i]);
                                let align = table_alignments.get(i).cloned().unwrap_or(Alignment::Left);
                                for _ in 0..CELL_PAD { line.push(' '); }
                                line.push_str(&pad_cell(&s, eff_widths[i], align));
                                for _ in 0..CELL_PAD { line.push(' '); }
                                line.push('│');
                            }
                            table_lines.push((line, "header"));
                        }

                        if !header_rows.is_empty() {
                            table_lines.push((header_sep.clone(), "border-strong"));
                        }

                        for (idx, r) in body_rows.iter().enumerate() {
                            let mut line = String::new();
                            line.push('│');
                            for i in 0..cols {
                                let raw = r.get(i).map(|s| s.as_str()).unwrap_or("");
                                let s = truncate_to(raw, eff_widths[i]);
                                let align = table_alignments.get(i).cloned().unwrap_or(Alignment::Left);
                                for _ in 0..CELL_PAD { line.push(' '); }
                                line.push_str(&pad_cell(&s, eff_widths[i], align));
                                for _ in 0..CELL_PAD { line.push(' '); }
                                line.push('│');
                            }
                            table_lines.push((line, "body"));
                            if idx + 1 < body_rows.len() { table_lines.push((row_sep.clone(), "border")); }
                        }

                        table_lines.push((bottom_border.clone(), "border"));

                        let mut table_spans = Vec::new();
                        for (line, kind) in table_lines {
                            let (color, size) = match kind {
                                "border-strong" => (Color::from_rgb(0.65, 0.70, 0.88), 14),
                                "border" => (Color::from_rgb(0.70, 0.75, 0.90), 14),
                                // No special styling for header text; match body rows
                                "header" => (Color::from_rgb(0.92, 0.92, 1.00), 14),
                                _ => (Color::from_rgb(0.92, 0.92, 1.00), 14),
                            };
                            table_spans.push(
                                span(format!("{}\n", line)).font(Font::MONOSPACE).size(size).color(color)
                            );
                        }

                        blocks.push(container(rich_text(table_spans)).padding(4).width(Length::Fill).into());

                        // Reset table state
                        in_table = false;
                        in_table_head = false;
                        header_rows.clear();
                        body_rows.clear();
                        table_alignments.clear();
                    }
                    Tag::TableHead => {
                        // Ensure any pending header row/cell is captured
                        if !current_cell.is_empty() {
                            current_row.push(current_cell.clone());
                            current_cell.clear();
                        }
                        if !current_row.is_empty() {
                            header_rows.push(current_row.clone());
                            current_row.clear();
                        }
                        in_table_head = false; // end of header section
                    }
                    Tag::TableRow => {
                        if !current_cell.is_empty() {
                            current_row.push(current_cell.clone());
                            current_cell.clear();
                        }
                        if !current_row.is_empty() {
                            if in_table_head {
                                header_rows.push(current_row.clone());
                            } else {
                                body_rows.push(current_row.clone());
                            }
                            current_row.clear();
                        }
                    }
                    Tag::TableCell => {
                        if !current_cell.is_empty() {
                            current_row.push(current_cell.clone());
                            current_cell.clear();
                        }
                    }
                    Tag::Heading(_, _, _) => {
                        if !current_text.is_empty() {
                            let size = match heading_level {
                                Some(HeadingLevel::H1) => 28,
                                Some(HeadingLevel::H2) => 24,
                                Some(HeadingLevel::H3) => 20,
                                Some(HeadingLevel::H4) => 18,
                                Some(HeadingLevel::H5) => 16,
                                _ => 15,
                            };
                            spans.push(
                                span(current_text.clone())
                                    .size(size)
                                    .color(Color::from_rgb(0.6, 0.8, 1.0))
                            );
                            current_text.clear();
                        }
                        heading_level = None;
                        spans.push(span("\n\n"));
                    }
                    Tag::CodeBlock(_) => {
                        if !code_block_content.is_empty() {
                            spans.push(span("\n"));
                            spans.push(
                                span(code_block_content.clone())
                                    .font(Font::MONOSPACE)
                                    .size(14)
                                    .color(Color::from_rgb(0.8, 0.9, 0.8))
                            );
                            spans.push(span("\n\n"));
                            code_block_content.clear();
                        }
                        in_code_block = false;
                    }
                    Tag::Strong => {
                        // Flush bold text when exiting bold
                        if !current_text.is_empty() {
                            spans.push(
                                span(current_text.clone())
                                    .size(15)
                                    .color(Color::from_rgb(1.0, 1.0, 1.0))
                            );
                            current_text.clear();
                        }
                        in_bold = false;
                    }
                    Tag::Emphasis => {
                        _in_italic = false;
                    }
                    Tag::Paragraph => {
                        if !current_text.is_empty() {
                            let mut text_span = span(current_text.clone()).size(15);
                            if in_bold {
                                text_span = text_span.color(Color::from_rgb(1.0, 1.0, 1.0));
                            }
                            spans.push(text_span);
                            current_text.clear();
                        }
                        // Only add newlines if not in a list
                        if !in_list {
                            spans.push(span("\n\n"));
                        }
                    }
                    Tag::List(_) => {
                        // Add spacing after list only for top-level lists
                        in_list = false;
                    }
                    Tag::Item => {
                        // Add newline after each list item
                        if !current_text.is_empty() {
                            let mut text_span = span(current_text.clone()).size(15);
                            if in_bold {
                                text_span = text_span.color(Color::from_rgb(1.0, 1.0, 1.0));
                            }
                            spans.push(text_span);
                            current_text.clear();
                        }
                        spans.push(span("\n"));
                    }
                    _ => {}
                }
            }
            MarkdownEvent::Text(t) => {
                if in_table {
                    current_cell.push_str(&t);
                } else if in_code_block {
                    code_block_content.push_str(&t);
                } else {
                    current_text.push_str(&t);
                }
            }
            MarkdownEvent::Code(code) => {
                // Flush current text
                if in_table {
                    current_cell.push_str(&format!("`{}`", code));
                } else {
                    if !current_text.is_empty() {
                        spans.push(span(current_text.clone()));
                        current_text.clear();
                    }
                    // Add inline code
                    spans.push(
                        span(format!("`{}`", code))
                            .font(Font::MONOSPACE)
                            .size(14)
                            .color(Color::from_rgb(0.9, 0.6, 0.6))
                    );
                }
            }
            MarkdownEvent::SoftBreak => {
                if in_table { current_cell.push(' '); } else { current_text.push(' '); }
            }
            MarkdownEvent::HardBreak => {
                if in_table { current_cell.push('\n'); } else { current_text.push('\n'); }
            }
            _ => {}
        }
    }

    // Add any remaining text
    if !current_text.is_empty() {
        spans.push(span(current_text));
    }
    if !code_block_content.is_empty() {
        spans.push(
            span(code_block_content)
                .font(Font::MONOSPACE)
                .size(14)
                .color(Color::from_rgb(0.8, 0.9, 0.8))
        );
    }

    if !blocks.is_empty() {
        flush_spans(&mut spans, &mut blocks);
        return column(blocks).spacing(6).into();
    }

    if spans.is_empty() {
        text("").into()
    } else {
        rich_text(spans).width(Length::Fill).into()
    }
}

fn main() -> iced::Result {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let screenshot_mode = args.iter().any(|arg| arg == "--screenshot" || arg == "-screenshot");
    let debug_mode = args.iter().any(|arg| arg == "--debug" || arg == "-debug");

    // Set debug mode globally
    DEBUG_MODE.store(debug_mode, Ordering::Relaxed);
    if debug_mode {
        std::env::set_var("BOBBAR_DEBUG", "1");
    }

    // Get screen dimensions to calculate center
    let config = config::Config::load();

    if screenshot_mode {
        // Run in screenshot mode
        run_screenshot_mode(config)
    } else {
        // Normal mode
        iced::application("bob-bar", App::update, App::view)
            .theme(App::theme)
            .subscription(App::subscription)
            .window(window::Settings {
                size: iced::Size::new(1200.0, 1200.0),
                position: window::Position::Centered,
                // Use a normal window level so it does not stay above others
                level: Level::Normal,
                decorations: true,
                resizable: true,
                platform_specific: PlatformSpecific {
                    ..Default::default()
                },
                ..Default::default()
            })
            .default_font(Font::MONOSPACE)
            .run_with(App::new)
    }
}

fn run_screenshot_mode(_config: config::Config) -> iced::Result {
    iced::application("bob-bar", App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(window::Settings {
            size: iced::Size::new(1200.0, 1200.0),
            position: window::Position::Centered,
            // Screenshot mode should also not force always-on-top
            level: Level::Normal,
            decorations: true,
            resizable: true,
            platform_specific: PlatformSpecific {
                ..Default::default()
            },
            ..Default::default()
        })
        .default_font(Font::MONOSPACE)
        .run_with(|| {
            let (mut app, task) = App::new();
            app.screenshot_mode = true;

            // Capture screenshot after a small delay to allow window to be hidden
            let screenshot_task = Task::future(async {
                // Small delay to let the app window minimize/hide
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;

                match screenshot::capture_screenshot() {
                    Ok(path) => Message::ScreenshotCaptured(Ok(path)),
                    Err(e) => Message::ScreenshotCaptured(Err(e.to_string())),
                }
            });

            (app, Task::batch([task, screenshot_task]))
        })
}

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    Submit,
    NewQuery,
    ResponseReceived(String),
    Error(String),
    Tick,
    CopyOutput,
    Exit,
    ScreenshotCaptured(Result<std::path::PathBuf, String>),
    ToggleFullscreen,
    HistorySelect(usize),
    HistoryDelete(usize),
    ToggleSelectMode,
    OutputEditorAction(text_editor::Action),
    ToggleResearchMode,
    #[allow(dead_code)]
    ResearchProgress(research::ResearchProgress),
    CancelQuery,
}

struct App {
    input_text: String,
    response_text: String,
    streaming_text: String,
    is_loading: bool,
    is_fullscreen: bool,
    loading_frame: usize,
    ollama_client: Arc<Mutex<ollama::OllamaClient>>,
    input_id: Id,
    screenshot_mode: bool,
    screenshot_path: Option<std::path::PathBuf>,
    vision_model: String,
    history: Vec<history::HistoryEntry>,
    selected_history: Option<usize>,
    select_mode: bool,
    output_editor: text_editor::Content,
    research_mode: bool,
    research_orchestrator: Option<Arc<Mutex<research::ResearchOrchestrator>>>,
    research_progress: Option<String>,
    research_start_time: Option<std::time::Instant>,
    current_query_cancel: Option<tokio_util::sync::CancellationToken>,
}

impl App {
    fn new() -> (Self, Task<Message>) {
        // Load config
        let config = config::Config::load();

        // Load tools from config directory
        let tools_path = config::Config::get_config_dir().join("tools.json");
        let tool_executor = if tools_path.exists() {
            match tools::ToolExecutor::from_file(&tools_path) {
                Ok(executor) => {
                    // Print tool configuration on startup (only in debug mode)
                    if DEBUG_MODE.load(Ordering::Relaxed) {
                        eprintln!("=== Tool Configuration ===");
                        eprintln!("Loaded from: {}", tools_path.display());

                        // Print HTTP tools
                        if !executor.config.tools.http.is_empty() {
                            eprintln!("\nHTTP Tools ({}):", executor.config.tools.http.len());
                            for tool in &executor.config.tools.http {
                                eprintln!("  • {} - {}", tool.name, tool.description);
                            }
                        }

                        // Print MCP tools
                        if !executor.config.tools.mcp.is_empty() {
                            eprintln!("\nMCP Servers ({}):", executor.config.tools.mcp.len());
                            for server in &executor.config.tools.mcp {
                                eprintln!("  • {} - {}", server.name, server.command);
                            }
                        }
                        eprintln!("==========================\n");
                    }

                    let executor_arc = Arc::new(Mutex::new(executor));

                    // Initialize MCP servers in background
                    let executor_clone = executor_arc.clone();
                    std::thread::spawn(move || {
                        tokio::runtime::Runtime::new()
                            .expect("Failed to create Tokio runtime")
                            .block_on(async {
                                let exec = executor_clone.lock().await;
                                if let Err(e) = exec.initialize_mcp_servers().await {
                                    eprintln!("Warning: Failed to initialize MCP servers: {}", e);
                                }
                            });
                    });

                    Some(executor_arc)
                }
                Err(e) => {
                    eprintln!("Warning: Could not load tools config: {}", e);
                    None
                }
            }
        } else {
            None
        };

        // Clone ollama config early for research orchestrator
        let ollama_config = config.ollama.clone();

        // Create Ollama client
        let mut ollama_client = ollama::OllamaClient::with_config(
            config.ollama.host,
            config.ollama.model.clone(),
        );
        ollama_client.set_max_tool_turns(config.ollama.max_tool_turns);
        ollama_client.set_summarization_config(
            config.ollama.summarization_model.clone(),
            config.ollama.summarization_threshold,
            false // Not research mode for main client
        );

        let tool_executor_clone = if let Some(ref executor) = tool_executor {
            ollama_client.set_tool_executor(executor.clone());
            Some(executor.clone())
        } else {
            None
        };

        let input_id = Id::unique();

        let vision_model = config.ollama.vision_model.clone();

        let ollama_client_arc = Arc::new(Mutex::new(ollama_client));

        // Initialize research orchestrator
        let agents_path = config::Config::get_config_dir().join("agents.json");
        let research_model = config.ollama.research_model.clone()
            .unwrap_or_else(|| config.ollama.model.clone());

        let research_orchestrator = if agents_path.exists() {
            match research::ResearchOrchestrator::from_file(
                &agents_path,
                ollama_config,
                ollama_client_arc.clone(),
                config.ollama.context_window,
                research_model.clone(),
                config.ollama.max_tool_turns
            ) {
                Ok(mut orchestrator) => {
                    // Override with config.toml settings
                    orchestrator.override_config(&config.research);

                    if let Some(executor) = tool_executor_clone {
                        orchestrator.set_tool_executor(executor);
                    }
                    if DEBUG_MODE.load(Ordering::Relaxed) {
                        eprintln!("=== Research Mode ===");
                        eprintln!("Research orchestrator initialized from: {}", agents_path.display());
                        eprintln!("Research model: {}", research_model);
                        eprintln!("Context window: {} tokens", config.ollama.context_window);
                        eprintln!("Max refinement iterations: {}", config.ollama.max_refinement_iterations);
                        eprintln!("Max debate rounds: {}", config.ollama.max_debate_rounds);
                        eprintln!("Worker count range: {}-{}", config.research.min_worker_count, config.research.max_worker_count);
                        eprintln!("=====================\n");
                    }
                    Some(Arc::new(Mutex::new(orchestrator)))
                }
                Err(e) => {
                    eprintln!("Warning: Could not load research config: {}", e);
                    None
                }
            }
        } else {
            eprintln!("Info: agents.json not found. Research mode will be unavailable.");
            None
        };

        let app = App {
            input_text: String::new(),
            response_text: String::new(),
            streaming_text: String::new(),
            is_loading: false,
            is_fullscreen: false,
            loading_frame: 0,
            ollama_client: ollama_client_arc,
            input_id: input_id.clone(),
            screenshot_mode: false,
            screenshot_path: None,
            vision_model,
            history: {
                let _ = history::init();
                history::list_entries(100).unwrap_or_default()
            },
            selected_history: None,
            select_mode: false,
            output_editor: text_editor::Content::with_text(""),
            research_mode: false,
            research_orchestrator,
            research_progress: None,
            research_start_time: None,
            current_query_cancel: None,
        };

        let focus_task = text_input::focus(input_id);
        // Do not force always-on-top; keep normal stacking behavior
        (app, Task::batch([focus_task]))
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::InputChanged(value) => {
                self.input_text = value;
                Task::none()
            }
            Message::Submit => {
                if self.input_text.trim().is_empty() || self.is_loading {
                    return Task::none();
                }

                self.is_loading = true;
                self.response_text = String::new();
                self.streaming_text = String::new();

                if ENABLE_NOTIFICATIONS {
                    std::thread::spawn(|| {
                        let _ = Notification::new()
                            .summary("bob-bar")
                            .body("Processing your query...")
                            .show();
                    });
                }

                // Check if research mode is enabled
                if self.research_mode && self.research_orchestrator.is_some() {
                    self.research_start_time = Some(std::time::Instant::now());
                    self.research_progress = Some("Starting research...".to_string());
                    if let Ok(mut g) = RESEARCH_PROGRESS_GLOBAL.lock() {
                        *g = Some("Starting research...".to_string());
                    }

                    // Create cancellation token for this query
                    let cancel_token = tokio_util::sync::CancellationToken::new();
                    self.current_query_cancel = Some(cancel_token.clone());

                    let query = self.input_text.clone();
                    let orchestrator = self.research_orchestrator.clone().unwrap();

                    // For now, research progress updates are visible in terminal via eprintln
                    // Future enhancement: implement subscription-based progress streaming to UI
                    use tokio::sync::mpsc;
                    let (progress_tx, mut progress_rx) = mpsc::unbounded_channel();

                    // Spawn task to monitor progress and print to terminal
                    tokio::spawn(async move {
                        use research::ResearchProgress;
                        while let Some(progress) = progress_rx.recv().await {
                            let msg = match progress {
                                ResearchProgress::Started => "🚀 Starting research...".to_string(),
                                ResearchProgress::Decomposing => "🔍 Decomposing query into sub-questions...".to_string(),
                                ResearchProgress::PlanningIteration(i, max) => format!("📋 Planning iteration {}/{}", i, max),
                                ResearchProgress::PlanGenerated(n) => format!("✓ Generated plan with {} sub-questions", n),
                                ResearchProgress::PlanCriticReviewing(i, max) => format!("🔎 Plan critic reviewing (iteration {}/{})", i, max),
                                ResearchProgress::PlanApproved => "✅ Plan approved, starting research".to_string(),
                                ResearchProgress::WorkersStarted(n) => format!("👥 Dispatching {} research workers...", n),
                                ResearchProgress::WorkerStarted { worker, question } => format!("→ {}: researching — {}", worker, question),
                                ResearchProgress::WorkerStatus { worker, status } => format!("  {}: {}", worker, status),
                                ResearchProgress::WorkerCompleted(name) => format!("✓ {} completed", name),
                                ResearchProgress::SupervisorAnalyzing => "👁️ Supervisor analyzing progress...".to_string(),
                                ResearchProgress::FollowUpQuestionsGenerated(n) => format!("📝 Generated {} follow-up questions", n),
                                ResearchProgress::Combining => "🔗 Combining research results...".to_string(),
                                ResearchProgress::Summarizing => "📊 Summarizing worker results...".to_string(),
                                ResearchProgress::CriticReviewing => "🔎 Critic reviewing output...".to_string(),
                                ResearchProgress::DebateRound(current, max) => format!("💬 Debate round {}/{} in progress...", current, max),
                                ResearchProgress::Refining(current, max) => format!("✨ Refining output (iteration {}/{})", current, max),
                                ResearchProgress::WritingDocument(current, max) => format!("✍️ Writing document (iteration {}/{})", current, max),
                                ResearchProgress::DocumentReviewing => "📝 Document critic reviewing...".to_string(),
                                ResearchProgress::ExportingMemories => "💾 Exporting research memories...".to_string(),
                                ResearchProgress::Completed => "🎉 Research complete!".to_string(),
                            };
                            if let Ok(mut g) = RESEARCH_PROGRESS_GLOBAL.lock() {
                                *g = Some(msg);
                            }
                        }
                    });

                    // Run the research task with cancellation support
                    Task::perform(
                        async move {
                            tokio::select! {
                                result = async {
                                    let mut orch = orchestrator.lock().await;
                                    orch.set_progress_channel(progress_tx);
                                    orch.research(&query).await
                                } => result,
                                _ = cancel_token.cancelled() => {
                                    Err(anyhow::anyhow!("Query cancelled by user"))
                                }
                            }
                        },
                        |result| match result {
                            Ok(response) => Message::ResponseReceived(response),
                            Err(e) => Message::Error(format!("Research error: {}", e)),
                        }
                    )
                } else {
                    // Normal mode
                    let prompt = format!("{}\n\n{}", TABLE_PLAIN_TEXT_RULES, self.input_text.clone());
                    let client = self.ollama_client.clone();

                    // Use Iced's Task system to run async work non-blocking
                    // This spawns the async work on Iced's tokio runtime thread pool,
                    // keeping the main GUI thread responsive to Wayland events
                    Task::perform(
                        async move {
                            let mut client_guard = client.lock().await;
                            client_guard.query_streaming(&prompt, |_text| {
                                // For now, we'll skip streaming updates to keep it simple
                                // We could implement this with a subscription if needed
                            }).await
                        },
                        |result| match result {
                            Ok(response) => Message::ResponseReceived(response),
                            Err(e) => Message::Error(format!("Error: {}", e)),
                        }
                    )
                }
            }
            Message::NewQuery => {
                if self.is_loading || self.research_progress.is_some() { return Task::none(); }
                self.input_text.clear();
                self.response_text.clear();
                self.streaming_text.clear();
                self.screenshot_path = None;
                self.selected_history = None;
                self.output_editor = text_editor::Content::with_text("");
                self.research_progress = None;
                crate::tools::clear_current_sources();
                crate::progress::clear();
                Task::none()
            }
            Message::ResponseReceived(response) => {
                self.response_text = response;
                self.streaming_text = String::new();
                self.is_loading = false;
                self.research_progress = None;
                self.research_start_time = None;
                self.current_query_cancel = None;  // Clear cancellation token
                if let Ok(mut g) = RESEARCH_PROGRESS_GLOBAL.lock() { *g = None; }
                crate::tools::clear_current_sources();
                crate::progress::clear();
                
                if ENABLE_NOTIFICATIONS {
                    std::thread::spawn(|| {
                        // On Linux, set urgency and timeout; other OSes may not support these
                        #[cfg(target_os = "linux")]
                        {
                            let _ = Notification::new()
                                .summary("bob-bar")
                                .body("Query complete! Click to view results.")
                                .urgency(notify_rust::Urgency::Normal)
                                .timeout(notify_rust::Timeout::Milliseconds(5000))
                                .show();
                        }

                        #[cfg(not(target_os = "linux"))]
                        {
                            let _ = Notification::new()
                                .summary("bob-bar")
                                .body("Query complete! Click to view results.")
                                .show();
                        }
                    });
                }

                // Save to history and refresh list
                let _ = history::add_entry(&self.input_text, &self.response_text);
                self.history = history::list_entries(100).unwrap_or_default();

                // Also request window focus immediately
                window::get_latest().and_then(|id| window::gain_focus(id))
            }
            Message::Error(error) => {
                self.response_text = error;
                self.streaming_text = String::new();
                self.is_loading = false;
                self.current_query_cancel = None;  // Clear cancellation token
                if let Ok(mut g) = RESEARCH_PROGRESS_GLOBAL.lock() { *g = None; }
                crate::tools::clear_current_sources();
                crate::progress::clear();
                Task::none()
            }
            Message::CancelQuery => {
                // Cancel the current query/research if one is running
                if let Some(cancel_token) = &self.current_query_cancel {
                    cancel_token.cancel();
                    self.response_text = "Query cancelled by user".to_string();
                    self.streaming_text = String::new();
                    self.is_loading = false;
                    self.research_progress = None;
                    self.research_start_time = None;
                    self.current_query_cancel = None;
                    if let Ok(mut g) = RESEARCH_PROGRESS_GLOBAL.lock() { *g = None; }
                    crate::tools::clear_current_sources();
                    crate::progress::clear();
                }
                Task::none()
            }
            Message::Tick => {
                if self.is_loading {
                    self.loading_frame = (self.loading_frame + 1) % 80; // 10 frames * 8 messages
                    // Pull latest research progress from global store
                    if let Ok(g) = RESEARCH_PROGRESS_GLOBAL.lock() {
                        if let Some(ref s) = *g {
                            self.research_progress = Some(s.clone());
                        }
                    }
                }
                Task::none()
            }
            Message::HistorySelect(idx) => {
                if let Some(entry) = self.history.get(idx).cloned() {
                    self.input_text = entry.prompt;
                    self.response_text = entry.response;
                    self.selected_history = Some(idx);
                    self.is_loading = false;
                }
                Task::none()
            }
            Message::OutputEditorAction(action) => {
                // Allow selection and navigation; if the user types, it will edit the ephemeral view only
                self.output_editor.perform(action);
                Task::none()
            }
            Message::ToggleSelectMode => {
                self.select_mode = !self.select_mode;
                if self.select_mode {
                    self.output_editor = text_editor::Content::with_text(&self.response_text);
                }
                Task::none()
            }
            Message::ToggleResearchMode => {
                if self.research_orchestrator.is_some() {
                    self.research_mode = !self.research_mode;
                }
                Task::none()
            }
            Message::ResearchProgress(progress) => {
                use research::ResearchProgress;

                let progress_text = match progress {
                    ResearchProgress::Started => "🚀 Starting research...".to_string(),
                    ResearchProgress::Decomposing => "🔍 Decomposing query into sub-questions...".to_string(),
                    ResearchProgress::PlanningIteration(i, max) => format!("📋 Planning iteration {}/{}", i, max),
                    ResearchProgress::PlanGenerated(n) => format!("✓ Generated plan with {} sub-questions", n),
                    ResearchProgress::PlanCriticReviewing(i, max) => format!("🔎 Plan critic reviewing (iteration {}/{})", i, max),
                    ResearchProgress::PlanApproved => "✅ Plan approved, starting research".to_string(),
                    ResearchProgress::WorkersStarted(n) => format!("👥 Dispatching {} research workers...", n),
                    ResearchProgress::WorkerStarted { worker, question } => format!("→ {}: researching — {}", worker, question),
                    ResearchProgress::WorkerStatus { worker, status } => format!("  {}: {}", worker, status),
                    ResearchProgress::WorkerCompleted(name) => format!("✓ {} completed", name),
                    ResearchProgress::SupervisorAnalyzing => "👁️ Supervisor analyzing progress...".to_string(),
                    ResearchProgress::FollowUpQuestionsGenerated(n) => format!("📝 Generated {} follow-up questions", n),
                    ResearchProgress::Combining => "🔗 Combining research results...".to_string(),
                    ResearchProgress::Summarizing => "📊 Summarizing worker results...".to_string(),
                    ResearchProgress::CriticReviewing => "🔎 Critic reviewing output...".to_string(),
                    ResearchProgress::DebateRound(current, max) => format!("💬 Debate round {}/{} in progress...", current, max),
                    ResearchProgress::Refining(current, max) => format!("✨ Refining output (iteration {}/{})", current, max),
                    ResearchProgress::WritingDocument(current, max) => format!("✍️ Writing document (iteration {}/{})", current, max),
                    ResearchProgress::DocumentReviewing => "📝 Document critic reviewing...".to_string(),
                    ResearchProgress::ExportingMemories => "💾 Exporting research memories...".to_string(),
                    ResearchProgress::Completed => "🎉 Research complete!".to_string(),
                };

                self.research_progress = Some(progress_text);
                Task::none()
            }
            Message::HistoryDelete(idx) => {
                if let Some(entry) = self.history.get(idx) {
                    let _ = history::delete_entry(entry.id);
                }
                self.history = history::list_entries(100).unwrap_or_default();
                if let Some(sel) = self.selected_history {
                    if sel == idx || sel >= self.history.len() {
                        self.selected_history = None;
                    }
                }
                Task::none()
            }
            Message::CopyOutput => {
                clipboard::write(self.response_text.clone())
            }
            Message::Exit => {
                // If a query is running, cancel it instead of exiting
                if self.is_loading || self.current_query_cancel.is_some() {
                    return self.update(Message::CancelQuery);
                }
                iced::exit()
            }
            Message::ToggleFullscreen => {
                // Toggle true fullscreen mode using iced window API
                let new_mode = if self.is_fullscreen {
                    window::Mode::Windowed
                } else {
                    window::Mode::Fullscreen
                };
                self.is_fullscreen = !self.is_fullscreen;
                window::get_latest().and_then(move |id| window::change_mode(id, new_mode))
            }
            Message::ScreenshotCaptured(result) => {
                match result {
                    Ok(path) => {
                        self.screenshot_path = Some(path.clone());
                        self.is_loading = true;
                        self.response_text = "Extracting information from screenshot...".to_string();
                        self.input_text = "Reading and analyzing screen content...".to_string();

                        let client = self.ollama_client.clone();
                        let screenshot_path = path;
                        let vision_model = self.vision_model.clone();

                        Task::future(async move {
                            let mut client = client.lock().await;

                            // Temporarily switch to vision model
                            let original_model = client.get_model().to_string();
                            client.set_model(vision_model);

                            // Encode image as base64
                            let result = match screenshot::encode_image_base64(&screenshot_path) {
                                Ok(base64_image) => {
                                    client.query_with_image(
                                        "You are analyzing a screenshot. Your task is to extract and report ONLY what you can directly see and read.

**CRITICAL RULES:**
- ONLY report text, numbers, and visual elements you can actually see in the image
- DO NOT guess, infer, or make assumptions about anything not clearly visible
- DO NOT explain what code does unless you can see comments or documentation explaining it
- DO NOT suggest fixes unless error messages explicitly state the solution
- If text is unclear or partially visible, state \"text unclear\" rather than guessing
- If you cannot see something clearly, say \"not visible in screenshot\"

**What to extract:**
1. **Visible text** - Transcribe exactly what you see: error messages, button labels, terminal output, code
2. **Visible numbers** - Version numbers, error codes, line numbers, timestamps
3. **Visible UI elements** - Application name (if shown), window titles, menu items
4. **Visible structure** - File paths, URLs, command names (only if clearly visible)

**Formatting rules:**
- When including tables in your response, do not apply styling to table headers or table cell values. Use plain text inside tables (no bold/italic/code inside table cells).
- Do not use Unicode symbols or emoji inside tables; use plain ASCII text only (letters, numbers, basic punctuation).

**Format your response as:**
- **Text Content:** [Quote exact text you see]
- **Key Information:** [Only concrete data visible: error codes, versions, paths]
- **Visual Context:** [What application/interface is shown, if identifiable]
- **Notable Elements:** [Any important UI elements or indicators you see]

Remember: Only report what is objectively visible. Do not interpret, explain, or suggest unless the image itself contains that information.",
                                        &base64_image
                                    ).await
                                }
                                Err(e) => Err(anyhow::anyhow!("Error encoding image: {}", e)),
                            };

                            // Restore original model
                            client.set_model(original_model);

                            match result {
                                Ok(response) => Message::ResponseReceived(response),
                                Err(e) => Message::Error(format!("Error analyzing screenshot: {}", e)),
                            }
                        })
                    }
                    Err(e) => {
                        self.response_text = format!("Error capturing screenshot: {}", e);
                        Task::none()
                    }
                }
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let timer = if self.is_loading || self.research_progress.is_some() {
            time::every(Duration::from_millis(200)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        };

        let events = event::listen_with(|event, _status, _id| {
            match event {
                IcedEvent::Keyboard(keyboard::Event::KeyPressed { key: Key::Named(keyboard::key::Named::Escape), .. }) => {
                    Some(Message::Exit)
                }
                IcedEvent::Keyboard(keyboard::Event::KeyPressed { key: Key::Character(c), modifiers, .. }) => {
                    // macOS-style fullscreen shortcut: Cmd + Ctrl + F
                    // Use `logo()` to represent Command on macOS
                    if (c == "f" || c == "F") && modifiers.control() && modifiers.logo() {
                        Some(Message::ToggleFullscreen)
                    } else if (c == "n" || c == "N") && (modifiers.logo() || modifiers.control()) {
                        Some(Message::NewQuery)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        });

        Subscription::batch([timer, events])
    }

    fn view(&self) -> Element<'_, Message> {
        let mut input = text_input("Type your message...", &self.input_text)
            .padding(15)
            .size(18)
            .id(self.input_id.clone());

        // Only enable input when not loading
        if !self.is_loading {
            input = input
                .on_input(Message::InputChanged)
                .on_submit(Message::Submit);
        }

        // Research mode toggle button (only show if orchestrator is available)
        let research_toggle = if self.research_orchestrator.is_some() {
            let toggle_text = if self.research_mode {
                "Research: ON "  // Extra space to match OFF length
            } else {
                "Research: OFF"
            };

            let toggle_btn = if self.is_loading {
                button(
                    container(text(toggle_text).size(14))
                        .align_x(alignment::Horizontal::Center)
                        .align_y(alignment::Vertical::Center)
                        .width(Length::Fixed(110.0))
                        .height(Length::Fill)
                )
                .padding([8, 12])
                .height(Length::Fixed(INPUT_HEIGHT))
            } else {
                button(
                    container(text(toggle_text).size(14))
                        .align_x(alignment::Horizontal::Center)
                        .align_y(alignment::Vertical::Center)
                        .width(Length::Fixed(110.0))
                        .height(Length::Fill)
                )
                .on_press(Message::ToggleResearchMode)
                .padding([8, 12])
                .height(Length::Fixed(INPUT_HEIGHT))
            };

            Some(toggle_btn)
        } else {
            None
        };

        // Mouse-friendly Enter + New buttons (center text vertically)
        let enter_label = container(text("Enter").size(16))
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .height(Length::Fill);
        let new_label = container(text("New").size(16))
            .align_x(alignment::Horizontal::Center)
            .align_y(alignment::Vertical::Center)
            .height(Length::Fill);

        let enter_btn = if self.is_loading || self.input_text.trim().is_empty() {
            button(enter_label)
                .padding([8, 12])
                .height(Length::Fixed(INPUT_HEIGHT))
        } else {
            button(enter_label)
                .on_press(Message::Submit)
                .padding([8, 12])
                .height(Length::Fixed(INPUT_HEIGHT))
        };
        let new_btn = if self.is_loading {
            button(new_label)
                .padding([8, 12])
                .height(Length::Fixed(INPUT_HEIGHT))
        } else {
            button(new_label)
                .on_press(Message::NewQuery)
                .padding([8, 12])
                .height(Length::Fixed(INPUT_HEIGHT))
        };

        // Create input row with optional research toggle and action buttons
        let input_row = if let Some(toggle) = research_toggle {
            row![
                container(input).width(Length::Fill),
                enter_btn,
                new_btn,
                toggle
            ]
            .spacing(8)
            .width(Length::Fill)
        } else {
            row![
                container(input).width(Length::Fill),
                enter_btn,
                new_btn
            ]
            .spacing(8)
            .width(Length::Fill)
        };

        let output: Element<Message> = if self.is_loading {
            // Show streaming text if available, otherwise show loading spinner
            if !self.streaming_text.is_empty() {
                scrollable(
                    container(render_markdown(self.streaming_text.clone()))
                        .padding(15)
                        .width(Length::Fill)
                )
                .direction(Direction::Vertical(Scrollbar::default()))
                .height(Length::Fill)
                .into()
            } else if let Some(ref progress_text) = self.research_progress {
                // Show research progress with elapsed time
                let elapsed = if let Some(start_time) = self.research_start_time {
                    let duration = start_time.elapsed();
                    format!("{}s", duration.as_secs())
                } else {
                    "0s".to_string()
                };

                // Fetch current sources list
                let sources = crate::tools::get_current_sources();
                let sources_view: Element<Message> = if sources.is_empty() {
                    text("").into()
                } else {
                    let mut col = column![text("Sources in progress:").size(14)];
                    for s in sources.iter().take(10) {
                        let host = extract_hostname(s);
                        col = col.push(text(host).size(13));
                    }
                    col.spacing(4).into()
                };

                // Recent activity lines (verbose progress)
                let recent = crate::progress::recent(8);
                let recent_view: Element<Message> = if recent.is_empty() {
                    text("").into()
                } else {
                    let mut col = column![text("Recent activity:").size(14)];
                    for entry in recent {
                        let color = match entry.kind {
                            crate::progress::Kind::Info => Color::from_rgb(0.75, 0.78, 0.90),
                            crate::progress::Kind::Http => Color::from_rgb(0.65, 0.70, 0.80),
                            crate::progress::Kind::Debate => Color::from_rgb(0.95, 0.70, 0.40),
                            crate::progress::Kind::Refiner => Color::from_rgb(0.45, 0.75, 1.0),
                            crate::progress::Kind::Writer => Color::from_rgb(0.55, 0.90, 0.55),
                            crate::progress::Kind::DocumentCritic => Color::from_rgb(0.90, 0.55, 0.85),
                            crate::progress::Kind::Combiner => Color::from_rgb(0.40, 0.85, 0.85),
                            crate::progress::Kind::Worker => Color::from_rgb(0.55, 0.85, 1.0),
                        };
                        let line = text(entry.text)
                            .size(13)
                            .style(move |_theme: &Theme| text_widget::Style{ color: Some(color), ..Default::default() });
                        col = col.push(line);
                    }
                    col.spacing(4).into()
                };

                container(
                    column![
                        text("🔬 Research Mode").size(24),
                        text(progress_text).size(18),
                        text(format!("Elapsed: {}", elapsed)).size(14),
                        sources_view,
                        recent_view
                    ]
                    .spacing(12)
                    .align_x(alignment::Horizontal::Center)
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center)
                .into()
            } else {
                // Show animated loading text with a more elegant ASCII spinner (bouncing star)
                let loading_frames = [
                    "[*    ]",
                    "[ *   ]",
                    "[  *  ]",
                    "[   * ]",
                    "[    *]",
                    "[   * ]",
                    "[  *  ]",
                    "[ *   ]",
                ];
                let loading_messages = if self.screenshot_mode {
                    [
                        "Reading visible text...",
                        "Extracting exact content...",
                        "Transcribing screen elements...",
                        "Identifying visible information...",
                        "Processing text and data...",
                        "Analyzing visible content...",
                        "Extracting concrete information...",
                        "Reading screen accurately...",
                    ]
                } else {
                    [
                        "Consulting the digital oracle...",
                        "Summoning AI wisdom...",
                        "Asking the machines nicely...",
                        "Brewing up an answer...",
                        "Thinking really hard...",
                        "Channeling silicon spirits...",
                        "Calculating probabilities...",
                        "Parsing the universe...",
                    ]
                };

                let message_idx = (self.loading_frame / 10) % loading_messages.len();
                let spinner_idx = self.loading_frame % loading_frames.len();

                container(
                    column![
                        text(loading_frames[spinner_idx])
                            .size(32),
                        text(loading_messages[message_idx])
                            .size(15)
                    ]
                    .spacing(10)
                    .align_x(alignment::Horizontal::Center)
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center)
                .into()
            }
        } else {
            if self.select_mode {
                scrollable(
                    container(text_editor(&self.output_editor)
                            .on_action(Message::OutputEditorAction)
                    )
                        .padding(15)
                        .width(Length::Fill)
                )
                .direction(Direction::Vertical(Scrollbar::default()))
                .height(Length::Fill)
                .into()
            } else if self.response_text.is_empty() {
                // Show centered welcome message when empty
                container(
                    column![
                        text("Ready").size(24),
                        text("Enter a query above to begin").size(14)
                    ]
                    .spacing(10)
                    .align_x(alignment::Horizontal::Center)
                )
                .width(Length::Fill)
                .height(Length::Fill)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center)
                .into()
            } else {
                scrollable(
                    container(render_markdown(self.response_text.clone()))
                        .padding(15)
                        .width(Length::Fill)
                )
                .direction(Direction::Vertical(Scrollbar::default()))
                .height(Length::Fill)
                .into()
            }
        };

        let mut content_column = column![input_row, output]
            .spacing(10)
            // Equal left/right padding (horizontal=3), vertical=10
            .padding(Padding::from([10, 3]));

        // Add action buttons at bottom right if we have output
        if !self.response_text.is_empty() && !self.is_loading {
            let actions = row![
                button(text(if self.select_mode { "[Done Selecting]" } else { "[Select Text]" }).size(14))
                    .on_press(Message::ToggleSelectMode)
                    .padding(8),
                button(text("[Copy]").size(14))
                    .on_press(Message::CopyOutput)
                    .padding(8)
            ]
            .spacing(8);

            let actions_row = container(actions)
                .width(Length::Fill)
                .align_x(alignment::Horizontal::Right)
                .padding(Padding::from([10, 10]));

            content_column = content_column.push(actions_row);
        }

        // Sidebar: History list (compact)
        let sidebar = {
            let mut items = column![]
                .spacing(4)
                .padding(Padding::from([10.0, 8.0]));

            // Center "History" and align its vertical center with the input height
            items = items.push(
                container(text("History").size(18))
                    .width(Length::Fill)
                    .height(Length::Fixed(INPUT_HEIGHT))
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center)
            );

            // Add spacer so the list starts just below the input's bottom
            items = items.push(Space::with_height(Length::Fixed(INPUT_HEIGHT / 2.0)));

            for (i, entry) in self.history.iter().enumerate() {
                let title = {
                    let s = &entry.prompt;
                    let mut it = s.chars();
                    let taken: String = it.by_ref().take(16).collect();
                    if it.next().is_some() { format!("{}...", taken) } else { s.clone() }
                };
                let select_btn = if self.is_loading {
                    button(
                        text(title)
                            .size(12)
                            .width(Length::Fill)
                    )
                    .padding(6)
                    .width(Length::Fill)
                } else {
                    button(
                        text(title)
                            .size(12)
                            .width(Length::Fill)
                    )
                    .on_press(Message::HistorySelect(i))
                    .padding(6)
                    .width(Length::Fill)
                };

                let delete_btn = if self.is_loading {
                    button(text("×").size(12))
                        .padding(6)
                } else {
                    button(text("×").size(12))
                        .on_press(Message::HistoryDelete(i))
                        .padding(6)
                };

                items = items.push(row![select_btn, delete_btn].spacing(4));
            }

            scrollable(container(items).width(Length::Fixed(180.0)))
                .width(Length::Fixed(200.0))
                .height(Length::Fill)
        };

        container(
            row![
                sidebar,
                container(content_column)
                    .width(Length::Fill)
                    .height(Length::Fill)
            ]
            .spacing(0)
        )
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        Theme::TokyoNight
    }
}
