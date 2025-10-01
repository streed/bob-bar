mod config;
mod ollama;
mod tools;
mod screenshot;

use iced::{
    widget::{column, container, scrollable, text, text_input, button, text_input::Id, rich_text, span},
    Element, Length, Task, Theme, Font, Subscription,
    time, clipboard,
    keyboard::{self, Key},
    event::{self, Event as IcedEvent},
    alignment, Padding,
    window::{self, Level},
    Color,
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use pulldown_cmark::{Parser, Event as MarkdownEvent, Tag, HeadingLevel};
use notify_rust::Notification;
use std::sync::atomic::{AtomicBool, Ordering};

static DEBUG_MODE: AtomicBool = AtomicBool::new(false);

fn render_markdown(markdown: String) -> Element<'static, Message> {
    let parser = Parser::new(&markdown);
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut in_code_block = false;
    let mut code_block_content = String::new();
    let mut in_bold = false;
    let mut in_italic = false;
    let mut heading_level: Option<HeadingLevel> = None;
    let mut in_list = false;

    for event in parser {
        match event {
            MarkdownEvent::Start(tag) => {
                match tag {
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
                        in_italic = true;
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
                        in_italic = false;
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
                if in_code_block {
                    code_block_content.push_str(&t);
                } else {
                    current_text.push_str(&t);
                }
            }
            MarkdownEvent::Code(code) => {
                // Flush current text
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
            MarkdownEvent::SoftBreak => {
                current_text.push(' ');
            }
            MarkdownEvent::HardBreak => {
                current_text.push('\n');
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

    if spans.is_empty() {
        text("").into()
    } else {
        rich_text(spans).into()
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
                size: iced::Size::new(config.window.width as f32, config.window.height as f32),
                position: window::Position::Centered,
                ..Default::default()
            })
            .default_font(Font::MONOSPACE)
            .run_with(App::new)
    }
}

fn run_screenshot_mode(config: config::Config) -> iced::Result {
    iced::application("bob-bar", App::update, App::view)
        .theme(App::theme)
        .subscription(App::subscription)
        .window(window::Settings {
            size: iced::Size::new(config.window.width as f32, config.window.height as f32),
            position: window::Position::Centered,
            ..Default::default()
        })
        .default_font(Font::MONOSPACE)
        .run_with(|| {
            let (mut app, mut task) = App::new();
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
    ResponseReceived(String),
    StreamingUpdate(String),
    Error(String),
    Tick,
    CopyOutput,
    Exit,
    ScreenshotCaptured(Result<std::path::PathBuf, String>),
}

struct App {
    input_text: String,
    response_text: String,
    streaming_text: String,
    is_loading: bool,
    loading_frame: usize,
    ollama_client: Arc<Mutex<ollama::OllamaClient>>,
    input_id: Id,
    screenshot_mode: bool,
    screenshot_path: Option<std::path::PathBuf>,
    vision_model: String,
    stream_receiver: Arc<Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Message>>>>,
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
                                let mut exec = executor_clone.lock().await;
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

        // Create Ollama client
        let mut ollama_client = ollama::OllamaClient::with_config(
            config.ollama.host,
            config.ollama.model,
        );

        if let Some(executor) = tool_executor {
            ollama_client.set_tool_executor(executor);
        }

        let input_id = Id::unique();

        let vision_model = config.ollama.vision_model.clone();

        let app = App {
            input_text: String::new(),
            response_text: String::new(),
            streaming_text: String::new(),
            is_loading: false,
            loading_frame: 0,
            ollama_client: Arc::new(Mutex::new(ollama_client)),
            input_id: input_id.clone(),
            screenshot_mode: false,
            screenshot_path: None,
            vision_model,
            stream_receiver: Arc::new(Mutex::new(None)),
        };

        let focus_task = text_input::focus(input_id);
        let window_task = window::get_latest()
            .and_then(|id| window::change_level(id, Level::AlwaysOnTop));

        (app, Task::batch([focus_task, window_task]))
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

                let prompt = self.input_text.clone();
                self.is_loading = true;
                self.response_text = String::new();
                self.streaming_text = String::new();

                // Send start notification
                let _ = Notification::new()
                    .summary("bob-bar")
                    .body("Processing your query...")
                    .show();

                let client = self.ollama_client.clone();
                let receiver = self.stream_receiver.clone();

                // Spawn immediately and don't wait
                tokio::spawn(async move {
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();

                    // Store the receiver for the subscription
                    *receiver.lock().await = Some(rx);

                    // Spawn the streaming task
                    tokio::spawn(async move {
                        let mut client = client.lock().await;
                        let result = client.query_streaming(&prompt, |text| {
                            let _ = tx.send(Message::StreamingUpdate(text));
                        }).await;

                        match result {
                            Ok(response) => {
                                let _ = tx.send(Message::ResponseReceived(response));
                            },
                            Err(e) => {
                                let _ = tx.send(Message::Error(format!("Error: {}", e)));
                            },
                        }
                    });
                });

                Task::none()
            }
            Message::StreamingUpdate(text) => {
                self.streaming_text = text;
                Task::none()
            }
            Message::ResponseReceived(response) => {
                self.response_text = response;
                self.streaming_text = String::new();
                self.is_loading = false;

                // Send completion notification
                tokio::spawn(async {
                    let result = Notification::new()
                        .summary("bob-bar")
                        .body("Query complete! Click to view results.")
                        .urgency(notify_rust::Urgency::Normal)
                        .timeout(notify_rust::Timeout::Milliseconds(5000))
                        .show();

                    if let Ok(handle) = result {
                        // Try to wait for click and focus window
                        tokio::task::spawn_blocking(move || {
                            handle.wait_for_action(|action| {
                                // Any action (including default click) should focus
                                let _ = std::process::Command::new("sh")
                                    .arg("-c")
                                    .arg("wmctrl -a bob-bar || xdotool search --name bob-bar windowactivate")
                                    .output();
                            });
                        });
                    }
                });

                // Also request window focus immediately
                window::get_latest().and_then(|id| window::gain_focus(id))
            }
            Message::Error(error) => {
                self.response_text = error;
                self.streaming_text = String::new();
                self.is_loading = false;
                Task::none()
            }
            Message::Tick => {
                if self.is_loading {
                    self.loading_frame = (self.loading_frame + 1) % 80; // 10 frames * 8 messages

                    // Check for streaming messages synchronously
                    let receiver = self.stream_receiver.clone();
                    return Task::future(async move {
                        let mut guard = receiver.lock().await;
                        if let Some(rx) = guard.as_mut() {
                            // Try to receive all available messages
                            while let Ok(msg) = rx.try_recv() {
                                // Return the first non-Tick message
                                match msg {
                                    Message::Tick => continue,
                                    _ => return msg,
                                }
                            }
                        }
                        Message::Tick
                    });
                }
                Task::none()
            }
            Message::CopyOutput => {
                clipboard::write(self.response_text.clone())
            }
            Message::Exit => {
                iced::exit()
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
        let timer = if self.is_loading {
            time::every(Duration::from_millis(50)).map(|_| Message::Tick)
        } else {
            Subscription::none()
        };

        let events = event::listen_with(|event, _status, _id| {
            if let IcedEvent::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(keyboard::key::Named::Escape),
                ..
            }) = event
            {
                Some(Message::Exit)
            } else {
                None
            }
        });

        Subscription::batch([timer, events])
    }

    fn view(&self) -> Element<Message> {
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

        let output: Element<Message> = if self.is_loading {
            // Show streaming text if available, otherwise show loading spinner
            if !self.streaming_text.is_empty() {
                scrollable(
                    container(render_markdown(self.streaming_text.clone()))
                        .padding(15)
                        .width(Length::Fill)
                )
                .height(Length::Fill)
                .into()
            } else {
                // Show animated loading text with fun messages using unicode spinner
                let loading_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
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
            scrollable(
                container(render_markdown(self.response_text.clone()))
                    .padding(15)
                    .width(Length::Fill)
            )
            .height(Length::Fill)
            .into()
        };

        let mut content_column = column![input, output]
            .spacing(10)
            .padding(10);

        // Add copy button at bottom right if we have output
        if !self.response_text.is_empty() && !self.is_loading {
            let copy_button = container(
                button(text("[Copy]").size(14))
                    .on_press(Message::CopyOutput)
                    .padding(10)
            )
            .width(Length::Fill)
            .align_x(alignment::Horizontal::Right)
            .padding(Padding::from([10, 10]));

            content_column = content_column.push(copy_button);
        }

        container(content_column)
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn theme(&self) -> Theme {
        Theme::TokyoNight
    }
}