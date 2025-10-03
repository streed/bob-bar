use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::process::Command;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use toml;
use std::time::{Instant, Duration};
use std::sync::Mutex as StdMutex;
use tokio::sync::Mutex as TokioMutex;

macro_rules! debug_println {
    ($($arg:tt)*) => {
        if std::env::var("BOBBAR_DEBUG").is_ok() {
            println!($($arg)*);
        }
    };
}

#[allow(unused_macros)]
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        if std::env::var("BOBBAR_DEBUG").is_ok() {
            eprintln!($($arg)*);
        }
    };
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolsConfig {
    pub tools: Tools,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Tools {
    pub http: Vec<HttpTool>,
    pub mcp: Vec<McpServer>,
    #[serde(default)]
    pub builtin: Vec<String>, // List of built-in tools to enable
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpTool {
    pub name: String,
    pub description: String,
    pub endpoint: String,
    pub method: String,
    pub parameters: HashMap<String, ParameterDef>,
    #[serde(default)]
    pub path_params: Vec<String>,  // List of parameter names that should be used in the path
    #[serde(default)]
    pub headers: HashMap<String, String>,
    pub response_format: String,
    #[serde(default)]
    pub response_path: Option<String>,  // Optional JSON path to extract from response (e.g., "data.results[0].value")
    #[serde(default = "default_expected_status")]
    pub expected_status: Vec<String>,  // Expected successful status codes (default: ["2xx"]) - supports wildcards like "2xx", "3xx" or specific codes like "200"
    #[serde(default)]
    pub acceptable_status: Vec<String>,  // Acceptable status codes to ignore result (empty response) - supports wildcards
    #[serde(default)]
    pub error_status: Vec<String>,  // Status codes that should throw detailed errors (if empty, all non-expected are errors) - supports wildcards
}

fn default_expected_status() -> Vec<String> {
    vec!["2xx".to_string(), "3xx".to_string()]
}

/// Check if a status code matches a pattern (supports wildcards like "2xx" or specific codes like "200")
fn status_matches(status_code: u16, pattern: &str) -> bool {
    // Check for exact match first
    if let Ok(exact) = pattern.parse::<u16>() {
        return status_code == exact;
    }

    // Check for wildcard patterns like "2xx", "3xx", etc.
    if pattern.ends_with("xx") && pattern.len() == 3 {
        if let Some(prefix) = pattern.chars().next() {
            if let Some(digit) = prefix.to_digit(10) {
                let status_hundreds = status_code / 100;
                return status_hundreds == digit as u16;
            }
        }
    }

    false
}

/// Check if status code matches any pattern in the list
fn status_in_list(status_code: u16, patterns: &[String]) -> bool {
    patterns.iter().any(|pattern| status_matches(status_code, pattern))
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ParameterDef {
    #[serde(rename = "type")]
    pub param_type: String,
    pub description: String,
    #[serde(default)]
    pub required: bool,
    pub default: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpServer {
    pub name: String,
    pub transport: String,
    pub command: String,
    pub args: Vec<String>,
    pub description: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug)]
pub struct McpConnection {
    #[allow(dead_code)]
    process: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: BufReader<tokio::process::ChildStdout>,
}

// Track tool usage for rate limiting
#[derive(Debug, Clone)]
struct ToolUsage {
    last_call: Instant,
    call_count: usize,
}

pub struct ToolExecutor {
    pub config: ToolsConfig,
    http_client: reqwest::Client,
    mcp_connections: TokioMutex<HashMap<String, McpConnection>>,  // Tokio mutex for async-safe access
    mcp_tools: StdMutex<HashMap<String, Vec<McpTool>>>,  // Store discovered MCP tools per server
    api_keys: HashMap<String, String>,
    tool_usage: StdMutex<HashMap<String, ToolUsage>>,  // Track usage per tool (with interior mutability)
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpTool {
    pub name: String,
    pub description: Option<String>,
    pub input_schema: Option<Value>,
}

impl ToolExecutor {
    pub fn new(config: ToolsConfig, api_keys: HashMap<String, String>) -> Self {
        let mut executor = ToolExecutor {
            config,
            http_client: reqwest::Client::new(),
            mcp_connections: TokioMutex::new(HashMap::new()),
            mcp_tools: StdMutex::new(HashMap::new()),
            api_keys,
            tool_usage: StdMutex::new(HashMap::new()),
        };

        // Register built-in tools
        executor.register_builtin_tools();

        executor
    }

    fn register_builtin_tools(&mut self) {
        // Check which built-in tools are enabled
        for tool_name in &self.config.tools.builtin {
            debug_println!("[BuiltIn] Registering built-in tool: {}", tool_name);
        }
    }

    pub fn is_builtin_tool(&self, tool_name: &str) -> bool {
        self.config.tools.builtin.contains(&tool_name.to_string())
    }

    pub fn from_file(path: &std::path::Path) -> Result<Self, anyhow::Error> {
        // Load tools config
        let config_str = std::fs::read_to_string(path)?;
        let config: ToolsConfig = serde_json::from_str(&config_str)?;

        // Load API keys from config directory
        let config_dir = crate::config::Config::get_config_dir();
        let api_keys_path = config_dir.join("api_keys.toml");

        let api_keys = load_api_keys(&api_keys_path).unwrap_or_else(|e| {
            debug_eprintln!("Warning: Failed to load api_keys.toml: {}", e);
            HashMap::new()
        });

        Ok(Self::new(config, api_keys))
    }

    /// Calculate delay based on recent tool usage
    /// Returns delay in milliseconds based on call frequency
    fn calculate_rate_limit_delay(&self, tool_name: &str) -> u64 {
        let now = Instant::now();
        const RATE_LIMIT_WINDOW: Duration = Duration::from_secs(60); // 1 minute window

        let mut usage_map = self.tool_usage.lock().unwrap();

        // Get or create usage entry
        let usage = usage_map.entry(tool_name.to_string())
            .or_insert(ToolUsage {
                last_call: now,
                call_count: 0,
            });

        // Check if we're still in the same time window
        let elapsed = now.duration_since(usage.last_call);

        if elapsed > RATE_LIMIT_WINDOW {
            // Reset counter if outside window
            usage.call_count = 1;
            usage.last_call = now;
            debug_println!("[RateLimit] {} - First call in new window", tool_name);
            return 0; // No delay for first call in window
        }

        // Within window - increment counter and calculate delay
        usage.call_count += 1;
        usage.last_call = now;

        // Progressive delay: 0ms, 200ms, 500ms, 1000ms, 2000ms, then cap at 3000ms
        let delay_ms = match usage.call_count {
            1 => 0,
            2 => 200,
            3 => 500,
            4 => 1000,
            5 => 2000,
            _ => 3000, // Cap at 3 seconds
        };

        debug_println!("[RateLimit] {} - Call #{} in window, delay: {}ms",
                      tool_name, usage.call_count, delay_ms);

        delay_ms
    }

    /// Apply rate limiting delay before tool execution
    async fn apply_rate_limit(&self, tool_name: &str) {
        let delay_ms = self.calculate_rate_limit_delay(tool_name);
        if delay_ms > 0 {
            debug_println!("[RateLimit] Waiting {}ms before calling {}", delay_ms, tool_name);
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
    }

    pub async fn initialize_mcp_servers(&self) -> Result<(), anyhow::Error> {
        let servers = self.config.tools.mcp.clone();
        if servers.is_empty() {
            debug_println!("[MCP] No MCP servers configured");
            return Ok(());
        }

        debug_println!("[MCP] Initializing {} MCP servers...", servers.len());
        for server in servers {
            debug_println!("[MCP] Connecting to server: {}", server.name);
            match self.connect_mcp_server(server.clone()).await {
                Ok(_) => debug_println!("[MCP] ✓ Successfully connected to: {}", server.name),
                Err(e) => debug_eprintln!("[MCP] ✗ Failed to connect to {}: {}", server.name, e),
            }
        }
        Ok(())
    }

    async fn connect_mcp_server(&self, server: McpServer) -> Result<(), anyhow::Error> {
        if server.transport != "stdio" {
            return Err(anyhow::anyhow!("Unsupported transport: {}", server.transport));
        }

        debug_println!("[MCP] Starting process: {} {:?}", server.command, server.args);
        let mut cmd = Command::new(&server.command);
        cmd.args(&server.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());  // Capture stderr for debugging

        for (key, value) in &server.env {
            debug_println!("[MCP] Setting env var: {}=***", key);
            cmd.env(key, value);
        }

        let mut process = cmd.spawn()?;
        debug_println!("[MCP] Process spawned for: {}", server.name);
        let stdin = process.stdin.take().ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;
        let stdout = process.stdout.take().ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?;
        let stderr = process.stderr.take().ok_or_else(|| anyhow::anyhow!("Failed to get stderr"))?;
        let stdout = BufReader::new(stdout);

        // Spawn a task to log stderr output
        let server_name_clone = server.name.clone();
        tokio::spawn(async move {
            let mut stderr_reader = BufReader::new(stderr);
            let mut line = String::new();
            while let Ok(bytes) = stderr_reader.read_line(&mut line).await {
                if bytes == 0 { break; }
                if !line.trim().is_empty() {
                    debug_eprintln!("[MCP] {} stderr: {}", server_name_clone, line.trim());
                }
                line.clear();
            }
        });

        let connection = McpConnection {
            process,
            stdin,
            stdout,
        };

        self.mcp_connections.lock().await.insert(server.name.clone(), connection);

        // Send initialization message
        self.initialize_mcp_connection(&server.name).await?;

        Ok(())
    }

    async fn initialize_mcp_connection(&self, server_name: &str) -> Result<(), anyhow::Error> {
        let init_message = json!({
            "jsonrpc": "2.0",
            "method": "initialize",
            "params": {
                "protocolVersion": "0.1.0",
                "capabilities": {
                    "roots": {
                        "listChanged": true
                    }
                }
            },
            "id": 1
        });

        self.send_mcp_message(server_name, &init_message).await?;
        // Read and process the response
        let init_response = self.read_mcp_response(server_name).await?;
        debug_println!("[MCP] Initialize response: {:?}", init_response);

        // Now request the list of tools
        let list_tools_message = json!({
            "jsonrpc": "2.0",
            "method": "tools/list",
            "params": {},
            "id": 2
        });

        debug_println!("[MCP] Requesting tool list from {}", server_name);
        self.send_mcp_message(server_name, &list_tools_message).await?;
        let tools_response = self.read_mcp_response(server_name).await?;

        // Parse the tools from the response
        if let Some(result) = tools_response.get("result") {
            if let Some(tools_array) = result.get("tools") {
                if let Ok(tools) = serde_json::from_value::<Vec<McpTool>>(tools_array.clone()) {
                    debug_println!("[MCP] {} tools discovered from {}:", tools.len(), server_name);
                    for tool in &tools {
                        debug_println!("[MCP]   • {}: {}", tool.name, tool.description.as_ref().unwrap_or(&"No description".to_string()));
                    }
                    self.mcp_tools.lock().unwrap().insert(server_name.to_string(), tools);
                } else {
                    debug_println!("[MCP] Failed to parse tools from response");
                }
            }
        }

        Ok(())
    }

    async fn send_mcp_message(&self, server_name: &str, message: &Value) -> Result<(), anyhow::Error> {
        debug_println!("[MCP] Sending message to {}: {}", server_name, message);

        let mut connections = self.mcp_connections.lock().await;
        let connection = connections.get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP server {} not connected", server_name))?;

        let msg_str = message.to_string();
        connection.stdin.write_all(msg_str.as_bytes()).await?;
        connection.stdin.write_all(b"\n").await?;
        connection.stdin.flush().await?;
        debug_println!("[MCP] Message sent to: {}", server_name);

        Ok(())
    }

    async fn read_mcp_response(&self, server_name: &str) -> Result<Value, anyhow::Error> {
        debug_println!("[MCP] Reading response from: {}", server_name);

        let mut connections = self.mcp_connections.lock().await;
        let connection = connections.get_mut(server_name)
            .ok_or_else(|| anyhow::anyhow!("MCP server {} not connected", server_name))?;

        // Keep reading lines until we get a valid JSON response
        // Some MCP servers might output debug info to stdout
        let mut attempts = 0;
        loop {
            let mut line = String::new();
            let bytes_read = connection.stdout.read_line(&mut line).await?;

            if bytes_read == 0 {
                return Err(anyhow::anyhow!("MCP server {} disconnected unexpectedly", server_name));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            debug_println!("[MCP] Raw response from {}: {}", server_name,
                if trimmed.len() > 200 {
                    format!("{}...", &trimmed[..200])
                } else {
                    trimmed.to_string()
                });

            // Try to parse as JSON
            match serde_json::from_str::<Value>(trimmed) {
                Ok(response) => {
                    debug_println!("[MCP] Successfully parsed JSON response");
                    return Ok(response);
                },
                Err(e) => {
                    // If it's not JSON, it might be debug output
                    if trimmed.starts_with('{') || trimmed.starts_with('[') {
                        // Looks like JSON but failed to parse
                        debug_println!("[MCP] Failed to parse JSON: {}", e);
                        if attempts > 5 {
                            return Err(anyhow::anyhow!("Failed to parse JSON response after retries"));
                        }
                    } else {
                        // Probably debug output, skip it
                        debug_println!("[MCP] Skipping non-JSON output: {}", trimmed);
                    }
                }
            }

            attempts += 1;
            if attempts > 10 {
                return Err(anyhow::anyhow!("Too many attempts reading MCP response"));
            }
        }
    }

    fn extract_json_path(&self, json: &Value, path: &str) -> Result<Value, anyhow::Error> {
        debug_println!("[JSON] Extracting path: {} from response", path);

        let mut current = json.clone();
        let parts: Vec<&str> = path.split('.').collect();

        for part in parts {
            // Handle array indexing like "results[0]"
            if part.contains('[') && part.contains(']') {
                let bracket_start = part.find('[').unwrap();
                let bracket_end = part.find(']').unwrap();
                let field_name = &part[..bracket_start];
                let index_str = &part[bracket_start + 1..bracket_end];

                // First get the field if it exists
                if !field_name.is_empty() {
                    current = current.get(field_name)
                        .ok_or_else(|| anyhow::anyhow!("Field '{}' not found in JSON", field_name))?
                        .clone();
                }

                // Then apply the index
                if let Ok(index) = index_str.parse::<usize>() {
                    current = current.as_array()
                        .ok_or_else(|| anyhow::anyhow!("Expected array at '{}'", part))?
                        .get(index)
                        .ok_or_else(|| anyhow::anyhow!("Index {} out of bounds", index))?
                        .clone();
                } else {
                    return Err(anyhow::anyhow!("Invalid array index: {}", index_str));
                }
            } else {
                // Simple field access
                current = current.get(part)
                    .ok_or_else(|| anyhow::anyhow!("Field '{}' not found in JSON", part))?
                    .clone();
            }
        }

        debug_println!("[JSON] Successfully extracted value from path: {}", path);
        Ok(current)
    }

    fn parse_value_by_type(&self, value: &str, param_type: &str) -> Value {
        match param_type.to_lowercase().as_str() {
            "number" => {
                // Try to parse as integer first, then as float
                if let Ok(n) = value.parse::<i64>() {
                    Value::Number(serde_json::Number::from(n))
                } else if let Ok(f) = value.parse::<f64>() {
                    serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or_else(|| Value::String(value.to_string()))
                } else {
                    Value::String(value.to_string())
                }
            },
            "boolean" | "bool" => {
                match value.to_lowercase().as_str() {
                    "true" | "1" | "yes" | "y" => Value::Bool(true),
                    "false" | "0" | "no" | "n" => Value::Bool(false),
                    _ => Value::String(value.to_string())
                }
            },
            "array" => {
                // Try to parse as JSON array
                if let Ok(arr) = serde_json::from_str::<Vec<Value>>(value) {
                    Value::Array(arr)
                } else {
                    // If not valid JSON, treat as comma-separated values
                    let items: Vec<Value> = value.split(',')
                        .map(|s| Value::String(s.trim().to_string()))
                        .collect();
                    Value::Array(items)
                }
            },
            "object" => {
                // Try to parse as JSON object
                serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
            },
            _ => Value::String(value.to_string())
        }
    }

    pub async fn execute_http_tool(&self, tool_name: &str, params: HashMap<String, String>)
        -> Result<Value, anyhow::Error> {

        // Apply rate limiting
        self.apply_rate_limit(tool_name).await;

        debug_println!("[HTTP] Executing tool: {} with params: {:?}", tool_name, params);
        let tool = self.config.tools.http.iter()
            .find(|t| t.name == tool_name)
            .ok_or_else(|| anyhow::anyhow!("HTTP tool '{}' not found", tool_name))?;

        // Separate path parameters from query/body parameters
        let mut path_param_values = HashMap::new();
        let mut final_params: HashMap<String, Value> = HashMap::new();

        for (key, param_def) in &tool.parameters {
            let value = if let Some(default) = &param_def.default {
                // Default values ALWAYS override what the LLM provides
                // This allows us to force certain parameter values
                match default {
                    Value::String(s) if s.starts_with("${") && s.ends_with("}") => {
                        // Environment variable substitution
                        let key_name = &s[2..s.len()-1];
                        let env_value = self.api_keys.get(key_name)
                            .cloned()
                            .or_else(|| std::env::var(key_name).ok())
                            .unwrap_or_else(|| s.clone());
                        self.parse_value_by_type(&env_value, &param_def.param_type)
                    },
                    _ => default.clone()
                }
            } else if let Some(string_value) = params.get(key) {
                // Parse the provided string value according to its type
                self.parse_value_by_type(string_value, &param_def.param_type)
            } else if param_def.required {
                return Err(anyhow::anyhow!("Missing required parameter: {}", key));
            } else {
                continue;
            };

            // Check if this is a path parameter (path params need to be strings)
            if tool.path_params.contains(key) {
                let string_value = match &value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => serde_json::to_string(&value)?
                };
                path_param_values.insert(key.clone(), string_value);
            } else {
                final_params.insert(key.clone(), value);
            }
        }

        // Process the endpoint URL with path parameters
        let mut final_endpoint = tool.endpoint.clone();

        // Replace {param_name} and :param_name placeholders in the URL
        for param_name in &tool.path_params {
            if let Some(value) = path_param_values.get(param_name) {
                debug_println!("[HTTP] Replacing path parameter '{}' with: {}", param_name, value);
                // Support both {param} and :param styles
                final_endpoint = final_endpoint
                    .replace(&format!("{{{}}}", param_name), value)
                    .replace(&format!(":{}", param_name), value);
            } else if tool.parameters.get(param_name).map_or(false, |p| p.required) {
                debug_println!("[HTTP] Warning: Required path parameter {} not found", param_name);
            }
        }

        debug_println!("[HTTP] Final endpoint after path substitution: {}", final_endpoint);

        // Process headers with environment variable substitution
        let mut request_builder = match tool.method.as_str() {
            "GET" => self.http_client.get(&final_endpoint),
            "POST" => self.http_client.post(&final_endpoint),
            "PUT" => self.http_client.put(&final_endpoint),
            "DELETE" => self.http_client.delete(&final_endpoint),
            "PATCH" => self.http_client.patch(&final_endpoint),
            _ => return Err(anyhow::anyhow!("Unsupported HTTP method: {}", tool.method)),
        };

        // Add headers with variable substitution
        for (header_name, header_value) in &tool.headers {
            let processed_value = if header_value.contains("${") {
                // Process environment variable substitution
                let mut value = header_value.clone();

                // Find all ${VAR_NAME} patterns and replace them
                while let Some(start) = value.find("${") {
                    if let Some(end) = value[start..].find('}') {
                        let var_name = &value[start + 2..start + end];

                        // Try api_keys first
                        let replacement = if let Some(api_key) = self.api_keys.get(var_name) {
                            api_key.clone()
                        } else if let Ok(env_val) = std::env::var(var_name) {
                            env_val
                        } else {
                            format!("${{{}}}", var_name)
                        };

                        value.replace_range(start..=start + end, &replacement);
                    } else {
                        break;
                    }
                }
                value
            } else {
                header_value.clone()
            };

            request_builder = request_builder.header(header_name, processed_value);
        }

        debug_println!("[HTTP] Making {} request to: {}", tool.method, tool.endpoint);

        // Add query parameters or JSON body based on method
        let response = match tool.method.as_str() {
            "GET" => {
                // Convert Values to strings for query parameters
                let query_params: HashMap<String, String> = final_params.iter()
                    .map(|(k, v)| {
                        let string_value = match v {
                            Value::String(s) => s.clone(),
                            Value::Number(n) => n.to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => serde_json::to_string(v).unwrap_or_default()
                        };
                        (k.clone(), string_value)
                    })
                    .collect();
                debug_println!("[HTTP] Adding query parameters: {:?}", query_params);
                request_builder
                    .query(&query_params)
                    .send()
                    .await?
            },
            "POST" => {
                debug_println!("[HTTP] Sending JSON body: {:?}", final_params);
                request_builder
                    .json(&final_params)
                    .send()
                    .await?
            },
            _ => unreachable!(),
        };

        let status_code = response.status().as_u16();
        debug_println!("[HTTP] Response status: {}", status_code);

        // Check if status is in acceptable_status list (should be ignored)
        if status_in_list(status_code, &tool.acceptable_status) {
            debug_println!("[HTTP] Status {} is acceptable, ignoring response", status_code);
            return Ok(json!({"status": "ignored", "status_code": status_code}));
        }

        // Check if status is in expected_status list
        let is_expected = status_in_list(status_code, &tool.expected_status);

        // Determine if we should throw an error
        let should_error = if !tool.error_status.is_empty() {
            // If error_status is specified, only error on those codes
            status_in_list(status_code, &tool.error_status)
        } else {
            // Otherwise, error on any non-expected status
            !is_expected
        };

        if should_error {
            let status = response.status();
            let error_body = response.text().await.unwrap_or_else(|_| "Could not read error response".to_string());
            // Always log HTTP error responses to console for debugging
            eprintln!("[HTTP Tool Error] Tool: {} | Status: {} | Response Body:\n{}",
                tool_name, status_code, error_body);
            debug_println!("[HTTP] Error response body: {}", error_body);
            return Err(anyhow::anyhow!(
                "HTTP {} error for tool '{}':\nStatus: {}\nResponse:\n{}",
                status_code, tool_name, status, error_body
            ));
        }

        let mut result = match tool.response_format.as_str() {
            "json" => response.json().await?,
            _ => json!({"text": response.text().await?}),
        };

        // Apply JSON path extraction if specified
        if let Some(path) = &tool.response_path {
            result = self.extract_json_path(&result, path)?;
        }

        debug_println!("[HTTP] Tool {} executed successfully", tool_name);
        Ok(result)
    }

    pub async fn execute_mcp_tool(&self, server_name: &str, tool_name: &str, params: Value)
        -> Result<Value, anyhow::Error> {

        // Apply rate limiting using combined name
        let rate_limit_key = format!("{}:{}", server_name, tool_name);
        self.apply_rate_limit(&rate_limit_key).await;

        debug_println!("[MCP] Executing tool '{}' on server: {}", tool_name, server_name);

        // MCP tools are called with tools/call method
        let message = json!({
            "jsonrpc": "2.0",
            "method": "tools/call",
            "params": {
                "name": tool_name,
                "arguments": params
            },
            "id": 3
        });

        self.send_mcp_message(server_name, &message).await?;
        let response = self.read_mcp_response(server_name).await?;
        debug_println!("[MCP] Tool execution completed for: {}", server_name);

        // Extract the result from the response
        if let Some(result) = response.get("result") {
            Ok(result.clone())
        } else if let Some(error) = response.get("error") {
            Err(anyhow::anyhow!("MCP error: {}", error))
        } else {
            Ok(response)
        }
    }

    pub async fn execute_builtin_tool(&self, tool_name: &str, params: HashMap<String, String>)
        -> Result<Value, anyhow::Error> {

        match tool_name {
            "pdf_extract" => self.builtin_pdf_extract(params).await,
            _ => Err(anyhow::anyhow!("Unknown built-in tool: {}", tool_name)),
        }
    }

    async fn builtin_pdf_extract(&self, params: HashMap<String, String>) -> Result<Value, anyhow::Error> {
        let url = params.get("url")
            .ok_or_else(|| anyhow::anyhow!("Missing 'url' parameter for pdf_extract"))?;

        debug_println!("[BuiltIn:PDF] Fetching PDF from: {}", url);

        // Download PDF
        let response = self.http_client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!("Failed to download PDF: HTTP {}", response.status()));
        }

        let pdf_bytes = response.bytes().await?;

        // Extract text from PDF
        let text = tokio::task::spawn_blocking(move || {
            pdf_extract::extract_text_from_mem(&pdf_bytes)
        }).await??;

        debug_println!("[BuiltIn:PDF] Extracted {} characters of text", text.len());

        Ok(json!({
            "text": text,
            "length": text.len(),
            "source": url
        }))
    }


    pub fn get_tool_descriptions(&self) -> Vec<ToolDescription> {
        let mut descriptions = Vec::new();

        // Built-in tools
        for tool_name in &self.config.tools.builtin {
            let (description, parameters) = match tool_name.as_str() {
                "pdf_extract" => (
                    "Extracts text content from a PDF file at a given URL. Returns the full text content of the PDF document.".to_string(),
                    vec![ParameterDescription {
                        name: "url".to_string(),
                        param_type: "string".to_string(),
                        description: "URL of the PDF file to extract text from. Must be a valid HTTP/HTTPS URL pointing to a PDF document.".to_string(),
                        required: true,
                    }]
                ),
                _ => continue,
            };

            descriptions.push(ToolDescription {
                name: tool_name.clone(),
                tool_type: "builtin".to_string(),
                description,
                parameters,
            });
        }

        // HTTP tools
        for tool in &self.config.tools.http {
            descriptions.push(ToolDescription {
                name: tool.name.clone(),
                tool_type: "http".to_string(),
                description: tool.description.clone(),
                parameters: tool.parameters.iter().map(|(name, def)| {
                    ParameterDescription {
                        name: name.clone(),
                        param_type: def.param_type.clone(),
                        description: def.description.clone(),
                        required: def.required,
                    }
                }).collect(),
            });
        }

        // MCP tools - include actual discovered tools
        let mcp_tools = self.mcp_tools.lock().unwrap();
        for (server_name, tools) in mcp_tools.iter() {
            for tool in tools {
                descriptions.push(ToolDescription {
                    name: format!("{}:{}", server_name, tool.name),  // Prefix with server name
                    tool_type: "mcp".to_string(),
                    description: tool.description.clone().unwrap_or_else(|| format!("MCP tool from {}", server_name)),
                    parameters: vec![],  // TODO: Parse from input_schema if needed
                });
            }
        }

        descriptions
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolDescription {
    pub name: String,
    pub tool_type: String,
    pub description: String,
    pub parameters: Vec<ParameterDescription>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ParameterDescription {
    pub name: String,
    pub param_type: String,
    pub description: String,
    pub required: bool,
}

#[allow(dead_code)]
pub fn load_tools_config(path: &str) -> Result<ToolsConfig, anyhow::Error> {
    debug_println!("[TOOLS] Loading tools configuration from: {}", path);
    let contents = std::fs::read_to_string(path)?;

    // Handle empty or invalid JSON files
    if contents.trim().is_empty() {
        debug_println!("[TOOLS] Configuration file is empty");
        return Ok(ToolsConfig {
            tools: Tools {
                builtin: Vec::new(),
                http: Vec::new(),
                mcp: Vec::new(),
            }
        });
    }

    let config: ToolsConfig = serde_json::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("Failed to parse tools.json: {}", e))?;

    debug_println!("[TOOLS] Successfully loaded {} HTTP tools and {} MCP servers",
        config.tools.http.len(),
        config.tools.mcp.len());

    Ok(config)
}

#[derive(Debug, Deserialize)]
struct ApiKeysConfig {
    keys: HashMap<String, String>,
}

pub fn load_api_keys(path: &std::path::Path) -> Result<HashMap<String, String>, anyhow::Error> {
    if !path.exists() {
        return Ok(HashMap::new());
    }

    let contents = std::fs::read_to_string(path)?;
    let config: ApiKeysConfig = toml::from_str(&contents)?;
    Ok(config.keys)
}
