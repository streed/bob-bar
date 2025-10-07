use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use anyhow::Result;
use crate::ollama::OllamaClient;
use crate::tools::ToolExecutor;
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub enum ResearchProgress {
    Started,
    Decomposing,
    PlanningIteration(usize, usize), // current iteration, max iterations
    PlanGenerated(usize), // number of sub-questions
    PlanCriticReviewing(usize, usize), // iteration, max
    PlanApproved,
    WorkersStarted(usize), // number of workers
    WorkerCompleted(String), // worker name
    #[allow(dead_code)]
    WorkerStarted { worker: String, question: String },
    WorkerStatus { worker: String, status: String },
    SupervisorAnalyzing,
    FollowUpQuestionsGenerated(usize), // number of follow-ups
    Combining,
    Summarizing,
    Refining(usize, usize), // current iteration, max iterations
    CriticReviewing,
    DebateRound(usize, usize), // current round, max rounds
    WritingDocument(usize, usize), // current iteration, max iterations
    DocumentReviewing,
    ExportingMemories,
    Completed,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentsConfig {
    pub agents: Agents,
    pub config: ResearchConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Agents {
    pub lead: AgentRole,
    pub workers: Vec<AgentRole>,
    pub debate_agents: Vec<AgentRole>,
    pub refiner: AgentRole,
    pub writer: AgentRole,
    pub document_critic: AgentRole,
    pub plan_critic: AgentRole,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentRole {
    pub name: String,
    pub role: String,
    pub description: String,
    pub system_prompt: String,
    pub available_tools: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ResearchConfig {
    pub min_worker_count: usize,
    pub max_worker_count: usize,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            min_worker_count: 3,
            max_worker_count: 10,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubQuestion {
    pub question: String,
    pub assigned_worker: String,
}

#[derive(Debug, Clone)]
pub struct WorkerResult {
    pub question: String,
    pub answer: String,
    pub worker_name: String,
}

pub struct ResearchOrchestrator {
    config: AgentsConfig,
    ollama_config: crate::config::OllamaConfig,
    base_client: Arc<Mutex<OllamaClient>>,
    tool_executor: Option<Arc<Mutex<ToolExecutor>>>,
    shared_memory: Option<Arc<crate::shared_memory::SharedMemory>>,
    progress_tx: Option<mpsc::UnboundedSender<ResearchProgress>>,
    context_window: usize,
    research_model: String,
    max_tool_turns: usize,
    query_id: Option<String>,  // Current research query ID for tracking history
    export_memories: bool,  // Whether to export memory summary to output
}

impl ResearchOrchestrator {
    pub fn new(config: AgentsConfig, ollama_config: crate::config::OllamaConfig, base_client: Arc<Mutex<OllamaClient>>, context_window: usize, research_model: String, max_tool_turns: usize) -> Self {
        // Initialize shared memory with embedding configuration
        let shared_memory = match crate::shared_memory::SharedMemory::new(
            ollama_config.host.clone(),
            ollama_config.embedding_model.clone(),
            ollama_config.embedding_dimensions,
        ) {
            Ok(mem) => {
                eprintln!("✓ Shared memory initialized successfully");
                Some(Arc::new(mem))
            }
            Err(e) => {
                eprintln!("⚠ Warning: Could not initialize shared memory: {}", e);
                eprintln!("  Research will continue without memory features");
                None
            }
        };

        eprintln!("[Research] Initializing with summarization_threshold_research = {} chars",
                  ollama_config.summarization_threshold_research);

        Self {
            config,
            ollama_config,
            base_client,
            tool_executor: None,
            shared_memory,
            progress_tx: None,
            context_window,
            research_model,
            max_tool_turns,
            query_id: None,
            export_memories: false,  // Default, will be overridden by config
        }
    }

    pub fn from_file(path: &std::path::Path, ollama_config: crate::config::OllamaConfig, base_client: Arc<Mutex<OllamaClient>>, context_window: usize, research_model: String, max_tool_turns: usize) -> Result<Self> {
        let config_str = std::fs::read_to_string(path)?;
        let config: AgentsConfig = serde_json::from_str(&config_str)?;
        Ok(Self::new(config, ollama_config, base_client, context_window, research_model, max_tool_turns))
    }

    /// Override config values from global config.toml
    pub fn override_config(&mut self, toml_config: &crate::config::ResearchConfig) {
        // Override worker count range from research config
        self.config.config.min_worker_count = toml_config.min_worker_count;
        self.config.config.max_worker_count = toml_config.max_worker_count;
        // Override export_memories setting
        self.export_memories = toml_config.export_memories;
    }

    pub fn set_tool_executor(&mut self, executor: Arc<Mutex<ToolExecutor>>) {
        // Set shared memory on the tool executor if available
        if let Some(ref shared_memory) = self.shared_memory {
            if let Ok(mut exec) = executor.try_lock() {
                exec.set_shared_memory(shared_memory.clone());
            }
        }
        self.tool_executor = Some(executor);
    }

    pub fn set_progress_channel(&mut self, tx: mpsc::UnboundedSender<ResearchProgress>) {
        self.progress_tx = Some(tx);
    }

    fn send_progress(&self, progress: ResearchProgress) {
        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(progress.clone());
        }
        // Also log a human-readable line for the UI verbose log
        use crate::progress::{log_with, Kind};
        let (line, kind) = match progress {
            ResearchProgress::Started => ("Research started".to_string(), Kind::Info),
            ResearchProgress::Decomposing => ("Decomposing query into sub-questions".to_string(), Kind::Info),
            ResearchProgress::PlanningIteration(i, max) => (format!("Planning iteration {}/{}", i, max), Kind::Info),
            ResearchProgress::PlanGenerated(n) => (format!("Generated plan with {} sub-questions", n), Kind::Info),
            ResearchProgress::PlanCriticReviewing(i, max) => (format!("Plan critic reviewing (iteration {}/{})", i, max), Kind::Debate),
            ResearchProgress::PlanApproved => ("Plan approved, starting research".to_string(), Kind::Info),
            ResearchProgress::WorkersStarted(n) => (format!("Dispatching {} workers", n), Kind::Worker),
            ResearchProgress::WorkerCompleted(name) => (format!("✓ Worker completed: {}", name), Kind::Worker),
            ResearchProgress::WorkerStarted { worker, question } => (format!("→ {} researching: {}", worker, question), Kind::Worker),
            ResearchProgress::WorkerStatus { worker, status } => {
                let k = match worker.as_str() {
                    "Debate" => Kind::Debate,
                    "Refiner" => Kind::Refiner,
                    "Writer" => Kind::Writer,
                    "DocumentCritic" => Kind::DocumentCritic,
                    "Combiner" => Kind::Combiner,
                    _ => Kind::Worker,
                };
                (format!("{}: {}", worker, status), k)
            },
            ResearchProgress::SupervisorAnalyzing => ("Supervisor analyzing progress".to_string(), Kind::Info),
            ResearchProgress::FollowUpQuestionsGenerated(n) => (format!("Generated {} follow-up questions", n), Kind::Worker),
            ResearchProgress::Combining => ("Combining results".to_string(), Kind::Combiner),
            ResearchProgress::Summarizing => ("Summarizing worker results".to_string(), Kind::Combiner),
            ResearchProgress::Refining(i, max) => (format!("Refining output (iteration {}/{})", i, max), Kind::Refiner),
            ResearchProgress::CriticReviewing => ("Critic reviewing output".to_string(), Kind::Debate),
            ResearchProgress::DebateRound(i, max) => (format!("Debate round {}/{}", i, max), Kind::Debate),
            ResearchProgress::WritingDocument(i, max) => (format!("Writing document (iteration {}/{})", i, max), Kind::Writer),
            ResearchProgress::DocumentReviewing => ("Document critic reviewing".to_string(), Kind::DocumentCritic),
            ResearchProgress::ExportingMemories => ("Exporting research memories".to_string(), Kind::Info),
            ResearchProgress::Completed => ("Research complete".to_string(), Kind::Info),
        };
        log_with(kind, line);
    }

    fn summarize_arg(text: &str, max: usize) -> String {
        // Collapse whitespace and newlines to single spaces
        let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.len() <= max { return collapsed; }
        let mut s = collapsed.chars().take(max.saturating_sub(1)).collect::<String>();
        s.push('…');
        s
    }

    /// Main entry point for research mode
    pub async fn research(&mut self, query: &str) -> Result<String> {
        // Generate unique query ID for this research session using timestamp + random
        use std::time::{SystemTime, UNIX_EPOCH};
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let random: u32 = (timestamp % 10000) as u32; // Simple pseudo-random from timestamp
        let query_id = format!("query_{}_{}", timestamp, random);
        self.query_id = Some(query_id.clone());

        // Set query_id on tool executor for memory tools
        if let Some(ref executor) = self.tool_executor {
            if let Ok(mut exec) = executor.try_lock() {
                exec.set_query_id(query_id.clone());
            }
        }

        eprintln!("[Research] Starting query: {} (ID: {})", query, query_id);
        self.send_progress(ResearchProgress::Started);

        // Clear previous memories from database to start fresh
        if let Some(ref shared_memory) = self.shared_memory {
            eprintln!("[Research] Clearing previous memories from database...");
            if let Err(e) = shared_memory.clear().await {
                eprintln!("[Research] Warning: Failed to clear memories: {}", e);
            } else {
                eprintln!("[Research] ✓ Memories cleared, starting fresh research session");
            }
        }

        // Step 1: Decompose query into sub-questions and create plan
        self.send_progress(ResearchProgress::Decomposing);
        let (sub_questions, plan) = self.decompose_query_and_plan(query).await?;

        if sub_questions.is_empty() {
            return Ok("Unable to decompose query into sub-questions.".to_string());
        }

        // Store the initial plan in shared memory
        if let Some(ref shared_memory) = self.shared_memory {
            let plan_content = format!(
                "Research Plan for: {}\n\nSub-questions:\n{}\n\nStrategy:\n{}",
                query,
                sub_questions.iter().enumerate()
                    .map(|(i, sq)| format!("{}. [{}] {}", i + 1, sq.assigned_worker, sq.question))
                    .collect::<Vec<_>>()
                    .join("\n"),
                plan
            );

            let mut metadata = std::collections::HashMap::new();
            metadata.insert("query_text".to_string(), query.to_string());
            // Add query_id for session tracking
            if let Some(ref qid) = self.query_id {
                metadata.insert("query_id".to_string(), qid.clone());
            }

            match shared_memory.store_memory(
                crate::shared_memory::MemoryType::Plan,
                plan_content,
                "lead_researcher".to_string(),
                Some(metadata)
            ).await {
                Ok(_) => eprintln!("[Research] Plan stored in shared memory"),
                Err(e) => eprintln!("[Research] Warning: Failed to store plan: {}", e),
            }
        }

        // Step 2: Execute workers with iterative refinement and supervisor monitoring
        self.send_progress(ResearchProgress::WorkersStarted(sub_questions.len()));
        let worker_results = self.execute_workers_with_refinement(&sub_questions, query).await?;

        // Step 3: Combine results (with summarization if needed)
        self.send_progress(ResearchProgress::Combining);
        let combined_output = self.combine_results(query, &worker_results).await?;

        // Step 4: Refinement loop with critic
        let refined_output = self.refinement_loop(&combined_output).await?;

        // Step 5: Document writing loop with document critic
        let mut final_document = self.document_writing_loop(query, &refined_output).await?;

        // Step 6: Optionally append memory summary and clear database
        if let Some(ref shared_memory) = self.shared_memory {
            // Check config for memory export
            if self.export_memories {
                self.send_progress(ResearchProgress::ExportingMemories);
                // Get all memories from current query
                let stats = shared_memory.get_stats().await;
                let discoveries = shared_memory.get_by_type(crate::shared_memory::MemoryType::Discovery).await;
                let insights = shared_memory.get_by_type(crate::shared_memory::MemoryType::Insight).await;
                let deadends = shared_memory.get_by_type(crate::shared_memory::MemoryType::Deadend).await;
                let feedback = shared_memory.get_by_type(crate::shared_memory::MemoryType::Feedback).await;

                // Get tool calls for this query
                let tool_calls = shared_memory.get_tool_calls(self.query_id.as_deref()).await.unwrap_or_default();

                // Group tool calls by agent
                let mut agent_tool_calls: std::collections::HashMap<String, Vec<&crate::shared_memory::ToolCall>> = std::collections::HashMap::new();
                for tc in &tool_calls {
                    agent_tool_calls.entry(tc.agent_name.clone()).or_default().push(tc);
                }

                // Format tool calls section
                let tool_calls_section = if !agent_tool_calls.is_empty() {
                    let mut sections = vec!["### Tool Usage by Agent\n".to_string()];
                    let mut sorted_agents: Vec<_> = agent_tool_calls.keys().collect();
                    sorted_agents.sort();

                    for agent_name in sorted_agents {
                        let calls = &agent_tool_calls[agent_name];
                        sections.push(format!("\n**{}**:\n", agent_name));
                        for tc in calls {
                            // Parse parameters for better display
                            let params_display = if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&tc.parameters) {
                                if let Some(obj) = parsed.as_object() {
                                    if obj.is_empty() {
                                        "(no params)".to_string()
                                    } else {
                                        let params_str: Vec<String> = obj.iter()
                                            .map(|(k, v)| {
                                                let val_str = match v {
                                                    serde_json::Value::String(s) => {
                                                        if s.len() > 50 {
                                                            format!("{}...", &s[..50])
                                                        } else {
                                                            s.clone()
                                                        }
                                                    },
                                                    _ => v.to_string()
                                                };
                                                format!("{}={}", k, val_str)
                                            })
                                            .collect();
                                        format!("({})", params_str.join(", "))
                                    }
                                } else {
                                    tc.parameters.clone()
                                }
                            } else {
                                tc.parameters.clone()
                            };
                            sections.push(format!("  - `[{}] {}` {}\n", tc.tool_type, tc.tool_name, params_display));
                        }
                    }
                    sections.join("")
                } else {
                    "### Tool Usage\n\nNo tools were used during this research.\n".to_string()
                };

                // Append memory summary to document
                let memory_summary = format!(
                    "\n\n---\n\n## Research Memory Summary\n\n\
                    **Total Memories**: {} (Discoveries: {}, Insights: {}, Deadends: {}, Feedback: {})\n\n\
                    {}\n\
                    ### Discoveries ({})\n\n{}\n\n\
                    ### Insights ({})\n\n{}\n\n\
                    ### Deadends ({})\n\n{}\n\n\
                    ### Supervisor Feedback ({})\n\n{}\n",
                    stats.total_count,
                    stats.discovery_count,
                    stats.insight_count,
                    stats.deadend_count,
                    stats.feedback_count,
                    tool_calls_section,
                    discoveries.len(),
                    discoveries.iter()
                        .map(|d| format!("- **[{}]**: {}", d.created_by, d.content))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    insights.len(),
                    insights.iter()
                        .map(|i| format!("- **[{}]**: {}", i.created_by, i.content))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    deadends.len(),
                    deadends.iter()
                        .map(|d| format!("- **[{}]**: {}", d.created_by, d.content))
                        .collect::<Vec<_>>()
                        .join("\n"),
                    feedback.len(),
                    feedback.iter()
                        .map(|f| format!("- **Iteration {}**: {}",
                            f.metadata.get("iteration").unwrap_or(&"?".to_string()),
                            f.content))
                        .collect::<Vec<_>>()
                        .join("\n")
                );

                final_document.push_str(&memory_summary);
                eprintln!("[Research] Memory summary appended to output");
            }

            // Clear memories for this query
            match shared_memory.clear().await {
                Ok(_) => eprintln!("[Research] Shared memory cleared for next query"),
                Err(e) => eprintln!("[Research] Warning: Failed to clear memory: {}", e),
            }
        }

        self.send_progress(ResearchProgress::Completed);
        Ok(final_document)
    }

    /// Decompose query into sub-questions and create research plan using lead agent
    async fn decompose_query_and_plan(&self, query: &str) -> Result<(Vec<SubQuestion>, String)> {
        let max_iterations = self.ollama_config.max_plan_iterations;
        let mut current_plan = String::new();
        let mut current_questions_json = String::new();

        for iteration in 0..max_iterations {
            self.send_progress(ResearchProgress::PlanningIteration(iteration + 1, max_iterations));
            eprintln!("[Research] Planning iteration {}/{}", iteration + 1, max_iterations);

            // Generate or refine the plan
            let (plan_response, questions_json) = if iteration == 0 {
                // Initial plan generation
                self.generate_initial_plan(query).await?
            } else {
                // Refine plan based on criticism
                self.refine_plan(query, &current_plan, &current_questions_json).await?
            };

            current_plan = plan_response;
            current_questions_json = questions_json.clone();

            // Parse to get question count for progress
            if let Ok(parsed) = serde_json::from_str::<Vec<SubQuestion>>(&questions_json) {
                self.send_progress(ResearchProgress::PlanGenerated(parsed.len()));
            }

            // Get plan critic feedback
            self.send_progress(ResearchProgress::PlanCriticReviewing(iteration + 1, max_iterations));
            eprintln!("[Research] Reviewing plan with plan critic...");
            let criticism = self.review_plan(query, &current_plan, &questions_json).await?;

            // Check if approved
            if criticism.trim().to_uppercase().starts_with("APPROVED") {
                self.send_progress(ResearchProgress::PlanApproved);
                eprintln!("[Research] Plan approved after {} iteration(s)", iteration + 1);
                break;
            }

            // If not approved and not last iteration, we'll refine in next iteration
            eprintln!("[Research] Plan iteration {}: Feedback received, will revise", iteration + 1);

            // On last iteration, use what we have
            if iteration == max_iterations - 1 {
                self.send_progress(ResearchProgress::PlanApproved);
                eprintln!("[Research] Max plan iterations reached. Using current plan.");
            }
        }

        // Parse the final plan
        self.parse_plan(&current_questions_json, &current_plan).await
    }

    /// Generate initial research plan
    async fn generate_initial_plan(&self, query: &str) -> Result<(String, String)> {
        let prompt = format!(
            "{}\n\n**WORKER COUNT GUIDANCE**: Based on query complexity, create between {} and {} sub-questions. \
            Simple queries should use fewer workers (closer to {}), while complex multi-faceted queries should \
            use more workers (closer to {}). The number of sub-questions determines how many workers will be spawned.\n\n\
            Query: {}\n\nProvide your response in two parts:\n\
            1. JSON array of sub-questions (as before)\n\
            2. After the JSON, provide a brief research strategy/plan explaining the approach and what to focus on.",
            self.config.agents.lead.system_prompt,
            self.config.config.min_worker_count,
            self.config.config.max_worker_count,
            self.config.config.min_worker_count,
            self.config.config.max_worker_count,
            query
        );

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut lead_client = OllamaClient::with_config(base_url, self.research_model.clone());
        lead_client.set_max_tool_turns(self.max_tool_turns);

        let response = lead_client.query_streaming(&prompt, |_| {}).await?;

        // Extract JSON array
        let json = self.extract_json_array(&response)?;

        Ok((response, json))
    }

    /// Refine plan based on critic feedback
    async fn refine_plan(&self, query: &str, previous_plan: &str, _previous_json: &str) -> Result<(String, String)> {
        let prompt = format!(
            "{}\n\n**WORKER COUNT GUIDANCE**: Based on query complexity, create between {} and {} sub-questions.\n\n\
            Original Query: {}\n\n\
            Previous Plan (with feedback):\n{}\n\n\
            Revise the plan to address the feedback. Provide:\n\
            1. JSON array of revised sub-questions\n\
            2. After the JSON, provide updated research strategy",
            self.config.agents.lead.system_prompt,
            self.config.config.min_worker_count,
            self.config.config.max_worker_count,
            query,
            previous_plan
        );

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut lead_client = OllamaClient::with_config(base_url, self.research_model.clone());
        lead_client.set_max_tool_turns(self.max_tool_turns);

        let response = lead_client.query_streaming(&prompt, |_| {}).await?;

        // Extract JSON array
        let json = self.extract_json_array(&response)?;

        Ok((response, json))
    }

    /// Review plan with plan critic
    async fn review_plan(&self, query: &str, plan: &str, questions_json: &str) -> Result<String> {
        tokio::time::sleep(tokio::time::Duration::from_millis(self.ollama_config.api_delay_ms)).await;

        let prompt = format!(
            "{}\n\nOriginal Query: {}\n\n\
            Research Plan:\n{}\n\n\
            Questions (JSON):\n{}",
            self.config.agents.plan_critic.system_prompt,
            query,
            plan,
            questions_json
        );

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut critic_client = OllamaClient::with_config(base_url, self.research_model.clone());
        critic_client.set_max_tool_turns(self.max_tool_turns);
        let review = critic_client.query_streaming(&prompt, |_| {}).await?;

        Ok(review)
    }

    /// Parse plan into sub-questions
    async fn parse_plan(&self, questions_json: &str, plan_text: &str) -> Result<(Vec<SubQuestion>, String)> {
        #[derive(Deserialize)]
        struct QuestionAssignment {
            question: String,
            worker: String,
        }

        let assignments: Vec<QuestionAssignment> = serde_json::from_str(questions_json)?;

        // Extract plan/strategy from plan_text
        let plan = if let Some(json_end) = plan_text.rfind(']') {
            plan_text[json_end + 1..].trim().to_string()
        } else {
            plan_text.trim().to_string()
        };

        // Log the planner's decisions in debug mode
        if std::env::var("BOBBAR_DEBUG").is_ok() {
            eprintln!("\n[Research Planner] Decomposed query into {} sub-questions:", assignments.len());
            for (i, assignment) in assignments.iter().enumerate() {
                eprintln!("  {}. [{}] {}", i + 1, assignment.worker, assignment.question);
            }
            eprintln!("\n[Research Strategy] {}\n", plan);
        }

        // Map worker role to actual worker name
        let mut sub_questions = Vec::new();
        for assignment in assignments {
            // Find worker by role
            let worker = self.config.agents.workers
                .iter()
                .find(|w| w.role == assignment.worker)
                .or_else(|| {
                    // Fallback: try to match by name if role doesn't match
                    self.config.agents.workers
                        .iter()
                        .find(|w| w.name.to_lowercase().contains(&assignment.worker.to_lowercase()))
                })
                .ok_or_else(|| anyhow::anyhow!("Worker role not found: {}", assignment.worker))?;

            sub_questions.push(SubQuestion {
                question: assignment.question,
                assigned_worker: worker.name.clone(),
            });
        }

        Ok((sub_questions, plan))
    }

    /// Generate follow-up questions based on early worker results (static version for use in spawned tasks)
    async fn generate_follow_up_questions_static(
        original_query: &str,
        early_results: &[WorkerResult],
        research_model: &str,
        max_tool_turns: usize,
        _ollama_config: &crate::config::OllamaConfig,
    ) -> Result<Vec<SubQuestion>> {
        if early_results.is_empty() {
            return Ok(Vec::new());
        }

        // Summarize early findings
        let early_findings = early_results.iter()
            .map(|r| format!("Worker: {}\nQuestion: {}\nKey Findings: {}",
                r.worker_name,
                r.question,
                // Truncate to first 300 chars to keep prompt manageable
                r.answer.chars().take(300).collect::<String>()
            ))
            .collect::<Vec<_>>()
            .join("\n\n");

        let prompt = format!(
            "You are analyzing early research findings to identify gaps and opportunities.\n\n\
            ORIGINAL QUERY: {}\n\n\
            EARLY RESEARCH FINDINGS (first {} workers completed):\n{}\n\n\
            Based on these early findings, generate 2-4 follow-up questions to:\n\
            1. Fill gaps in coverage that early results revealed\n\
            2. Resolve any contradictions or inconsistencies\n\
            3. Explore promising areas more deeply\n\
            4. Investigate new angles that emerged from findings\n\n\
            CRITICAL: Return ONLY valid JSON array. No markdown, no text, no code blocks.\n\
            Format: [{{\"question\": \"...\", \"worker\": \"...\"}}]\n\n\
            Available workers: web_researcher, technical_analyst, data_specialist, comparative_analyst, news_researcher",
            original_query,
            early_results.len(),
            early_findings
        );

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut refinement_client = OllamaClient::with_config(base_url, research_model.to_string());
        refinement_client.set_max_tool_turns(max_tool_turns);

        let response = refinement_client.query_streaming(&prompt, |_| {}).await?;

        // Parse JSON response - need to extract array ourselves since we don't have access to self
        let cleaned = if let Some(start) = response.find('[') {
            if let Some(end) = response.rfind(']') {
                response[start..=end].to_string()
            } else {
                response.clone()
            }
        } else {
            response.clone()
        };

        #[derive(Deserialize)]
        struct QuestionAssignment {
            question: String,
            worker: String,
        }

        let assignments: Vec<QuestionAssignment> = serde_json::from_str(&cleaned)
            .unwrap_or_else(|_| Vec::new()); // If parsing fails, return empty (graceful degradation)

        let follow_ups: Vec<SubQuestion> = assignments.into_iter()
            .map(|qa| SubQuestion {
                question: qa.question,
                assigned_worker: qa.worker,
            })
            .collect();

        eprintln!("[Research] Generated {} follow-up questions based on early results", follow_ups.len());

        Ok(follow_ups)
    }

    /// Execute workers with iterative refinement: start initial workers, then add follow-ups based on early results
    async fn execute_workers_with_refinement(&self, initial_questions: &[SubQuestion], query: &str) -> Result<Vec<WorkerResult>> {
        if initial_questions.is_empty() {
            return Ok(Vec::new());
        }

        // Create channel for supervisor to request additional gap-filling workers
        let (gap_tx, mut gap_rx) = mpsc::channel::<Vec<SubQuestion>>(1);

        // Spawn supervisor task
        let shared_memory = self.shared_memory.clone();
        let ollama_config = self.ollama_config.clone();
        let research_model = self.research_model.clone();
        let max_tool_turns = self.max_tool_turns;
        let query_owned = query.to_string();
        let query_id = self.query_id.clone();
        let min_worker_count = self.config.config.min_worker_count;
        let max_worker_count = self.config.config.max_worker_count;
        let initial_worker_count = initial_questions.len();

        let supervisor_handle = if shared_memory.is_some() {
            Some(tokio::spawn(async move {
                Self::supervisor_loop(
                    shared_memory.unwrap(),
                    ollama_config,
                    research_model,
                    max_tool_turns,
                    query_owned,
                    query_id,
                    gap_tx,
                    min_worker_count,
                    max_worker_count,
                    initial_worker_count
                ).await
            }))
        } else {
            None
        };

        // Set up channel for worker results (with extra capacity for follow-ups)
        let (tx, mut rx) = mpsc::channel(initial_questions.len() + 10);

        // Launch initial workers
        let mut handles = Vec::new();
        for sub_q in initial_questions {
            let tx = tx.clone();
            let sub_q = sub_q.clone();
            let worker = self.config.agents.workers
                .iter()
                .find(|w| w.name == sub_q.assigned_worker)
                .cloned();

            if worker.is_none() {
                eprintln!("[Research] Warning: No worker found for {}", sub_q.assigned_worker);
                continue;
            }

            let base_client = self.base_client.clone();
            let tool_executor = self.tool_executor.clone();
            let research_model = self.research_model.clone();
            let max_tool_turns = self.max_tool_turns;
            let progress_tx = self.progress_tx.clone();
            let shared_memory = self.shared_memory.clone();

            let api_delay_ms = self.ollama_config.api_delay_ms;
            let summarization_threshold_research = self.ollama_config.summarization_threshold_research;
            let handle = tokio::spawn(async move {
                let result = Self::execute_worker(
                    worker.unwrap(),
                    &sub_q.question,
                    base_client,
                    tool_executor,
                    research_model,
                    max_tool_turns,
                    progress_tx,
                    shared_memory,
                    api_delay_ms,
                    summarization_threshold_research,
                )
                .await;

                let worker_result = WorkerResult {
                    question: sub_q.question.clone(),
                    answer: result.unwrap_or_else(|e| format!("Error: {}", e)),
                    worker_name: sub_q.assigned_worker.clone(),
                };

                let _ = tx.send(worker_result).await;
            });

            handles.push(handle);
        }

        // Don't drop tx yet - we may spawn gap-filling workers
        // Keep track of active workers and gap-filling state
        let mut active_workers = initial_questions.len();
        let mut gap_workers_spawned = false;
        let total_initial_workers = initial_questions.len();

        // Collect results, triggering refinement after first 2-3 completions
        let mut all_results = Vec::new();
        let mut early_results_for_refinement = Vec::new();
        let mut refinement_triggered = false;
        let early_threshold = 2.min(initial_questions.len());

        // Calculate midpoint for gap detection (halfway through initial workers)
        let midpoint_threshold = (total_initial_workers + 1) / 2;

        loop {
            tokio::select! {
                // Handle gap-filling worker requests from supervisor
                Some(gap_questions) = gap_rx.recv() => {
                    if !gap_workers_spawned {
                        gap_workers_spawned = true;
                        eprintln!("[Research] Supervisor requested {} gap-filling workers", gap_questions.len());

                        for sub_q in gap_questions {
                            let tx = tx.clone();
                            let worker = self.config.agents.workers
                                .iter()
                                .find(|w| w.name == sub_q.assigned_worker)
                                .cloned();

                            if worker.is_none() {
                                eprintln!("[Research] Warning: No worker found for {}", sub_q.assigned_worker);
                                continue;
                            }

                            active_workers += 1;
                            let base_client = self.base_client.clone();
                            let tool_executor = self.tool_executor.clone();
                            let research_model = self.research_model.clone();
                            let max_tool_turns = self.max_tool_turns;
                            let progress_tx = self.progress_tx.clone();
                            let shared_memory = self.shared_memory.clone();

            let api_delay_ms = self.ollama_config.api_delay_ms;
            let summarization_threshold_research = self.ollama_config.summarization_threshold_research;
                            let handle = tokio::spawn(async move {
                                let result = Self::execute_worker(
                                    worker.unwrap(),
                                    &sub_q.question,
                                    base_client,
                                    tool_executor,
                                    research_model,
                                    max_tool_turns,
                                    progress_tx,
                                    shared_memory,
                                    api_delay_ms,
                                    summarization_threshold_research,
                                )
                                .await;

                                let worker_result = WorkerResult {
                                    question: sub_q.question.clone(),
                                    answer: result.unwrap_or_else(|e| format!("Error: {}", e)),
                                    worker_name: sub_q.assigned_worker.clone(),
                                };

                                let _ = tx.send(worker_result).await;
                            });

                            handles.push(handle);
                        }
                    }
                }

                // Handle worker results
                Some(result) = rx.recv() => {
            self.send_progress(ResearchProgress::WorkerCompleted(result.worker_name.clone()));
            all_results.push(result.clone());

            // Store completion progress for supervisor to track
            if let Some(ref shared_memory) = self.shared_memory {
                let progress_content = format!("Workers completed: {}/{}", all_results.len(), total_initial_workers);
                let mut metadata = std::collections::HashMap::new();
                metadata.insert("completed_count".to_string(), all_results.len().to_string());
                metadata.insert("total_initial_workers".to_string(), total_initial_workers.to_string());
                metadata.insert("midpoint_threshold".to_string(), midpoint_threshold.to_string());
                if let Some(ref qid) = self.query_id {
                    metadata.insert("query_id".to_string(), qid.clone());
                }
                let _ = shared_memory.store_memory(
                    crate::shared_memory::MemoryType::Context,
                    progress_content,
                    "executor".to_string(),
                    Some(metadata)
                ).await;
            }

            // Collect early results for refinement
            if all_results.len() <= early_threshold && !refinement_triggered {
                early_results_for_refinement.push(result);

                // Trigger refinement after collecting enough early results
                if early_results_for_refinement.len() == early_threshold {
                    refinement_triggered = true;

                    // Generate and launch follow-up questions in background
                    let query_clone = query.to_string();
                    let early_clone = early_results_for_refinement.clone();
                    let config = self.config.clone();
                    let base_client = self.base_client.clone();
                    let tool_executor = self.tool_executor.clone();
                    let research_model = self.research_model.clone();
                    let max_tool_turns = self.max_tool_turns;
                    let progress_tx = self.progress_tx.clone();
                    let ollama_config = self.ollama_config.clone();
                    let shared_memory = self.shared_memory.clone();
                    let api_delay_ms_clone = self.ollama_config.api_delay_ms;
                    let summarization_threshold_research = self.ollama_config.summarization_threshold_research;

                    tokio::spawn(async move {
                        // Generate follow-up questions
                        let follow_ups = Self::generate_follow_up_questions_static(
                            &query_clone,
                            &early_clone,
                            &research_model,
                            max_tool_turns,
                            &ollama_config
                        ).await;

                        if let Ok(follow_ups) = follow_ups {
                            if !follow_ups.is_empty() {
                                eprintln!("[Research] Launching {} follow-up workers...", follow_ups.len());
                                // Send progress update (need to access progress_tx from outer scope)
                                if let Some(ref ptx) = progress_tx {
                                    let _ = ptx.send(ResearchProgress::FollowUpQuestionsGenerated(follow_ups.len()));
                                }
                                // Launch follow-up workers
                                for follow_up in follow_ups {
                                    let worker = config.agents.workers
                                        .iter()
                                        .find(|w| w.name == follow_up.assigned_worker)
                                        .cloned();

                                    if let Some(worker) = worker {
                                        let base_client = base_client.clone();
                                        let tool_executor = tool_executor.clone();
                                        let research_model = research_model.clone();
                                        let progress_tx = progress_tx.clone();
                                        let shared_memory = shared_memory.clone();
                                        let api_delay_ms = api_delay_ms_clone;
                                        let summarization_threshold_research = summarization_threshold_research;

                                        tokio::spawn(async move {
                                            let _ = Self::execute_worker(
                                                worker,
                                                &follow_up.question,
                                                base_client,
                                                tool_executor,
                                                research_model,
                                                max_tool_turns,
                                                progress_tx,
                                                shared_memory,
                                                api_delay_ms,
                                                summarization_threshold_research,
                                            ).await;
                                        });
                                    }
                                }
                            }
                        }
                    });
                }
            }

                    // Check if all workers have completed
                    active_workers -= 1;
                    if active_workers == 0 {
                        break;
                    }
                }

                else => break,
            }
        }

        // Wait for initial worker handles to complete
        for handle in handles {
            let _ = handle.await;
        }

        // Stop supervisor
        if let Some(handle) = supervisor_handle {
            handle.abort();
        }

        Ok(all_results)
    }

    /// Execute workers with active supervision from lead researcher
    #[allow(dead_code)]
    async fn execute_workers_with_supervision(&self, sub_questions: &[SubQuestion], query: &str) -> Result<Vec<WorkerResult>> {
        // Create dummy gap worker channel (not used in this mode)
        let (gap_tx, _gap_rx) = mpsc::channel::<Vec<SubQuestion>>(1);

        // Spawn supervisor task
        let shared_memory = self.shared_memory.clone();
        let ollama_config = self.ollama_config.clone();
        let research_model = self.research_model.clone();
        let max_tool_turns = self.max_tool_turns;
        let query_owned = query.to_string();
        let query_id = self.query_id.clone();
        let min_worker_count = self.config.config.min_worker_count;
        let max_worker_count = self.config.config.max_worker_count;
        let initial_worker_count = sub_questions.len();

        let supervisor_handle = if shared_memory.is_some() {
            Some(tokio::spawn(async move {
                Self::supervisor_loop(
                    shared_memory.unwrap(),
                    ollama_config,
                    research_model,
                    max_tool_turns,
                    query_owned,
                    query_id,
                    gap_tx,
                    min_worker_count,
                    max_worker_count,
                    initial_worker_count
                ).await
            }))
        } else {
            None
        };

        // Execute workers as normal
        let results = self.execute_workers(sub_questions).await?;

        // Stop supervisor
        if let Some(handle) = supervisor_handle {
            handle.abort();
        }

        Ok(results)
    }

    /// Supervisor loop - monitors memory and provides guidance
    /// Can spawn 1-3 gap-filling workers once during research if gaps detected
    async fn supervisor_loop(
        shared_memory: Arc<crate::shared_memory::SharedMemory>,
        ollama_config: crate::config::OllamaConfig,
        research_model: String,
        max_tool_turns: usize,
        query: String,
        query_id: Option<String>,
        gap_worker_tx: mpsc::Sender<Vec<SubQuestion>>,
        _min_worker_count: usize,
        max_worker_count: usize,
        initial_worker_count: usize,
    ) {
        let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(15));
        let mut iteration = 0;
        let mut gap_workers_requested = false;

        loop {
            interval.tick().await;
            iteration += 1;

            // Get MOST RECENT memories (not just similar) - workers may be producing new findings
            // get_by_type returns memories ordered by creation time
            let mut discoveries = shared_memory.get_by_type(crate::shared_memory::MemoryType::Discovery).await;
            let mut insights = shared_memory.get_by_type(crate::shared_memory::MemoryType::Insight).await;
            let deadends = shared_memory.get_by_type(crate::shared_memory::MemoryType::Deadend).await;
            let plans = shared_memory.get_by_type(crate::shared_memory::MemoryType::Plan).await;
            let progress_contexts = shared_memory.get_by_type(crate::shared_memory::MemoryType::Context).await;

            // Check worker completion progress for gap detection trigger
            let (completed_count, midpoint_threshold) = if let Some(latest_progress) = progress_contexts.last() {
                let completed = latest_progress.metadata.get("completed_count")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(0);
                let midpoint = latest_progress.metadata.get("midpoint_threshold")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or((initial_worker_count + 1) / 2);
                (completed, midpoint)
            } else {
                (0, (initial_worker_count + 1) / 2)
            };

            // Reverse to get newest first
            discoveries.reverse();
            insights.reverse();

            // Take most recent discoveries and insights (limit to avoid token overflow)
            let discoveries: Vec<_> = discoveries.into_iter().take(20).collect();
            let insights: Vec<_> = insights.into_iter().take(10).collect();

            // Skip first iteration if nothing to review yet
            if discoveries.is_empty() && insights.is_empty() && iteration == 1 {
                eprintln!("[Supervisor] Iteration {}: No discoveries/insights yet, skipping", iteration);
                continue;
            }

            // After first iteration, always provide feedback even if workers haven't produced much
            // This helps guide workers who may be stuck or off-track

            eprintln!("[Supervisor] Iteration {}: Reviewing {} discoveries, {} insights, {} deadends",
                iteration, discoveries.len(), insights.len(), deadends.len());

            // Get the plan
            let plan_content = plans.first().map(|p| p.content.as_str()).unwrap_or("No plan found");

            // Create summary of current state
            let discoveries_summary = discoveries.iter()
                .map(|d| format!("- {} (by {})", d.content.chars().take(200).collect::<String>(), d.created_by))
                .collect::<Vec<_>>()
                .join("\n");

            let insights_summary = insights.iter()
                .map(|i| format!("- {} (by {})", i.content.chars().take(200).collect::<String>(), i.created_by))
                .collect::<Vec<_>>()
                .join("\n");

            // Analyze and provide feedback
            let analysis_prompt = format!(
                "You are monitoring a multi-agent research session.\n\n\
                ORIGINAL QUERY: {}\n\n\
                RESEARCH PLAN:\n{}\n\n\
                CURRENT DISCOVERIES ({}):\n{}\n\n\
                CURRENT INSIGHTS ({}):\n{}\n\n\
                Your task:\n\
                1. Are agents staying focused on the query and plan?\n\
                2. Are there discoveries/insights that are off-topic or misleading?\n\
                3. What additional context or guidance should be provided?\n\n\
                Provide brief analysis (2-3 sentences) and any recommended guidance to store in feedback memory.",
                query,
                plan_content,
                discoveries.len(),
                if discoveries_summary.is_empty() { "(none yet)" } else { &discoveries_summary },
                insights.len(),
                if insights_summary.is_empty() { "(none yet)" } else { &insights_summary }
            );

            // Query supervisor LLM
            let base_url = std::env::var("OLLAMA_HOST")
                .unwrap_or_else(|_| ollama_config.host.clone());

            let mut supervisor_client = OllamaClient::with_config(base_url, research_model.clone());
            supervisor_client.set_max_tool_turns(max_tool_turns);

            match supervisor_client.query_streaming(&analysis_prompt, |_| {}).await {
                Ok(analysis) => {
                    eprintln!("[Supervisor] Analysis: {}", analysis.chars().take(150).collect::<String>());

                    // Store feedback in memory
                    let mut metadata = std::collections::HashMap::new();
                    metadata.insert("iteration".to_string(), iteration.to_string());
                    // Add query_id for session tracking
                    if let Some(ref qid) = query_id {
                        metadata.insert("query_id".to_string(), qid.clone());
                    }

                    // Update existing feedback rather than creating new row
                    // This keeps only the latest supervisor feedback, reducing memory noise
                    if let Err(e) = shared_memory.update_or_store_memory(
                        crate::shared_memory::MemoryType::Feedback,
                        analysis.clone(),
                        "supervisor".to_string(),
                        Some(metadata)
                    ).await {
                        eprintln!("[Supervisor] Failed to update feedback: {}", e);
                    }

                    // GAP DETECTION: When halfway through workers complete, check for gaps and spawn workers once
                    if !gap_workers_requested && completed_count >= midpoint_threshold && completed_count > 0 && initial_worker_count < max_worker_count {
                        eprintln!("[Supervisor] Midpoint reached ({}/{} workers completed), checking for research gaps...", completed_count, initial_worker_count);
                        gap_workers_requested = true;

                        let gap_detection_prompt = format!(
                            "You are supervising a multi-agent research session.\n\n\
                            ORIGINAL QUERY: {}\n\n\
                            RESEARCH PLAN:\n{}\n\n\
                            CURRENT DISCOVERIES ({}):\n{}\n\n\
                            CURRENT INSIGHTS ({}):\n{}\n\n\
                            WORKERS DEPLOYED: {} out of max {}\n\n\
                            Your task: Identify CRITICAL GAPS in the research coverage.\n\
                            - What key aspects of the query are NOT being researched?\n\
                            - What important angles/perspectives are missing?\n\
                            - What questions should have been asked but weren't?\n\n\
                            If you identify gaps, create 1-3 focused sub-questions to fill them.\n\
                            These will spawn additional workers (you can add up to {} more workers).\n\n\
                            Respond with EITHER:\n\
                            1. 'NO_GAPS' if research coverage is adequate\n\
                            2. A JSON array of gap-filling questions:\n\
                            [{{\"question\": \"...\", \"worker\": \"web_researcher|technical_analyst|data_specialist|comparative_analyst|news_researcher\"}}]\n\n\
                            Available workers:\n\
                            - web_researcher: General web research, organizational info, established facts\n\
                            - technical_analyst: Technical specifications, APIs, implementation details\n\
                            - data_specialist: Quantitative metrics, statistics, numerical data\n\
                            - comparative_analyst: Side-by-side comparisons, trade-off analysis\n\
                            - news_researcher: Recent developments, breaking news, updates\n\n\
                            CRITICAL: Return ONLY 'NO_GAPS' or valid JSON array. No markdown, no explanation.",
                            query,
                            plan_content,
                            discoveries.len(),
                            if discoveries_summary.is_empty() { "(none yet)" } else { &discoveries_summary },
                            insights.len(),
                            if insights_summary.is_empty() { "(none yet)" } else { &insights_summary },
                            initial_worker_count,
                            max_worker_count,
                            (max_worker_count - initial_worker_count).min(3)
                        );

                        match supervisor_client.query_streaming(&gap_detection_prompt, |_| {}).await {
                            Ok(gap_response) => {
                                let trimmed = gap_response.trim();
                                if trimmed != "NO_GAPS" && !trimmed.is_empty() {
                                    // Try to parse as JSON array
                                    if let Ok(cleaned) = Self::extract_json_array_static(&gap_response) {
                                        #[derive(serde::Deserialize)]
                                        struct GapQuestion {
                                            question: String,
                                            worker: String,
                                        }

                                        if let Ok(gap_assignments) = serde_json::from_str::<Vec<GapQuestion>>(&cleaned) {
                                            let gap_questions: Vec<SubQuestion> = gap_assignments.into_iter()
                                                .take(3) // Limit to 3 gap-filling workers max
                                                .take((max_worker_count - initial_worker_count).min(3))
                                                .map(|g| SubQuestion {
                                                    question: g.question,
                                                    assigned_worker: g.worker,
                                                })
                                                .collect();

                                            if !gap_questions.is_empty() {
                                                eprintln!("[Supervisor] Detected research gaps, spawning {} additional workers", gap_questions.len());
                                                let _ = gap_worker_tx.send(gap_questions).await;
                                            }
                                        }
                                    }
                                } else {
                                    eprintln!("[Supervisor] No significant research gaps detected");
                                }
                            },
                            Err(e) => {
                                eprintln!("[Supervisor] Error in gap detection: {}", e);
                            }
                        }
                    }
                },
                Err(e) => {
                    eprintln!("[Supervisor] Error analyzing memories: {}", e);
                }
            }
        }
    }

    /// Execute worker agents in parallel using mpsc channels
    #[allow(dead_code)]
    async fn execute_workers(&self, sub_questions: &[SubQuestion]) -> Result<Vec<WorkerResult>> {
        let (tx, mut rx) = mpsc::channel(sub_questions.len());
        let mut handles = Vec::new();

        for sub_q in sub_questions {
            let tx = tx.clone();
            let sub_q = sub_q.clone();
            let worker = self.config.agents.workers
                .iter()
                .find(|w| w.name == sub_q.assigned_worker)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Worker not found: {}", sub_q.assigned_worker))?;

            let base_client = self.base_client.clone();
            let tool_executor = self.tool_executor.clone();
            let progress_tx = self.progress_tx.clone();
            let research_model = self.research_model.clone();
            let max_tool_turns = self.max_tool_turns;
            let shared_memory = self.shared_memory.clone();

            // Emit start event for this worker
            if let Some(p) = &progress_tx {
                let _ = p.send(ResearchProgress::WorkerStarted { worker: worker.name.clone(), question: sub_q.question.clone() });
            }

            let api_delay_ms = self.ollama_config.api_delay_ms;
            let summarization_threshold_research = self.ollama_config.summarization_threshold_research;
            let handle = tokio::spawn(async move {
                let result = Self::execute_worker(
                    worker,
                    &sub_q.question,
                    base_client,
                    tool_executor,
                    research_model,
                    max_tool_turns,
                    progress_tx.clone(),
                    shared_memory,
                    api_delay_ms,
                    summarization_threshold_research,
                ).await;

                let worker_result = match result {
                    Ok(answer) => WorkerResult {
                        question: sub_q.question.clone(),
                        answer,
                        worker_name: sub_q.assigned_worker.clone(),
                    },
                    Err(e) => WorkerResult {
                        question: sub_q.question.clone(),
                        answer: format!("Error: {}", e),
                        worker_name: sub_q.assigned_worker.clone(),
                    },
                };

                // Send progress update
                if let Some(progress) = progress_tx {
                    let _ = progress.send(ResearchProgress::WorkerCompleted(worker_result.worker_name.clone()));
                }

                let _ = tx.send(worker_result).await;
            });

            handles.push(handle);
        }

        // Drop the original sender so rx knows when all workers are done
        drop(tx);

        // Collect results
        let mut results = Vec::new();
        while let Some(result) = rx.recv().await {
            results.push(result);
        }

        // Wait for all handles to complete
        for handle in handles {
            let _ = handle.await;
        }

        Ok(results)
    }

    /// Build pre-task memory context for agent
    /// Follows LangGraph pattern: load relevant memories BEFORE agent starts reasoning
    #[allow(dead_code)]
    async fn build_memory_context(
        tool_executor: &Option<Arc<Mutex<ToolExecutor>>>,
        question: &str,
    ) -> Result<String> {
        if tool_executor.is_none() {
            return Ok(String::new());
        }

        let executor = tool_executor.as_ref().unwrap();
        let executor_lock = executor.lock().await;

        let mut context_parts = Vec::new();
        let empty_params = std::collections::HashMap::new();

        // 1. Get research plan (always relevant)
        if let Ok(plan_result) = executor_lock.execute_builtin_tool("memory_get_plan", empty_params.clone()).await {
            if let Some(plan_text) = plan_result.get("plan").and_then(|v| v.as_str()) {
                if plan_text != "No plan found" && !plan_text.is_empty() {
                    context_parts.push(format!("📋 RESEARCH PLAN:\n{}", plan_text));
                }
            }
        }

        // 2. Get latest supervisor feedback
        if let Ok(feedback_result) = executor_lock.execute_builtin_tool("memory_get_feedback", empty_params.clone()).await {
            if let Some(feedback_arr) = feedback_result.get("feedback").and_then(|v| v.as_array()) {
                if !feedback_arr.is_empty() {
                    let feedback_items: Vec<String> = feedback_arr.iter()
                        .filter_map(|f| {
                            let content = f.get("content")?.as_str()?;
                            let iteration = f.get("metadata")
                                .and_then(|m| m.get("iteration"))
                                .and_then(|i| i.as_str())
                                .unwrap_or("?");
                            Some(format!("  • [Iteration {}]: {}", iteration, content))
                        })
                        .collect();
                    if !feedback_items.is_empty() {
                        context_parts.push(format!("👁️ SUPERVISOR GUIDANCE:\n{}", feedback_items.join("\n")));
                    }
                }
            }
        }

        // 3. Semantic search for relevant discoveries (top 5)
        let mut search_params = std::collections::HashMap::new();
        search_params.insert("query".to_string(), question.to_string());
        search_params.insert("type".to_string(), "discovery".to_string());
        search_params.insert("limit".to_string(), "5".to_string());

        if let Ok(search_result) = executor_lock.execute_builtin_tool("memory_search", search_params).await {
            if let Some(results_arr) = search_result.get("results").and_then(|v| v.as_array()) {
                if !results_arr.is_empty() {
                    let discovery_items: Vec<String> = results_arr.iter()
                        .filter_map(|r| {
                            let content = r.get("content")?.as_str()?;
                            let created_by = r.get("created_by")?.as_str()?;
                            Some(format!("  • [{}]: {}", created_by, content))
                        })
                        .collect();
                    if !discovery_items.is_empty() {
                        context_parts.push(format!("🔍 RELEVANT FINDINGS FROM OTHER AGENTS ({} discoveries):\n{}",
                            discovery_items.len(), discovery_items.join("\n")));
                    }
                }
            }
        }

        // 4. Get deadends to avoid (limit to recent 3)
        if let Ok(deadend_result) = executor_lock.execute_builtin_tool("memory_get_deadends", empty_params).await {
            if let Some(deadends_arr) = deadend_result.get("deadends").and_then(|v| v.as_array()) {
                if !deadends_arr.is_empty() {
                    let deadend_items: Vec<String> = deadends_arr.iter()
                        .take(3)  // Limit to 3 most recent
                        .filter_map(|d| {
                            let content = d.get("content")?.as_str()?;
                            let created_by = d.get("created_by")?.as_str()?;
                            Some(format!("  • [{}]: {}", created_by, content))
                        })
                        .collect();
                    if !deadend_items.is_empty() {
                        context_parts.push(format!("⚠️ APPROACHES TO AVOID ({} deadends):\n{}",
                            deadend_items.len(), deadend_items.join("\n")));
                    }
                }
            }
        }

        drop(executor_lock);  // Release lock before formatting

        if context_parts.is_empty() {
            Ok(String::new())
        } else {
            Ok(format!(
                "╔══════════════════════════════════════════════════════════════╗\n\
                 ║  RELEVANT RESEARCH CONTEXT (from shared memory)              ║\n\
                 ╚══════════════════════════════════════════════════════════════╝\n\n\
                 {}\n\n\
                 ═══════════════════════════════════════════════════════════════\n",
                context_parts.join("\n\n")
            ))
        }
    }

    /// Execute a single worker agent
    async fn execute_worker(
        worker: AgentRole,
        question: &str,
        _base_client: Arc<Mutex<OllamaClient>>,
        tool_executor: Option<Arc<Mutex<ToolExecutor>>>,
        research_model: String,
        max_tool_turns: usize,
        progress_tx: Option<mpsc::UnboundedSender<ResearchProgress>>,
        shared_memory: Option<Arc<crate::shared_memory::SharedMemory>>,
        api_delay_ms: u64,
        summarization_threshold_research: usize,
    ) -> Result<String> {
        // Add small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(api_delay_ms)).await;

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut worker_client = OllamaClient::with_config(base_url, research_model.clone());
        worker_client.set_max_tool_turns(max_tool_turns);

        // Configure research mode summarization with threshold from config
        worker_client.set_summarization_config(None, summarization_threshold_research, true);

        // Set tool executor and available tools if available
        if let Some(ref executor) = tool_executor {
            worker_client.set_tool_executor(executor.clone());

            // Set agent name for tool call tracking
            executor.lock().await.set_agent_name(worker.name.clone());
        }
        let available = worker.available_tools.clone();
        worker_client.set_available_tools(available.clone());

        // Emit a brief status about planned tool usage
        if let Some(p) = &progress_tx {
            if !available.is_empty() {
                let _ = p.send(ResearchProgress::WorkerStatus { worker: worker.name.clone(), status: format!("Preparing tools: {}", available.join(", ")) });
            } else {
                let _ = p.send(ResearchProgress::WorkerStatus { worker: worker.name.clone(), status: "No external tools configured; using model only".to_string() });
            }
        }

        // Build pre-task memory context (LangGraph pattern: load memories BEFORE agent starts)
        if let Some(p) = &progress_tx {
            let _ = p.send(ResearchProgress::WorkerStatus {
                worker: worker.name.clone(),
                status: "Loading relevant context from shared memory...".to_string()
            });
        }

        // Create per-worker dynamic context with access to shared memory
        let dynamic_context = Arc::new(Mutex::new(crate::dynamic_context::DynamicContext::new(
            question.to_string(),
            worker.system_prompt.clone(),
            shared_memory,
        )));

        // Execute worker with dynamic context that updates each iteration
        let answer = Self::execute_worker_with_dynamic_context(
            &worker,
            question,
            &mut worker_client,
            dynamic_context,
            progress_tx.clone(),
        ).await?;

        // Emit a status after response
        if let Some(p) = &progress_tx {
            let _ = p.send(ResearchProgress::WorkerStatus { worker: worker.name.clone(), status: "Processing and structuring results...".to_string() });
        }
        Ok(answer)
    }

    /// Execute worker with dynamic context that syncs with shared memory before starting
    async fn execute_worker_with_dynamic_context(
        worker: &AgentRole,
        question: &str,
        worker_client: &mut OllamaClient,
        dynamic_context: Arc<Mutex<crate::dynamic_context::DynamicContext>>,
        progress_tx: Option<mpsc::UnboundedSender<ResearchProgress>>,
    ) -> Result<String> {
        // Build context with latest plan, feedback, and relevant findings from shared memory
        let context_section = {
            let mut ctx = dynamic_context.lock().await;
            ctx.build_prompt_context().await.unwrap_or_default()
        };

        // Strong reminder to store discoveries in memory
        let memory_workflow = "\n\n>>> MANDATORY WORKFLOW - FOLLOW EXACTLY <<<\n\n\
            1. Call research tool\n\
            2. IMMEDIATELY: memory_store(type=\"discovery\", content=\"Fact [Source: Name](URL)\", agent=\"your_role\")\n\
            3. Call another research tool\n\
            4. IMMEDIATELY: memory_store(type=\"discovery\", content=\"Another fact [Source](URL)\", agent=\"your_role\")\n\
            5. Repeat steps 1-4 until you have 3-5 discoveries\n\
            6. Write final comprehensive answer\n\n\
            CRITICAL: Store discoveries IMMEDIATELY after each tool call!\n\
            Other agents cannot see your findings unless stored in memory.\n\n";

        let prompt = format!(
            "{}{}\n\
            CITATION REQUIREMENT: When citing sources, ALWAYS prefer full URLs when available. Use format [Source: https://full-url.com] instead of just site names. This enables independent verification.\n\n\
            {}\n\n\
            Question: {}",
            context_section,
            memory_workflow,
            worker.system_prompt,
            question
        );

        // Status update
        if let Some(ref p) = progress_tx {
            let _ = p.send(ResearchProgress::WorkerStatus {
                worker: worker.name.clone(),
                status: "Executing with latest plan and feedback...".to_string()
            });
        }

        // query_streaming handles tool iterations internally with its own context
        let answer = worker_client.query_streaming(&prompt, |_| {}).await?;

        Ok(answer)
    }

    /// Summarize a long worker result to reduce token count
    async fn summarize_worker_result(&self, result: &WorkerResult, _num_workers: usize) -> Result<String> {
        // Use the research-specific summarization threshold (default: 50K chars)
        // This is much higher than regular chat to preserve detailed research findings
        let max_chars = self.ollama_config.summarization_threshold_research;

        eprintln!("[Research] Summarization threshold: {} chars, Worker result: {} chars",
                  max_chars, result.answer.len());

        // If result is within limit, return as-is
        if result.answer.len() <= max_chars {
            eprintln!("[Research] Worker result within threshold, keeping full content");
            return Ok(result.answer.clone());
        }

        eprintln!("[Research] Worker result exceeds threshold ({} > {} chars), summarizing...",
                  result.answer.len(), max_chars);

        // Add delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(self.ollama_config.api_delay_ms)).await;

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut summarizer_client = OllamaClient::with_config(base_url, self.research_model.clone());
        summarizer_client.set_max_tool_turns(self.max_tool_turns);

        let prompt = format!(
            "Condense these research findings while preserving all key information:\n\n\
            - Keep ALL facts, data points, and citations [Source: name]\n\
            - Preserve technical details and specifications\n\
            - Maintain examples and context\n\
            - Remove only redundant explanations and filler words\n\
            - Keep the depth and completeness of information\n\n\
            Research findings:\n{}",
            result.answer
        );

        match summarizer_client.query_streaming(&prompt, |_| {}).await {
            Ok(summary) => {
                eprintln!("[Research] Summarized from {} to {} characters", result.answer.len(), summary.len());
                Ok(summary)
            },
            Err(e) => {
                eprintln!("[Research] Summarization failed: {}, using truncated version", e);
                // Fallback to truncation if summarization fails
                let truncate_len = max_chars.min(result.answer.len());
                Ok(format!("{}...\n\n[Note: Content truncated due to length]", &result.answer[..truncate_len]))
            }
        }
    }

    /// Combine worker results into a cohesive output
    async fn combine_results(&self, original_query: &str, results: &[WorkerResult]) -> Result<String> {
        // Emit a status about combination stage for verbosity
        self.send_progress(ResearchProgress::WorkerStatus {
            worker: "Combiner".to_string(),
            status: format!("Combining {} worker results", results.len()),
        });
        let mut output = format!("# Research Results for: {}\n\n", original_query);
        let num_workers = results.len();

        for (idx, result) in results.iter().enumerate() {
            // Show progress for summarization if needed
            if result.answer.len() > self.ollama_config.summarization_threshold_research {
                self.send_progress(ResearchProgress::Summarizing);
            }

            // Summarize if needed based on available context per worker
            let answer = self.summarize_worker_result(result, num_workers).await?;

            output.push_str(&format!(
                "## {}\n**Question:** {}\n\n{}\n\n",
                result.worker_name,
                result.question,
                answer
            ));
        }

        Ok(output)
    }

    /// Extract sources from text and add sources section
    fn add_sources_section(&self, text: &str) -> String {
        let sources = self.extract_sources(text);

        if sources.is_empty() {
            eprintln!("[Research] No sources found in document");
            return text.to_string();
        }

        eprintln!("[Research] Found {} unique sources", sources.len());

        // Create sources section
        let mut output = text.to_string();

        // Ensure there's separation from main content
        if !output.ends_with("\n\n") {
            output.push_str("\n\n");
        }

        output.push_str("---\n\n");
        output.push_str("## References\n\n");

        // Separate URLs from other sources for better organization
        let mut urls = Vec::new();
        let mut other_sources = Vec::new();

        for source in sources.iter() {
            if source.starts_with("http://") || source.starts_with("https://") {
                urls.push(source.as_str());
            } else {
                other_sources.push(source.as_str());
            }
        }

        // List URLs first (primary sources for verification)
        if !urls.is_empty() {
            output.push_str("### Web Sources\n\n");
            output.push_str("The following websites and online resources were consulted:\n\n");
            for (i, url) in urls.iter().enumerate() {
                // Format as clickable markdown links
                output.push_str(&format!("{}. <{}>\n", i + 1, url));
            }
        }

        // Then list other sources (documents, books, APIs, etc.)
        if !other_sources.is_empty() {
            if !urls.is_empty() {
                output.push_str("\n");
            }
            output.push_str("### Additional Sources\n\n");
            output.push_str("Other sources referenced:\n\n");
            let start_num = urls.len() + 1;
            for (i, source) in other_sources.iter().enumerate() {
                output.push_str(&format!("{}. {}\n", start_num + i, source));
            }
        }

        output
    }

    /// Extract unique sources from text with various citation formats
    fn extract_sources(&self, text: &str) -> BTreeSet<String> {
        let mut sources = BTreeSet::new();

        // Pattern 1: [Source: url] or [Source: name] or [Source: name, date]
        if let Ok(re) = regex::Regex::new(r"\[Source:\s*([^\]]+)\]") {
            for cap in re.captures_iter(text) {
                if let Some(source) = cap.get(1) {
                    let cleaned = source.as_str().trim().to_string();
                    if !cleaned.is_empty() {
                        sources.insert(cleaned);
                    }
                }
            }
        }

        // Pattern 2: (Source: url) or (Source: name)
        if let Ok(re) = regex::Regex::new(r"\(Source:\s*([^\)]+)\)") {
            for cap in re.captures_iter(text) {
                if let Some(source) = cap.get(1) {
                    let cleaned = source.as_str().trim().to_string();
                    if !cleaned.is_empty() {
                        sources.insert(cleaned);
                    }
                }
            }
        }

        // Pattern 3: Standalone URLs (http/https)
        if let Ok(re) = regex::Regex::new(r"https?://[^\s\)\]]+") {
            for cap in re.captures_iter(text) {
                let url = cap.get(0).unwrap().as_str();
                // Only include if not already captured in a [Source: ] tag
                if !text.contains(&format!("[Source: {}]", url)) &&
                   !text.contains(&format!("(Source: {})", url)) {
                    sources.insert(url.trim_end_matches(|c| c == '.' || c == ',' || c == ';').to_string());
                }
            }
        }

        sources
    }

    /// Legacy method - kept for compatibility
    /// Refinement loop with multi-agent debate
    async fn refinement_loop(&self, initial_output: &str) -> Result<String> {
        let mut current_output = initial_output.to_string();
        let max_iterations = self.ollama_config.max_refinement_iterations;

        for iteration in 0..max_iterations {
            // Multi-agent debate
            self.send_progress(ResearchProgress::CriticReviewing);
            self.send_progress(ResearchProgress::WorkerStatus {
                worker: "Debate".to_string(),
                status: "Launching debate between Advocate, Skeptic, and Synthesizer".to_string(),
            });
            let debate_result = self.conduct_debate(&current_output).await?;

            // Check if approved
            if debate_result.trim().to_uppercase().contains("APPROVED") {
                eprintln!("[Research] Output approved by debate after {} iteration(s)", iteration + 1);
                break;
            }

            // Refine based on debate conclusions
            eprintln!("[Research] Iteration {}: Refining based on debate", iteration + 1);
            self.send_progress(ResearchProgress::Refining(iteration + 1, max_iterations));
            self.send_progress(ResearchProgress::WorkerStatus {
                worker: "Refiner".to_string(),
                status: format!("Applying debate conclusions (iteration {}/{})", iteration + 1, max_iterations),
            });
            current_output = self.refine_output(&current_output, &debate_result).await?;

            // If this was the last iteration, use the refined output anyway
            if iteration == max_iterations - 1 {
                eprintln!("[Research] Max iterations reached. Using last refined output.");
            }
        }

        Ok(current_output)
    }

    /// Conduct multi-agent debate to evaluate research output
    async fn conduct_debate(&self, output: &str) -> Result<String> {
        eprintln!("[Research] Starting multi-agent debate...");
        self.send_progress(ResearchProgress::WorkerStatus {
            worker: "Debate".to_string(),
            status: "Starting debate session".to_string(),
        });

        // Get advocate, skeptic, and synthesizer from debate_agents
        let advocate = self.config.agents.debate_agents.iter()
            .find(|a| a.role == "advocate")
            .ok_or_else(|| anyhow::anyhow!("Advocate agent not found"))?;

        let skeptic = self.config.agents.debate_agents.iter()
            .find(|a| a.role == "skeptic")
            .ok_or_else(|| anyhow::anyhow!("Skeptic agent not found"))?;

        let synthesizer = self.config.agents.debate_agents.iter()
            .find(|a| a.role == "synthesizer")
            .ok_or_else(|| anyhow::anyhow!("Synthesizer agent not found"))?;

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let max_rounds = self.ollama_config.max_debate_rounds;
        let mut debate_history = String::new();
        let mut last_advocate_arg;
        let mut last_skeptic_arg = String::new();

        // Conduct multiple rounds of debate
        for round in 1..=max_rounds {
            self.send_progress(ResearchProgress::DebateRound(round, max_rounds));
            eprintln!("[Research] Debate round {}/{}", round, max_rounds);
            self.send_progress(ResearchProgress::WorkerStatus {
                worker: "Debate".to_string(),
                status: format!("Advocate presenting arguments (round {}/{})", round, max_rounds),
            });

            tokio::time::sleep(tokio::time::Duration::from_millis(self.ollama_config.api_delay_ms)).await;

            // Advocate's turn
            let advocate_prompt = if round == 1 {
                // First round: defend the research
                format!(
                    "{}\n\nResearch Output to Defend:\n{}",
                    advocate.system_prompt,
                    output
                )
            } else {
                // Subsequent rounds: respond to skeptic's critique
                format!(
                    "{}\n\nResearch Output:\n{}\n\nDebate History:\n{}\n\nSkeptic's Last Critique:\n{}\n\nProvide your rebuttal:",
                    advocate.system_prompt,
                    output,
                    debate_history,
                    last_skeptic_arg
                )
            };

            let mut advocate_client = OllamaClient::with_config(base_url.clone(), self.research_model.clone());
            advocate_client.set_max_tool_turns(self.max_tool_turns);
            if let Some(executor) = &self.tool_executor {
                advocate_client.set_tool_executor(executor.clone());
            }
            advocate_client.set_available_tools(advocate.available_tools.clone());

            last_advocate_arg = advocate_client.query_streaming(&advocate_prompt, |_| {}).await?;
            eprintln!("[Research] Advocate round {}: presented argument", round);
            // Log a shortened advocate argument for UI verbosity
            crate::progress::log_with(
                crate::progress::Kind::Debate,
                format!(
                    "Advocate (round {}/{}): {}",
                    round,
                    max_rounds,
                    Self::summarize_arg(&last_advocate_arg, 140)
                ),
            );

            // Add to history
            debate_history.push_str(&format!("\n--- Round {} ---\n", round));
            debate_history.push_str(&format!("**Advocate:**\n{}\n\n", last_advocate_arg));

            tokio::time::sleep(tokio::time::Duration::from_millis(self.ollama_config.api_delay_ms)).await;

            // Skeptic's turn
            self.send_progress(ResearchProgress::WorkerStatus {
                worker: "Debate".to_string(),
                status: format!("Skeptic challenging (round {}/{})", round, max_rounds),
            });
            let skeptic_prompt = if round == 1 {
                // First round: critique the research and advocate's defense
                format!(
                    "{}\n\nResearch Output:\n{}\n\nAdvocate's Defense:\n{}\n\nPresent your critique:",
                    skeptic.system_prompt,
                    output,
                    last_advocate_arg
                )
            } else {
                // Subsequent rounds: respond to advocate's rebuttal
                format!(
                    "{}\n\nResearch Output:\n{}\n\nDebate History:\n{}\n\nAdvocate's Last Rebuttal:\n{}\n\nProvide your response:",
                    skeptic.system_prompt,
                    output,
                    debate_history,
                    last_advocate_arg
                )
            };

            let mut skeptic_client = OllamaClient::with_config(base_url.clone(), self.research_model.clone());
            skeptic_client.set_max_tool_turns(self.max_tool_turns);
            if let Some(executor) = &self.tool_executor {
                skeptic_client.set_tool_executor(executor.clone());
            }
            skeptic_client.set_available_tools(skeptic.available_tools.clone());

            last_skeptic_arg = skeptic_client.query_streaming(&skeptic_prompt, |_| {}).await?;
            eprintln!("[Research] Skeptic round {}: presented critique", round);
            // Log a shortened skeptic rebuttal
            crate::progress::log_with(
                crate::progress::Kind::Debate,
                format!(
                    "Skeptic (round {}/{}): {}",
                    round,
                    max_rounds,
                    Self::summarize_arg(&last_skeptic_arg, 140)
                ),
            );

            // Add to history
            debate_history.push_str(&format!("**Skeptic:**\n{}\n\n", last_skeptic_arg));
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(self.ollama_config.api_delay_ms)).await;

        // Synthesizer makes final decision after all rounds
        let synthesizer_prompt = format!(
            "{}\n\nResearch Output:\n{}\n\nComplete Debate Transcript:\n{}\n\nProvide your balanced assessment:",
            synthesizer.system_prompt,
            output,
            debate_history
        );

        let mut synthesizer_client = OllamaClient::with_config(base_url, self.research_model.clone());
        synthesizer_client.set_max_tool_turns(self.max_tool_turns);
        if let Some(executor) = &self.tool_executor {
            synthesizer_client.set_tool_executor(executor.clone());
        }
        synthesizer_client.set_available_tools(synthesizer.available_tools.clone());

        let final_decision = synthesizer_client.query_streaming(&synthesizer_prompt, |_| {}).await?;
        eprintln!("[Research] Synthesizer reached decision after {} debate rounds", max_rounds);
        // Log a shortened synthesizer decision
        crate::progress::log_with(
            crate::progress::Kind::Debate,
            format!(
                "Synthesizer decision: {}",
                Self::summarize_arg(&final_decision, 140)
            ),
        );
        self.send_progress(ResearchProgress::WorkerStatus {
            worker: "Debate".to_string(),
            status: "Synthesizer compiling final decision".to_string(),
        });
        
        Ok(final_decision)
    }

    /// Document writing loop with document critic
    async fn document_writing_loop(&self, original_query: &str, research_content: &str) -> Result<String> {
        let mut current_document = String::new();
        let max_iterations = self.ollama_config.max_document_iterations;

        for iteration in 0..max_iterations {
            // Write or rewrite the document
            self.send_progress(ResearchProgress::WritingDocument(iteration + 1, max_iterations));
            self.send_progress(ResearchProgress::WorkerStatus {
                worker: "Writer".to_string(),
                status: format!("Drafting document (iteration {}/{})", iteration + 1, max_iterations),
            });

            let document = if iteration == 0 {
                // First iteration: create initial document from research
                self.write_document(original_query, research_content, None).await?
            } else {
                // Subsequent iterations: rewrite based on criticism
                self.write_document(original_query, research_content, Some(&current_document)).await?
            };

            current_document = document;

            // Get document critic feedback
            self.send_progress(ResearchProgress::DocumentReviewing);
            self.send_progress(ResearchProgress::WorkerStatus {
                worker: "DocumentCritic".to_string(),
                status: "Reviewing draft for clarity, correctness, and structure".to_string(),
            });
            let criticism = self.review_document(original_query, &current_document).await?;

            // Check if approved
            if criticism.trim().to_uppercase() == "APPROVED" {
                eprintln!("[Research] Document approved after {} iteration(s)", iteration + 1);
                break;
            }

            // If not approved and not last iteration, we'll rewrite in next iteration
            eprintln!("[Research] Document iteration {}: Feedback received, will revise", iteration + 1);

            // On last iteration, use what we have
            if iteration == max_iterations - 1 {
                eprintln!("[Research] Max document iterations reached. Using current version.");
            }
        }

        // Add sources section to the document
        let final_document = self.add_sources_section(&current_document);
        self.send_progress(ResearchProgress::WorkerStatus {
            worker: "Writer".to_string(),
            status: "Finalizing document and references".to_string(),
        });
        
        Ok(final_document)
    }

    /// Write or rewrite a document from research findings
    async fn write_document(&self, original_query: &str, research_content: &str, previous_document: Option<&str>) -> Result<String> {
        // Add delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(self.ollama_config.api_delay_ms)).await;

        let prompt = if let Some(prev_doc) = previous_document {
            format!(
                "{}\n\nOriginal Query: {}\n\n\
                Research Findings:\n{}\n\n\
                Previous Document Draft:\n{}\n\n\
                Revise the previous document to address any shortcomings while maintaining its strengths.",
                self.config.agents.writer.system_prompt,
                original_query,
                research_content,
                prev_doc
            )
        } else {
            format!(
                "{}\n\nOriginal Query: {}\n\n\
                Research Findings:\n{}\n\n\
                Create a comprehensive, professional document that fully answers the query.",
                self.config.agents.writer.system_prompt,
                original_query,
                research_content
            )
        };

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut writer_client = OllamaClient::with_config(base_url, self.research_model.clone());
        writer_client.set_max_tool_turns(self.max_tool_turns);
        let document = writer_client.query_streaming(&prompt, |_| {}).await?;

        Ok(document)
    }

    /// Review document with document critic
    async fn review_document(&self, original_query: &str, document: &str) -> Result<String> {
        // Add delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(self.ollama_config.api_delay_ms)).await;

        let prompt = format!(
            "{}\n\nOriginal Query: {}\n\n\
            Document to Review:\n{}",
            self.config.agents.document_critic.system_prompt,
            original_query,
            document
        );

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut critic_client = OllamaClient::with_config(base_url, self.research_model.clone());
        critic_client.set_max_tool_turns(self.max_tool_turns);
        let review = critic_client.query_streaming(&prompt, |_| {}).await?;

        Ok(review)
    }

    /// Refine output based on debate conclusions
    async fn refine_output(&self, output: &str, debate_result: &str) -> Result<String> {
        // Add delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(self.ollama_config.api_delay_ms)).await;

        let prompt = format!(
            "CITATION REQUIREMENT: When adding new sources, ALWAYS use full URLs when available. Format: [Source: https://full-url.com]. This enables independent verification.\n\n{}\n\nOriginal output:\n{}\n\nDebate Conclusions:\n{}\n\nProvide the improved output:",
            self.config.agents.refiner.system_prompt,
            output,
            debate_result
        );

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut refiner_client = OllamaClient::with_config(base_url, self.research_model.clone());
        refiner_client.set_max_tool_turns(self.max_tool_turns);

        // Refiner can use tools
        if let Some(executor) = &self.tool_executor {
            refiner_client.set_tool_executor(executor.clone());
        }
        refiner_client.set_available_tools(self.config.agents.refiner.available_tools.clone());

        let refined = refiner_client.query_streaming(&prompt, |_| {}).await?;
        Ok(refined)
    }

    /// Extract JSON array from response text
    fn extract_json_array(&self, text: &str) -> Result<String> {
        Self::extract_json_array_static(text)
    }

    fn extract_json_array_static(text: &str) -> Result<String> {
        // Try to find JSON array in the response
        if let Some(start) = text.find('[') {
            if let Some(end) = text.rfind(']') {
                if end > start {
                    return Ok(text[start..=end].to_string());
                }
            }
        }

        // If no array found, try to parse the whole text
        if text.trim().starts_with('[') {
            return Ok(text.trim().to_string());
        }

        Err(anyhow::anyhow!("No JSON array found in response"))
    }
}
