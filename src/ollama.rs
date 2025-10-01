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
use futures_util::StreamExt;

macro_rules! debug_println {
    ($($arg:tt)*) => {
        if std::env::var("BOBBAR_DEBUG").is_ok() {
            println!($($arg)*);
        }
    };
}

macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        if std::env::var("BOBBAR_DEBUG").is_ok() {
            eprintln!($($arg)*);
        }
    };
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    images: Option<Vec<String>>,
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

    pub fn get_model(&self) -> &str {
        &self.model
    }

    pub async fn query(&mut self, prompt: &str) -> Result<String> {
        self.query_internal::<fn(String)>(prompt, true, None, None).await
    }

    pub async fn query_streaming<F>(&mut self, prompt: &str, mut callback: F) -> Result<String>
    where
        F: FnMut(String) + Send,
    {
        self.query_internal(prompt, true, None, Some(&mut callback)).await
    }

    pub async fn query_with_image(&mut self, prompt: &str, base64_image: &str) -> Result<String> {
        self.query_internal::<fn(String)>(prompt, false, Some(base64_image.to_string()), None).await
    }

    fn query_without_tools<'a>(&'a mut self, prompt: &'a str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.query_internal::<fn(String)>(prompt, false, None, None))
    }

    async fn query_internal<F>(&mut self, initial_prompt: &str, allow_tools: bool, image: Option<String>, mut callback: Option<&mut F>) -> Result<String>
    where
        F: FnMut(String) + Send,
    {
        self.query_internal_with_iterations(initial_prompt, allow_tools, image, callback, 5).await
    }

    async fn query_internal_with_iterations<F>(&mut self, initial_prompt: &str, allow_tools: bool, image: Option<String>, mut callback: Option<&mut F>, max_iterations: usize) -> Result<String>
    where
        F: FnMut(String) + Send,
    {
        let original_question = initial_prompt.to_string();
        let mut prompt_for_iteration = initial_prompt.to_string();
        let mut iteration = 0;
        let mut use_image = image.clone(); // Clone for first iteration
        let mut tool_results_context = String::new();

        loop {
            iteration += 1;

            if iteration > max_iterations {
                return Ok(format!("Maximum tool iteration limit ({}) reached. Last response: {}", max_iterations, prompt_for_iteration));
            }

            // Build prompt with tool descriptions if available and allowed
            let enhanced_prompt = if allow_tools && self.tool_executor.is_some() {
            let executor = self.tool_executor.as_ref().unwrap().lock().await;
            let tools = executor.get_tool_descriptions();
            if !tools.is_empty() {
                let tools_json = serde_json::to_string_pretty(&tools)?;

                // Build the full context with original question and any previous tool results
                let full_context = if !tool_results_context.is_empty() {
                    format!("Original user question: {}\n\n{}\n\nCurrent task: {}",
                        original_question, tool_results_context, prompt_for_iteration)
                } else {
                    format!("User question: {}", prompt_for_iteration)
                };

                format!("You must follow these instructions exactly:

IF the user's question requires using tools from the list below, respond with ONLY valid JSON in one of these formats (no other text):

Single tool:
{{\"tool_type\": \"<type>\", \"tool_name\": \"<name>\", \"parameters\": {{<params>}}}}

Multiple tools (will be executed in parallel):
[
  {{\"tool_type\": \"<type>\", \"tool_name\": \"<name>\", \"parameters\": {{<params>}}}},
  {{\"tool_type\": \"<type>\", \"tool_name\": \"<name>\", \"parameters\": {{<params>}}}}
]

Available tools:\n{}\n\n{}\n\nRemember:
- You can call multiple tools in one response by using an array
- If tools are needed, respond with ONLY the JSON (no markdown, no formatting)
- If no tools are needed, format your response in clean markdown (use headers, lists, code blocks, etc. as appropriate)

**CRITICAL: ABSOLUTELY NEVER use markdown tables (the |---|---| format). Tables will NOT display correctly.**
Instead, present structured data using:
- Clear section headings (## or ###)
- Bullet points with bold labels (• **Label:** value)
- Numbered lists for sequential information
- Simple key-value format on separate lines", tools_json, full_context)
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

        let use_streaming = callback.is_some();

        let request = OllamaChatRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: enhanced_prompt,
                tool_calls: None,
                images: use_image.take().map(|img| vec![img]), // Take image on first use only
            }],
            stream: use_streaming,
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

        let mut response_text = String::new();

        if use_streaming {
            let mut stream = response.bytes_stream();

            while let Some(item) = stream.next().await {
                let chunk = item?;
                let chunk_str = String::from_utf8_lossy(&chunk);

                for line in chunk_str.lines() {
                    if line.trim().is_empty() {
                        continue;
                    }

                    if let Ok(chunk_response) = serde_json::from_str::<OllamaChatResponse>(line) {
                        response_text.push_str(&chunk_response.message.content);

                        if let Some(ref mut cb) = callback {
                            cb(response_text.clone());
                        }
                    }
                }

                // Yield to allow UI to remain responsive
                tokio::task::yield_now().await;
            }
        } else {
            let ollama_response: OllamaChatResponse = response.json().await?;
            response_text = ollama_response.message.content;
        }

            // Check if response contains tool call(s) (only if tools are allowed)
            if allow_tools && self.tool_executor.is_some() {
                let executor = self.tool_executor.as_ref().unwrap().clone();

                // More robust detection - check for various patterns
                let looks_like_tool_call =
                    (response_text.contains("\"tool_type\"") && response_text.contains("\"tool_name\"")) ||
                    (response_text.contains("tool_type") && response_text.contains("tool_name") && response_text.contains("{")) ||
                    response_text.trim().starts_with('{') ||
                    response_text.trim().starts_with('[');

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

                    let mut tools_executed = false;
                    let mut tool_results = Vec::new();

                    for attempt in &parse_attempts {
                        // Try parsing as array of tool calls
                        if let Ok(tool_calls_array) = serde_json::from_str::<Vec<Value>>(attempt) {
                            for tool_call in tool_calls_array {
                                if tool_call.get("tool_type").is_some() && tool_call.get("tool_name").is_some() {
                                    let result = self.execute_tool_call_get_result(tool_call, executor.clone()).await?;
                                    tool_results.push(result);
                                    tools_executed = true;
                                }
                            }
                            if tools_executed {
                                break;
                            }
                        }

                        // Try parsing as single tool call
                        if !tools_executed {
                            if let Ok(tool_call) = serde_json::from_str::<Value>(attempt) {
                                if tool_call.get("tool_type").is_some() && tool_call.get("tool_name").is_some() {
                                    let result = self.execute_tool_call_get_result(tool_call, executor.clone()).await?;
                                    tool_results.push(result);
                                    tools_executed = true;
                                    break;
                                }
                            }
                        }
                    }

                    if tools_executed {
                        // Combine all tool results and append to context
                        let combined_results = tool_results.join("\n\n---\n\n");

                        // Update context with these tool results
                        if tool_results_context.is_empty() {
                            tool_results_context = format!("Tool results from iteration {}:\n{}", iteration, combined_results);
                        } else {
                            tool_results_context.push_str(&format!("\n\nTool results from iteration {}:\n{}", iteration, combined_results));
                        }

                        // Set next iteration prompt
                        prompt_for_iteration = format!(
                            "Based on the tool results above, either:\n1. Call more tools if additional information is needed to answer the original question\n2. Provide the final answer to the user in clean markdown format\n\n**CRITICAL: NEVER use markdown tables (|---|---|). Instead use bullet points with bold labels.**"
                        );

                        // Print the full context that will be sent to the next iteration
                        debug_eprintln!("\n=== Iteration {} Complete - Next Prompt ===", iteration);
                        debug_eprintln!("Original question: {}", original_question);
                        if !tool_results_context.is_empty() {
                            debug_eprintln!("\n{}", tool_results_context);
                        }
                        debug_eprintln!("\nNext task: {}", prompt_for_iteration);
                        debug_eprintln!("=========================================\n");

                        // Continue loop to reprocess with tool results
                        continue;
                    }
                }
            }

            // No tool call detected, this is the final response
            return Ok(response_text);
        }
    }

    async fn execute_tool_call_get_result(&mut self, tool_call: Value, executor: Arc<Mutex<crate::tools::ToolExecutor>>)
        -> Result<String> {
        // Execute the tool and return a formatted result string

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

                // Format parameters for display
                let params_summary: Vec<String> = params.iter()
                    .map(|(k, v)| format!("- **{}**: {}", k, v))
                    .collect();
                let params_str = if params_summary.is_empty() {
                    "No parameters".to_string()
                } else {
                    params_summary.join("\n")
                };

                match executor.execute_http_tool(tool_name, params.clone()).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;
                        // Return formatted result
                        Ok(format!(
                            "Tool '{}' was called with:\n{}\n\nAnd returned:\n{}",
                            tool_name, params_str, result_str
                        ))
                    },
                    Err(e) => {
                        Ok(format!("Tool '{}' failed with error: {}", tool_name, e))
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

                // Format parameters for display
                let params_str = serde_json::to_string_pretty(&parameters)?;

                let mut executor = executor.lock().await;
                match executor.execute_mcp_tool(&server_name, actual_tool_name, parameters.clone()).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;
                        // Return formatted result
                        Ok(format!(
                            "MCP tool '{}' was called with:\n{}\n\nAnd returned:\n{}",
                            tool_name, params_str, result_str
                        ))
                    },
                    Err(e) => {
                        Ok(format!("MCP tool '{}' failed with error: {}", tool_name, e))
                    }
                }
            },
            _ => Err(anyhow::anyhow!("Unknown tool type: {}", tool_type))?
        }
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

                // Format parameters for display
                let params_summary: Vec<String> = params.iter()
                    .map(|(k, v)| format!("- **{}**: {}", k, v))
                    .collect();
                let params_str = if params_summary.is_empty() {
                    "No parameters".to_string()
                } else {
                    params_summary.join("\n")
                };

                match executor.execute_http_tool(tool_name, params.clone()).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;
                        // Send tool results back to LLM for processing
                        let process_prompt = format!(
                            "Tool '{}' was called with:\n{}\n\nAnd returned:\n{}\n\n**CRITICAL: NEVER use markdown tables (|---|---|). Instead use bullet points with bold labels.**\n\nProvide a clear answer in markdown format with the key information the user needs.",
                            tool_name, params_str, result_str
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

                // Format parameters for display
                let params_str = serde_json::to_string_pretty(&parameters)?;

                let mut executor = executor.lock().await;
                match executor.execute_mcp_tool(&server_name, actual_tool_name, parameters.clone()).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;

                        // Send tool results back to LLM for processing
                        let process_prompt = format!(
                            "MCP tool '{}' was called with:\n{}\n\nAnd returned:\n{}\n\n**CRITICAL: NEVER use markdown tables (|---|---|). Instead use bullet points with bold labels.**\n\nProvide a clear answer in markdown format with the key information the user needs.",
                            tool_name, params_str, result_str
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