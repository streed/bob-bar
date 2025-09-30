mod config;
mod ollama;
mod tools;

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
    // Get screen dimensions to calculate center
    let config = config::Config::load();

    iced::application("W-AI-Land", App::update, App::view)
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

#[derive(Debug, Clone)]
enum Message {
    InputChanged(String),
    Submit,
    ResponseReceived(String),
    Error(String),
    Tick,
    CopyOutput,
    Exit,
}

struct App {
    input_text: String,
    response_text: String,
    is_loading: bool,
    loading_frame: usize,
    ollama_client: Arc<Mutex<ollama::OllamaClient>>,
    input_id: Id,
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

        let app = App {
            input_text: String::new(),
            response_text: String::new(),
            is_loading: false,
            loading_frame: 0,
            ollama_client: Arc::new(Mutex::new(ollama_client)),
            input_id: input_id.clone(),
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
        let input = text_input("Type your message...", &self.input_text)
            .on_input(Message::InputChanged)
            .on_submit(Message::Submit)
            .padding(15)
            .size(18)
            .id(self.input_id.clone());

        let output: Element<Message> = if self.is_loading {
            // Show animated loading text with fun messages using unicode spinner
            let loading_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let loading_messages = [
                "Consulting the digital oracle...",
                "Summoning AI wisdom...",
                "Asking the machines nicely...",
                "Brewing up an answer...",
                "Thinking really hard...",
                "Channeling silicon spirits...",
                "Calculating probabilities...",
                "Parsing the universe...",
            ];

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