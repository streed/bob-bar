use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

fn default_max_tool_turns() -> usize {
    5
}

fn default_context_window() -> usize {
    128000
}

fn default_max_refinement_iterations() -> usize {
    5
}

fn default_max_document_iterations() -> usize {
    3
}

fn default_min_worker_count() -> usize {
    3
}

fn default_max_worker_count() -> usize {
    10
}

fn default_max_debate_rounds() -> usize {
    2
}

fn default_embedding_model() -> String {
    "nomic-embed-text".to_string()
}

fn default_embedding_dimensions() -> usize {
    768
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub research: ResearchConfig,
}

fn default_vision_model() -> String {
    "llama3.2-vision:11b".to_string()
}

fn default_research_model() -> Option<String> {
    None // Will use main model if not specified
}

fn default_summarization_model() -> Option<String> {
    None // Will use main model if not specified
}

fn default_summarization_threshold() -> usize {
    5000
}

fn default_summarization_threshold_research() -> usize {
    10000
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct OllamaConfig {
    pub host: String,
    pub model: String,
    #[serde(default = "default_vision_model")]
    pub vision_model: String,
    #[serde(default = "default_research_model")]
    pub research_model: Option<String>,
    #[serde(default = "default_summarization_model")]
    pub summarization_model: Option<String>,
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    #[serde(default = "default_embedding_dimensions")]
    pub embedding_dimensions: usize,
    #[serde(default = "default_max_refinement_iterations")]
    pub max_refinement_iterations: usize,
    #[serde(default = "default_max_document_iterations")]
    pub max_document_iterations: usize,
    #[serde(default = "default_max_debate_rounds")]
    pub max_debate_rounds: usize,
    #[serde(default = "default_context_window")]
    pub context_window: usize,
    #[serde(default = "default_max_tool_turns")]
    pub max_tool_turns: usize,
    #[serde(default = "default_summarization_threshold")]
    pub summarization_threshold: usize,
    #[serde(default = "default_summarization_threshold_research")]
    pub summarization_threshold_research: usize,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ResearchConfig {
    #[serde(default = "default_min_worker_count")]
    pub min_worker_count: usize,
    #[serde(default = "default_max_worker_count")]
    pub max_worker_count: usize,
    #[serde(default = "default_export_memories")]
    pub export_memories: bool,
}

fn default_export_memories() -> bool {
    false
}

impl Default for ResearchConfig {
    fn default() -> Self {
        ResearchConfig {
            min_worker_count: 3,
            max_worker_count: 10,
            export_memories: false,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            ollama: OllamaConfig {
                host: "http://localhost:11434".to_string(),
                model: "llama2".to_string(),
                vision_model: "llama3.2-vision:11b".to_string(),
                research_model: None,
                summarization_model: None,
                embedding_model: "nomic-embed-text".to_string(),
                embedding_dimensions: 768,
                max_refinement_iterations: 5,
                max_document_iterations: 3,
                max_debate_rounds: 2,
                context_window: 128000,
                max_tool_turns: 5,
                summarization_threshold: 5000,
                summarization_threshold_research: 10000,
            },
            research: ResearchConfig::default(),
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