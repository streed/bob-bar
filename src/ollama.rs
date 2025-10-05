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

#[allow(unused_macros)]
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
    #[allow(dead_code)]
    role: String,
    content: String,
    #[allow(dead_code)]
    #[serde(default)]
    tool_calls: Vec<ToolCall>,
    #[allow(dead_code)]
    #[serde(default)]
    thinking: Option<String>,
}

pub struct OllamaClient {
    base_url: String,
    model: String,
    client: reqwest::Client,
    tool_executor: Option<Arc<Mutex<ToolExecutor>>>,
    available_tools: Option<Vec<String>>, // Filter to only these tools if specified
    max_tool_turns: usize,
    summarization_model: Option<String>,
    summarization_threshold: usize,
    is_research_mode: bool,  // Whether this client is used for research (higher thresholds)
}

impl OllamaClient {
    #[allow(dead_code)]
    pub fn new() -> Self {
        let base_url = env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());
        let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama2".to_string());

        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        OllamaClient {
            base_url,
            model,
            client,
            tool_executor: None,
            available_tools: None,
            max_tool_turns: 5,
            summarization_model: None,
            summarization_threshold: 5000,
            is_research_mode: false,
        }
    }

    pub fn with_config(base_url: String, model: String) -> Self {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        OllamaClient {
            base_url,
            model,
            client,
            tool_executor: None,
            available_tools: None,
            max_tool_turns: 5,
            summarization_model: None,
            summarization_threshold: 5000,
            is_research_mode: false,
        }
    }

    pub fn set_max_tool_turns(&mut self, max_turns: usize) {
        self.max_tool_turns = max_turns;
    }

    pub fn set_tool_executor(&mut self, executor: Arc<Mutex<ToolExecutor>>) {
        self.tool_executor = Some(executor);
    }

    pub fn set_available_tools(&mut self, tools: Vec<String>) {
        self.available_tools = Some(tools);
    }

    pub fn set_summarization_config(&mut self, model: Option<String>, threshold: usize, is_research: bool) {
        self.summarization_model = model;
        self.summarization_threshold = threshold;
        self.is_research_mode = is_research;
    }

    pub fn get_model(&self) -> &str {
        &self.model
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
    fn query_without_tools<'a>(&'a mut self, prompt: &'a str) -> Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>> {
        Box::pin(self.query_internal::<fn(String)>(prompt, false, None, None))
    }

    async fn query_internal<F>(&mut self, initial_prompt: &str, allow_tools: bool, image: Option<String>, callback: Option<&mut F>) -> Result<String>
    where
        F: FnMut(String) + Send,
    {
        let max_iterations = self.max_tool_turns;
        self.query_internal_with_iterations(initial_prompt, allow_tools, image, callback, max_iterations).await
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
                // Return accumulated context instead of error message
                if !tool_results_context.is_empty() {
                    debug_eprintln!("[Tool] Maximum iteration limit ({}) reached. Returning accumulated context.", max_iterations);
                    return Ok(format!(
                        "Based on the research gathered:\n\n{}\n\nNote: Reached maximum tool iteration limit. The above represents all gathered information.",
                        tool_results_context
                    ));
                } else {
                    return Ok(format!("Maximum tool iteration limit ({}) reached before gathering results.", max_iterations));
                }
            }

            // Build prompt with tool descriptions if available and allowed
            let enhanced_prompt = if allow_tools && self.tool_executor.is_some() {
            let executor = self.tool_executor.as_ref().unwrap().lock().await;
            let mut tools = executor.get_tool_descriptions();

            // Filter tools if available_tools is specified and non-empty
            // Empty list means no filtering (all tools visible)
            if let Some(ref allowed) = self.available_tools {
                if !allowed.is_empty() {
                    tools.retain(|tool| allowed.contains(&tool.name));
                }
            }

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
- **IMPORTANT: Call ALL needed tools at once in a single array when possible** - Don't make users wait for sequential tool calls
- Use multiple tools when: gathering different types of info, checking multiple sources, or performing parallel lookups
- Example: If researching a topic, call web_search AND read relevant files in the same response
- If tools are needed, respond with ONLY the JSON (no markdown, no formatting)
- If no tools are needed, format your response in clean markdown (use headers, lists, code blocks, etc. as appropriate)

Present structured data using:
- Clear section headings (## or ###)
- Bullet points with bold labels (• **Label:** value)
- Numbered lists for sequential information
- Simple key-value format on separate lines
- If you use a markdown table, include clear, specific column headers for each column (no generic names)", tools_json, full_context)
            } else {
                format!("Present structured data using:
- Clear section headings (## or ###)
- Bullet points with bold labels (• **Label:** value)
- Numbered lists for sequential information
- Simple key-value format on separate lines
- If you use a markdown table, include clear, specific column headers for each column (no generic names)

Format your response in clean markdown (use headers, lists, code blocks, etc. as appropriate). Be concise - keep responses to 1-3 sentences unless asked for more detail.\n\nUser: {}", prompt_for_iteration)
            }
        } else {
            format!("Present structured data using:
- Clear section headings (## or ###)
- Bullet points with bold labels (• **Label:** value)
- Numbered lists for sequential information
- Simple key-value format on separate lines
- If you use a markdown table, include clear, specific column headers for each column (no generic names)

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

        // Retry logic: try up to 10 times on non-2xx status codes
        let mut last_error = None;
        let mut response = None;

        for attempt in 1..=10 {
            let req_response = self.client
                .post(format!("{}/api/chat", self.base_url))
                .json(&request)
                .send()
                .await;

            match req_response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        response = Some(resp);
                        break;
                    } else {
                        // For 400 errors, print full response body for debugging
                        if status.as_u16() == 400 {
                            let error_body = resp.text().await.unwrap_or_else(|_| "Could not read response body".to_string());
                            let error_msg = format!("Ollama API 400 Bad Request (attempt {}/10):\n{}", attempt, error_body);
                            eprintln!("{}", error_msg);
                            last_error = Some(error_msg);
                        } else {
                            let error_msg = format!("Ollama API error: {} (attempt {}/10)", status, attempt);
                            eprintln!("{}", error_msg);
                            last_error = Some(error_msg);
                        }

                        // Wait before retrying (progressive backoff: 2s, 5s, 10s, 15s, 20s, 25s, 30s, 35s, 40s)
                        if attempt < 10 {
                            let delay_secs = match attempt {
                                1 => 2,
                                2 => 5,
                                3 => 10,
                                4 => 15,
                                5 => 20,
                                6 => 25,
                                7 => 30,
                                8 => 35,
                                9 => 40,
                                _ => 40,
                            };
                            tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                        }
                    }
                },
                Err(e) => {
                    let error_msg = format!("Ollama request failed: {} (attempt {}/10)", e, attempt);
                    eprintln!("{}", error_msg);
                    last_error = Some(error_msg);

                    // Wait before retrying (progressive backoff: 2s, 5s, 10s, 15s, 20s, 25s, 30s, 35s, 40s)
                    if attempt < 10 {
                        let delay_secs = match attempt {
                            1 => 2,
                            2 => 5,
                            3 => 10,
                            4 => 15,
                            5 => 20,
                            6 => 25,
                            7 => 30,
                            8 => 35,
                            9 => 40,
                            _ => 40,
                        };
                        tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
                    }
                }
            }
        }

        let response = response.ok_or_else(|| {
            anyhow::anyhow!("Ollama API failed after 10 attempts. Last error: {}",
                last_error.unwrap_or_else(|| "Unknown error".to_string()))
        })?;

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
                            "Based on the tool results above, either:\n1. Call more tools if additional information is needed to answer the original question\n2. Provide the final answer to the user in clean markdown format"
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

    /// Extract critical research fields from JSON that should always be preserved
    fn extract_critical_fields(json_value: &Value) -> Vec<(String, String)> {
        let mut critical = Vec::new();

        fn extract_recursive(value: &Value, path: String, critical: &mut Vec<(String, String)>) {
            match value {
                Value::Object(map) => {
                    for (key, val) in map {
                        let key_lower = key.to_lowercase();
                        let new_path = if path.is_empty() { key.clone() } else { format!("{}.{}", path, key) };

                        // Check if this is a critical field
                        let is_critical = key_lower.contains("url") || key_lower.contains("doi") ||
                                        key_lower.contains("author") || key_lower.contains("title") ||
                                        key_lower.contains("date") || key_lower.contains("citation") ||
                                        key_lower.contains("link") || key_lower.contains("href") ||
                                        key_lower.contains("source") || key_lower.contains("reference");

                        if is_critical {
                            if let Some(s) = val.as_str() {
                                critical.push((new_path.clone(), s.to_string()));
                            } else if !val.is_null() {
                                critical.push((new_path.clone(), val.to_string()));
                            }
                        }

                        extract_recursive(val, new_path, critical);
                    }
                },
                Value::Array(arr) => {
                    for (i, val) in arr.iter().enumerate() {
                        let new_path = format!("{}[{}]", path, i);
                        extract_recursive(val, new_path, critical);
                    }
                },
                _ => {}
            }
        }

        extract_recursive(json_value, String::new(), &mut critical);
        critical
    }

    /// Smart structural summarization for JSON data
    fn smart_summarize_json(json_value: &Value, max_chars: usize) -> Option<Value> {
        match json_value {
            Value::Array(arr) if arr.len() > 10 => {
                // For large arrays, sample first 5 + last 2 items
                let mut sampled = Vec::new();
                sampled.extend_from_slice(&arr[..5.min(arr.len())]);
                if arr.len() > 7 {
                    sampled.push(serde_json::json!({
                        "_note": format!("... {} items omitted ...", arr.len() - 7)
                    }));
                    sampled.extend_from_slice(&arr[arr.len()-2..]);
                }
                Some(Value::Array(sampled))
            },
            Value::Object(map) if serde_json::to_string(json_value).ok()?.len() > max_chars => {
                // For large objects, preserve critical fields and summarize others
                let mut result = serde_json::Map::new();
                let mut char_count = 0;
                let mut fields_included = 0;
                let total_fields = map.len();

                for (key, value) in map {
                    let key_lower = key.to_lowercase();
                    let is_critical = key_lower.contains("url") || key_lower.contains("doi") ||
                                    key_lower.contains("author") || key_lower.contains("title") ||
                                    key_lower.contains("date") || key_lower.contains("citation");

                    if is_critical || char_count < max_chars / 2 {
                        result.insert(key.clone(), value.clone());
                        char_count += serde_json::to_string(value).ok()?.len();
                        fields_included += 1;
                    }
                }

                if fields_included < total_fields {
                    result.insert(
                        "_summary".to_string(),
                        Value::String(format!("{} of {} fields shown (critical fields preserved)", fields_included, total_fields))
                    );
                }

                Some(Value::Object(result))
            },
            _ => None
        }
    }

    /// Summarize a long tool result to reduce token usage (non-recursive version)
    async fn summarize_tool_result(&self, tool_name: &str, result: &str) -> Result<String> {
        // Use configured threshold (higher for research mode)
        let max_length = self.summarization_threshold;

        // If result is short enough, return as-is
        if result.len() <= max_length {
            return Ok(result.to_string());
        }

        debug_eprintln!("[Tool] Result from '{}' is {} chars, summarizing (threshold: {})...", tool_name, result.len(), max_length);

        // Try structural summarization first for JSON
        if let Ok(json_value) = serde_json::from_str::<Value>(result) {
            debug_eprintln!("[Tool] Attempting structural summarization for JSON...");

            // Extract critical fields first
            let critical_fields = Self::extract_critical_fields(&json_value);

            // Try smart JSON summarization
            if let Some(summarized_json) = Self::smart_summarize_json(&json_value, max_length) {
                let summarized_str = serde_json::to_string_pretty(&summarized_json)
                    .unwrap_or_else(|_| result.to_string());

                if summarized_str.len() <= max_length * 2 {
                    debug_eprintln!("[Tool] Structural summarization successful: {} -> {} chars", result.len(), summarized_str.len());

                    // Append critical fields as a note if any were extracted
                    if !critical_fields.is_empty() && summarized_str.len() < max_length {
                        let critical_note = format!(
                            "\n\n# Critical Research Data Preserved:\n{}",
                            critical_fields.iter()
                                .take(20) // Limit to first 20 critical fields
                                .map(|(k, v)| format!("- {}: {}", k, v))
                                .collect::<Vec<_>>()
                                .join("\n")
                        );
                        return Ok(format!("{}{}", summarized_str, critical_note));
                    }

                    return Ok(summarized_str);
                }
            }
        }

        // Fall back to LLM summarization
        debug_eprintln!("[Tool] Using LLM summarization...");

        let prompt = format!(
            "Condense this tool result while keeping all important information:\n\n\
            - Preserve ALL URLs, DOIs, citations, author names, and dates\n\
            - Keep all key facts, data, and specific details\n\
            - Preserve technical information and numbers\n\
            - Maintain structure and context\n\
            - Remove only truly redundant content\n\n\
            Tool result:\n{}",
            result
        );

        // Use summarization model if configured, otherwise use main model
        let model_to_use = self.summarization_model.as_ref().unwrap_or(&self.model).clone();

        // Make a direct API call without going through query_internal to avoid recursion
        let request = OllamaChatRequest {
            model: model_to_use.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
                images: None,
            }],
            stream: false,
            tools: None,
        };

        let response = self.client
            .post(format!("{}/api/chat", self.base_url))
            .json(&request)
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<OllamaChatResponse>().await {
                    Ok(ollama_response) => {
                        let summary = ollama_response.message.content;
                        debug_eprintln!("[Tool] LLM summarized '{}' using {} from {} to {} chars",
                            tool_name, model_to_use, result.len(), summary.len());
                        Ok(summary)
                    },
                    Err(e) => {
                        debug_eprintln!("[Tool] Failed to parse summarization response: {}", e);
                        Ok(format!("{}...\n\n[Note: Content truncated due to length]", &result[..max_length]))
                    }
                }
            },
            _ => {
                debug_eprintln!("[Tool] Summarization request failed, using truncated version");
                Ok(format!("{}...\n\n[Note: Content truncated due to length]", &result[..max_length]))
            }
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

        // Always look up the tool by name to verify/correct the type
        // LLM sometimes returns wrong tool_type (e.g., "builtin" for HTTP tools)
        let actual_tool_type: String = {
            let exec = executor.lock().await;
            let tool_descriptions = exec.get_tool_descriptions();

            if let Some(tool_desc) = tool_descriptions.iter().find(|t| t.name == tool_name) {
                if tool_desc.tool_type != tool_type {
                    debug_eprintln!("[Tool] Corrected type for '{}': {} -> {}", tool_name, tool_type, tool_desc.tool_type);
                }
                tool_desc.tool_type.clone()
            } else {
                debug_eprintln!("[Tool] Warning: Tool '{}' not found in descriptions, using LLM-provided type '{}'", tool_name, tool_type);
                tool_type.to_string()
            }
        };

        let raw_result = match actual_tool_type.as_str() {
            "builtin" => {
                // Convert JSON parameters to HashMap<String, String>
                let params: std::collections::HashMap<String, String> = if let Some(obj) = parameters.as_object() {
                    obj.iter()
                        .map(|(k, v)| {
                            let value = match v {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                Value::Bool(b) => b.to_string(),
                                _ => v.to_string(),
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

                match executor.execute_builtin_tool(tool_name, params.clone()).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;

                        // Summarize if too long
                        let summarized_result = self.summarize_tool_result(tool_name, &result_str).await?;

                        format!(
                            "Built-in tool '{}' was called with:\n{}\n\nAnd returned:\n{}",
                            tool_name, params_str, summarized_result
                        )
                    },
                    Err(e) => {
                        format!("Built-in tool '{}' failed with error: {}", tool_name, e)
                    }
                }
            },
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

                        // Summarize if too long
                        let summarized_result = self.summarize_tool_result(tool_name, &result_str).await?;

                        format!(
                            "Tool '{}' was called with:\n{}\n\nAnd returned:\n{}",
                            tool_name, params_str, summarized_result
                        )
                    },
                    Err(e) => {
                        format!("Tool '{}' failed with error: {}", tool_name, e)
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

                let executor = executor.lock().await;
                match executor.execute_mcp_tool(&server_name, actual_tool_name, parameters.clone()).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;

                        // Summarize if too long
                        let summarized_result = self.summarize_tool_result(tool_name, &result_str).await?;

                        format!(
                            "MCP tool '{}' was called with:\n{}\n\nAnd returned:\n{}",
                            tool_name, params_str, summarized_result
                        )
                    },
                    Err(e) => {
                        format!("MCP tool '{}' failed with error: {}", tool_name, e)
                    }
                }
            },
            _ => {
                format!("Tool '{}' of type '{}' is not available. Please continue without this tool.",
                    tool_name, tool_type)
            }
        };

        Ok(raw_result)
    }

    #[allow(dead_code)]
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
            "builtin" => {
                // Convert JSON parameters to HashMap<String, String>
                let params: std::collections::HashMap<String, String> = if let Some(obj) = parameters.as_object() {
                    obj.iter()
                        .map(|(k, v)| {
                            let value = match v {
                                Value::String(s) => s.clone(),
                                Value::Number(n) => n.to_string(),
                                Value::Bool(b) => b.to_string(),
                                _ => v.to_string(),
                            };
                            (k.clone(), value)
                        })
                        .collect()
                } else {
                    std::collections::HashMap::new()
                };

                let executor = executor.lock().await;

                match executor.execute_builtin_tool(tool_name, params).await {
                    Ok(result) => Ok(serde_json::to_string(&result)?),
                    Err(e) => Err(e),
                }
            },
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
                            "Tool '{}' was called with:\n{}\n\nAnd returned:\n{}\n\nProvide a clear answer in markdown format with the key information the user needs. If you present data in a table, include clear, specific column headers for each column.",
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

                let executor = executor.lock().await;
                match executor.execute_mcp_tool(&server_name, actual_tool_name, parameters.clone()).await {
                    Ok(result) => {
                        let result_str = serde_json::to_string_pretty(&result)?;

                        // Send tool results back to LLM for processing
                        let process_prompt = format!(
                            "MCP tool '{}' was called with:\n{}\n\nAnd returned:\n{}\n\nProvide a clear answer in markdown format with the key information the user needs. If you present data in a table, include clear, specific column headers for each column.",
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
            _ => {
                // Unknown tool type - return message and continue instead of failing
                Ok(format!("Tool '{}' of type '{}' is not available. Please continue without this tool.",
                    tool_name, tool_type))
            }
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
