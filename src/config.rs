use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

fn default_max_tool_turns() -> usize {
    5
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub ollama: OllamaConfig,
    pub window: WindowConfig,
}

fn default_vision_model() -> String {
    "llama3.2-vision:11b".to_string()
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OllamaConfig {
    pub host: String,
    pub model: String,
    #[serde(default = "default_vision_model")]
    pub vision_model: String,
    #[serde(default = "default_max_tool_turns")]
    pub max_tool_turns: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WindowConfig {
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub min_height: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ollama: OllamaConfig {
                host: "http://localhost:11434".to_string(),
                model: "llama2".to_string(),
                vision_model: "llama3.2-vision:11b".to_string(),
                max_tool_turns: 5,
            },
            window: WindowConfig {
                width: 800,
                height: 600,
                min_width: 400,
                min_height: 300,
            },
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let config_path = Self::get_config_path();

        if config_path.exists() {
            match fs::read_to_string(&config_path) {
                Ok(contents) => {
                    match toml::from_str(&contents) {
                        Ok(config) => return config,
                        Err(e) => eprintln!("Error parsing config.toml: {}. Using defaults.", e),
                    }
                }
                Err(e) => eprintln!("Error reading config.toml: {}. Using defaults.", e),
            }
        } else {
            // Create config directory if it doesn't exist
            if let Some(parent) = config_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
        }

        Config::default()
    }

    pub fn get_config_path() -> PathBuf {
        if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(".config/bob-bar/config.toml")
        } else {
            PathBuf::from("config.toml")
        }
    }

    pub fn get_config_dir() -> PathBuf {
        if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home).join(".config/bob-bar")
        } else {
            PathBuf::from(".")
        }
    }
}