use reqwest;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use crate::tools::ToolExecutor;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::pin::Pin;
use std::future::Future;
use anyhow::Result;

// Helper function to extract JSON object from text
fn extract_json_object(text: &str) -> Option<String> {
    // Find the first { and last } to extract JSON
    if let Some(start) = text.find('{') {
        let mut brace_count = 0;
        let mut in_string = false;
        let mut escape = false;

        for (i, ch) in text[start..].chars().enumerate() {
            if escape {
                escape = false;
                continue;
            }

            if ch == '\\' {
                escape = true;
                continue;
            }

            if ch == '"' && !escape {
                in_string = !in_string;
            }

            if !in_string {
                match ch {
                    '{' => brace_count += 1,
                    '}' => {
                        brace_count -= 1;
                        if brace_count == 0 {
                            return Some(text[start..start + i + 1].to_string());
                        }
                    },
                    _ => {}
                }
            }
        }
    }
    None
}

#[derive(Debug, Serialize)]
struct OllamaChatRequest {
    model: String,
    messages: Vec<Message>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<Value>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Message {
    role: String,
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCall>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ToolCall {
    function: FunctionCall,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct FunctionCall {
    name: String,
    arguments: Value,
}

#[derive(Debug, Deserialize)]
struct OllamaChatResponse {
    #[allow(dead_code)]
    model: String,
    #[allow(dead_code)]
    created_at: String,
    message: ResponseMessage,
    #[allow(dead_code)]
    done: bool,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    role: String,
    content: String,
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
    #[serde(default)]
    thinking: Option<String>,
}

pub struct OllamaClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
    tool_executor: Option<Arc<Mutex<ToolExecutor>>>,
}

impl OllamaClient {
    #[allow(dead_code)]
    pub fn new() -> Self {
        let base_url = env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string());

        OllamaClient {
            base_url,
            model,
            client: reqwest::Client::new(),
            tool_executor: None,
        }
    }

    pub fn with_config(base_url: String, model: String) -> Self {
        OllamaClient {
            base_url,
            model,
            client: reqwest::Client::new(),
            tool_executor: None,
        }
    }

    pub fn set_tool_executor(&mut self, executor: Arc<Mutex<ToolExecutor>>) {
        self.tool_executor = Some(executor);
    }

    pub async fn query(&mut self, prompt: &str) -> Result<String> {
        self.query_internal(prompt, true).await
    }

    fn query_without_tools<'a>(&'a mut self, prompt: &'a str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.query_internal(prompt, false))
    }

    async fn query_internal(&mut self, initial_prompt: &str, allow_tools: bool) -> Result<String> {
        let prompt_for_iteration = initial_prompt.to_string();

            // Build prompt with tool descriptions if available and allowed
            let enhanced_prompt = if allow_tools && self.tool_executor.is_some() {
            let executor = self.tool_executor.as_ref().unwrap().lock().await;
            let tools = executor.get_tool_descriptions();
            if !tools.is_empty() {
                let tools_json = serde_json::to_string_pretty(&tools)?;
                format!("You must follow these instructions exactly:

IF the user's question requires using a tool from the list below, respond with ONLY valid JSON in this exact format (no other text):
{{\"tool_type\": \"<type>\", \"tool_name\": \"<name>\", \"parameters\": {{<params>}}}}

Available tools:\n{}\n\nUser question: {}\n\nRemember:
- If a tool is needed, respond with ONLY the JSON (no markdown, no formatting).
- If no tool is needed, format your response in clean markdown (use headers, lists, code blocks, etc. as appropriate).

**CRITICAL: ABSOLUTELY NEVER use markdown tables (the |---|---| format). Tables will NOT display correctly.**
Instead, present structured data using:
- Clear section headings (## or ###)
- Bullet points with bold labels (• **Label:** value)
- Numbered lists for sequential information
- Simple key-value format on separate lines", tools_json, prompt_for_iteration)
            } else {
                format!("**CRITICAL: ABSOLUTELY NEVER use markdown tables (the |---|---| format). Tables will NOT display correctly.**

Instead, present structured data using:
- Clear section headings (## or ###)
- Bullet points with bold labels (• **Label:** value)
- Numbered lists for sequential information
- Simple key-value format on separate lines

Format your response in clean markdown (use headers, lists, code blocks, etc. as appropriate). Be concise - keep responses to 1-3 sentences unless asked for more detail.\n\nUser: {}", prompt_for_iteration)
            }
        } else {
            format!("**CRITICAL: ABSOLUTELY NEVER use markdown tables (the |---|---| format). Tables will NOT display correctly.**

Instead, present structured data using:
- Clear section headings (## or ###)
- Bullet points with bold labels (• **Label:** value)
- Numbered lists for sequential information
- Simple key-value format on separate lines

Format your response in clean markdown (use headers, lists, code blocks, etc. as appropriate). Be concise - keep responses to 1-3 sentences unless asked for more detail.\n\nUser: {}", prompt_for_iteration)
        };

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: enhanced_prompt,
                tool_calls: None,
            }],
            stream: false,
            tools: None,
        };

        let response = self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Ollama API error: {}", response.status()));
        }

        let ollama_response: OllamaChatResponse = response.json().await?;
        let mut response_text = ollama_response.message.content;

        // Check if response contains tool call (only if tools are allowed)
        if allow_tools && self.tool_executor.is_some() {
            let executor = self.tool_executor.as_ref().unwrap();

            // More robust detection - check for various patterns
            let looks_like_tool_call =
                (response_text.contains("\"tool_type\"") && response_text.contains("\"tool_name\"")) ||
                (response_text.contains("tool_type") && response_text.contains("tool_name") && response_text.contains("{")) ||
                response_text.trim().starts_with('{');

            if looks_like_tool_call {

                // Strip markdown code blocks if present
                let cleaned_response = if response_text.contains("```") {
                    // Find content between ``` markers
                    let start_marker = "```json";
                    let alt_start = "```";
                    let end_marker = "```";

                    let mut content = response_text.as_str();

                    // Remove opening marker
                    if content.contains(start_marker) {
                        if let Some(pos) = content.find(start_marker) {
                            content = &content[pos + start_marker.len()..];
                        }
                    } else if content.starts_with(alt_start) {
                        content = &content[alt_start.len()..];
                    }

                    // Remove closing marker
                    if let Some(pos) = content.rfind(end_marker) {
                        content = &content[..pos];
                    }

                    content.trim().to_string()
                } else {
                    response_text.clone()
                };

                // Try multiple parsing strategies
                let mut parse_attempts = vec![
                    cleaned_response.clone(),
                    response_text.trim().to_string(),
                ];

                // Also try extracting just the JSON object
                if let Some(extracted) = extract_json_object(&response_text) {
                    parse_attempts.push(extracted);
                }

                for attempt in &parse_attempts {
                    match serde_json::from_str::<Value>(attempt) {
                        Ok(tool_call) => {
                            // Validate that it has the required fields
                            if tool_call.get("tool_type").is_some() && tool_call.get("tool_name").is_some() {
                                response_text = self.execute_tool_call(tool_call, executor.clone()).await?;
                                break; // Stop trying other parse strategies
                            }
                        },
                        Err(_) => {
                            // This parsing strategy didn't work, try the next one
                            continue;
                        }
                    }
                }
            }
        }

        // No tool call, this is the final response
        Ok(response_text)
    }

    async fn execute_tool_call(&mut self, tool_call: Value, executor: Arc<Mutex<crate::tools::ToolExecutor>>)
        -> Result<String> {

        let tool_type = tool_call.get("tool_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing tool_type"))?;

        let tool_name = tool_call.get("tool_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing tool_name"))?;

        let parameters = tool_call.get("parameters")
            .ok_or_else(|| anyhow::anyhow!("Missing parameters"))?;

        match tool_type {
            "http" => {
                // Convert JSON parameters to HashMap<String, String>
                let params: std::collections::HashMap<String, String> = if let Some(obj) = parameters.as_object() {
                    obj.iter()
                        .map(|(k, v)| {
                            let value = match v {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                Value::Bool(b) => b.to_string(),
                                _ => v.to_string().trim_matches('"').to_string(),
                            };
                            (k.clone(), value)
                        })
                        .collect()
                } else {
                    std::collections::HashMap::new()
                };

                let executor = executor.lock().await;
                match executor.execute_http_tool(tool_name, params).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;
                        // Send tool results back to LLM for processing
                        let process_prompt = format!(
                            "Tool '{}' returned:\n{}\n\n**CRITICAL: NEVER use markdown tables (|---|---|). Instead use bullet points with bold labels.**\n\nProvide a clear answer in markdown format with the key information the user needs.",
                            tool_name, result_str
                        );

                        // Make a second call to process the results (without tools to prevent recursion)
                        let processed = Box::pin(self.query_without_tools(&process_prompt)).await?;
                        Ok(processed)
                    },
                    Err(e) => {
                        Ok(format!("Error: {}", e))
                    }
                }
            },
            "mcp" => {
                // Split server:tool format
                let parts: Vec<&str> = tool_name.split(':').collect();
                let (server_name, actual_tool_name) = if parts.len() == 2 {
                    (parts[0].to_string(), parts[1])
                } else {
                    // Fallback to first MCP server if not specified
                    let first_server = {
                        let exec = executor.lock().await;
                        exec.config.tools.mcp.first()
                            .map(|s| s.name.clone())
                            .unwrap_or_else(|| "mcp".to_string())
                    };
                    (first_server, tool_name)
                };

                let mut executor = executor.lock().await;
                match executor.execute_mcp_tool(&server_name, actual_tool_name, parameters.clone()).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;

                        // Send tool results back to LLM for processing
                        let process_prompt = format!(
                            "MCP tool '{}' returned:\n{}\n\n**CRITICAL: NEVER use markdown tables (|---|---|). Instead use bullet points with bold labels.**\n\nProvide a clear answer in markdown format with the key information the user needs.",
                            tool_name, result_str
                        );

                        // Make a second call to process the results (without tools to prevent recursion)
                        let processed = Box::pin(self.query_without_tools(&process_prompt)).await?;
                        Ok(processed)
                    },
                    Err(e) => {
                        Ok(format!("Error: {}", e))
                    }
                }
            },
            _ => Err(anyhow::anyhow!("Unknown tool type: {}", tool_type))?
        }
    }

    #[allow(dead_code)]
    pub fn set_model(&mut self, model: String) {
        self.model = model;
    }

    #[allow(dead_code)]
    pub fn set_base_url(&mut self, url: String) {
        self.base_url = url;
    }
}