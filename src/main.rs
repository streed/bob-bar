mod config;
mod ollama;
mod tools;
mod screenshot;

use iced::{
    widget::{column, container, scrollable, text, text_input, button, text_input::Id},
    Element, Length, Task, Theme, Font, Subscription,
    time, clipboard,
    keyboard::{self, Key},
    event::{self, Event as IcedEvent},
    alignment, Padding,
    window::{self, Level},
};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

fn render_markdown(markdown: String) -> Element<'static, Message> {
    // Just display the text as-is without parsing
    text(markdown)
        .size(15)
        .into()
}

fn main() -> iced::Result {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let screenshot_mode = args.iter().any(|arg| arg == "--screenshot" || arg == "-screenshot");

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
    Error(String),
    Tick,
    CopyOutput,
    Exit,
    ScreenshotCaptured(Result<std::path::PathBuf, String>),
}

struct App {
    input_text: String,
    response_text: String,
    is_loading: bool,
    loading_frame: usize,
    ollama_client: Arc<Mutex<ollama::OllamaClient>>,
    input_id: Id,
    screenshot_mode: bool,
    screenshot_path: Option<std::path::PathBuf>,
    vision_model: String,
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
            is_loading: false,
            loading_frame: 0,
            ollama_client: Arc::new(Mutex::new(ollama_client)),
            input_id: input_id.clone(),
            screenshot_mode: false,
            screenshot_path: None,
            vision_model,
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

                let client = self.ollama_client.clone();

                Task::future(async move {
                    let mut client = client.lock().await;
                    let result = client.query(&prompt).await;

                    match result {
                        Ok(response) => Message::ResponseReceived(response),
                        Err(e) => Message::Error(format!("Error: {}", e)),
                    }
                })
            }
            Message::ResponseReceived(response) => {
                self.response_text = response;
                self.is_loading = false;
                Task::none()
            }
            Message::Error(error) => {
                self.response_text = error;
                self.is_loading = false;
                Task::none()
            }
            Message::Tick => {
                if self.is_loading {
                    self.loading_frame = (self.loading_frame + 1) % 80; // 10 frames * 8 messages
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
            time::every(Duration::from_millis(80)).map(|_| Message::Tick)
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