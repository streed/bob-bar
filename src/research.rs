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
    WorkersStarted(usize), // number of workers
    WorkerCompleted(String), // worker name
    Combining,
    Refining(usize, usize), // current iteration, max iterations
    CriticReviewing,
    DebateRound(usize, usize), // current round, max rounds
    AddingBibliography,
    WritingDocument(usize, usize), // current iteration, max iterations
    DocumentReviewing,
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
    pub max_refinement_iterations: usize,
    pub max_document_iterations: usize,
    pub worker_count: usize,
    pub max_debate_rounds: usize,
}

impl Default for ResearchConfig {
    fn default() -> Self {
        Self {
            max_refinement_iterations: 5,
            max_document_iterations: 3,
            worker_count: 3,
            max_debate_rounds: 2,
        }
    }
}

#[derive(Debug, Clone)]
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
    base_client: Arc<Mutex<OllamaClient>>,
    tool_executor: Option<Arc<Mutex<ToolExecutor>>>,
    progress_tx: Option<mpsc::UnboundedSender<ResearchProgress>>,
    context_window: usize,
    research_model: String,
    max_tool_turns: usize,
}

impl ResearchOrchestrator {
    pub fn new(config: AgentsConfig, base_client: Arc<Mutex<OllamaClient>>, context_window: usize, research_model: String, max_tool_turns: usize) -> Self {
        Self {
            config,
            base_client,
            tool_executor: None,
            progress_tx: None,
            context_window,
            research_model,
            max_tool_turns,
        }
    }

    pub fn from_file(path: &std::path::Path, base_client: Arc<Mutex<OllamaClient>>, context_window: usize, research_model: String, max_tool_turns: usize) -> Result<Self> {
        let config_str = std::fs::read_to_string(path)?;
        let config: AgentsConfig = serde_json::from_str(&config_str)?;
        Ok(Self::new(config, base_client, context_window, research_model, max_tool_turns))
    }

    /// Override config values from global config.toml
    pub fn override_config(&mut self, toml_config: &crate::config::ResearchConfig) {
        // Override with values from config.toml if they differ from defaults
        self.config.config.max_refinement_iterations = toml_config.max_refinement_iterations;
        self.config.config.max_document_iterations = toml_config.max_document_iterations;
        self.config.config.worker_count = toml_config.worker_count;
        self.config.config.max_debate_rounds = toml_config.max_debate_rounds;
    }

    pub fn set_tool_executor(&mut self, executor: Arc<Mutex<ToolExecutor>>) {
        self.tool_executor = Some(executor);
    }

    pub fn set_progress_channel(&mut self, tx: mpsc::UnboundedSender<ResearchProgress>) {
        self.progress_tx = Some(tx);
    }

    fn send_progress(&self, progress: ResearchProgress) {
        if let Some(tx) = &self.progress_tx {
            let _ = tx.send(progress);
        }
    }

    /// Main entry point for research mode
    pub async fn research(&mut self, query: &str) -> Result<String> {
        self.send_progress(ResearchProgress::Started);

        // Step 1: Decompose query into sub-questions
        self.send_progress(ResearchProgress::Decomposing);
        let sub_questions = self.decompose_query(query).await?;

        if sub_questions.is_empty() {
            return Ok("Unable to decompose query into sub-questions.".to_string());
        }

        // Step 2: Execute workers in parallel
        self.send_progress(ResearchProgress::WorkersStarted(sub_questions.len()));
        let worker_results = self.execute_workers(&sub_questions).await?;

        // Step 3: Combine results (with summarization if needed)
        self.send_progress(ResearchProgress::Combining);
        let combined_output = self.combine_results(query, &worker_results).await?;

        // Step 4: Refinement loop with critic
        let refined_output = self.refinement_loop(&combined_output).await?;

        // Step 5: Document writing loop with document critic
        let final_document = self.document_writing_loop(query, &refined_output).await?;

        self.send_progress(ResearchProgress::Completed);
        Ok(final_document)
    }

    /// Decompose query into sub-questions using lead agent
    async fn decompose_query(&self, query: &str) -> Result<Vec<SubQuestion>> {
        let prompt = format!(
            "{}\n\nQuery: {}",
            self.config.agents.lead.system_prompt,
            query
        );

        // Create a temporary client for lead agent (no tools needed)
        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut lead_client = OllamaClient::with_config(base_url, self.research_model.clone());
        lead_client.set_max_tool_turns(self.max_tool_turns);

        let response = lead_client.query_streaming(&prompt, |_| {}).await?;

        // Parse JSON response - expecting array of {question, worker} objects
        let cleaned = self.extract_json_array(&response)?;

        #[derive(Deserialize)]
        struct QuestionAssignment {
            question: String,
            worker: String,
        }

        let assignments: Vec<QuestionAssignment> = serde_json::from_str(&cleaned)?;

        // Log the planner's decisions in debug mode
        if std::env::var("BOBBAR_DEBUG").is_ok() {
            eprintln!("\n[Research Planner] Decomposed query into {} sub-questions:", assignments.len());
            for (i, assignment) in assignments.iter().enumerate() {
                eprintln!("  {}. [{}] {}", i + 1, assignment.worker, assignment.question);
            }
            eprintln!();
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

        Ok(sub_questions)
    }

    /// Execute worker agents in parallel using mpsc channels
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

            let handle = tokio::spawn(async move {
                let result = Self::execute_worker(
                    worker,
                    &sub_q.question,
                    base_client,
                    tool_executor,
                    research_model,
                    max_tool_turns,
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

    /// Execute a single worker agent
    async fn execute_worker(
        worker: AgentRole,
        question: &str,
        _base_client: Arc<Mutex<OllamaClient>>,
        tool_executor: Option<Arc<Mutex<ToolExecutor>>>,
        research_model: String,
        max_tool_turns: usize,
    ) -> Result<String> {
        // Add small delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        let mut worker_client = OllamaClient::with_config(base_url, research_model.clone());
        worker_client.set_max_tool_turns(max_tool_turns);

        // Set tool executor and available tools if available
        if let Some(executor) = tool_executor {
            worker_client.set_tool_executor(executor);
        }
        worker_client.set_available_tools(worker.available_tools.clone());

        let prompt = format!(
            "CITATION REQUIREMENT: When citing sources, ALWAYS prefer full URLs when available. Use format [Source: https://full-url.com] instead of just site names. This enables independent verification.\n\n{}\n\nQuestion: {}",
            worker.system_prompt,
            question
        );

        let answer = worker_client.query_streaming(&prompt, |_| {}).await?;
        Ok(answer)
    }

    /// Summarize a long worker result to reduce token count
    async fn summarize_worker_result(&self, result: &WorkerResult, num_workers: usize) -> Result<String> {
        // Calculate available tokens per worker based on context window
        // Reserve 20% for prompts, system messages, and overhead
        let available_tokens = (self.context_window as f64 * 0.8) as usize;

        // Divide available tokens among all workers
        // Use 4 chars ≈ 1 token as rough estimate
        let max_chars_per_worker = (available_tokens / num_workers) * 4;

        eprintln!("[Research] Context window: {}, Available per worker: ~{} chars ({} workers)",
                  self.context_window, max_chars_per_worker, num_workers);

        // If result is within allocation, return as-is
        if result.answer.len() <= max_chars_per_worker {
            return Ok(result.answer.clone());
        }

        eprintln!("[Research] Worker result too long ({} chars), summarizing to fit ~{} chars...",
                  result.answer.len(), max_chars_per_worker);

        // Add delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

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
                let truncate_len = max_chars_per_worker.min(result.answer.len());
                Ok(format!("{}...\n\n[Note: Content truncated due to length]", &result.answer[..truncate_len]))
            }
        }
    }

    /// Combine worker results into a cohesive output
    async fn combine_results(&self, original_query: &str, results: &[WorkerResult]) -> Result<String> {
        let mut output = format!("# Research Results for: {}\n\n", original_query);
        let num_workers = results.len();

        for result in results {
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
    fn add_bibliography(&self, text: &str) -> String {
        self.add_sources_section(text)
    }

    /// Refinement loop with multi-agent debate
    async fn refinement_loop(&self, initial_output: &str) -> Result<String> {
        let mut current_output = initial_output.to_string();
        let max_iterations = self.config.config.max_refinement_iterations;

        for iteration in 0..max_iterations {
            // Multi-agent debate
            self.send_progress(ResearchProgress::CriticReviewing);
            let debate_result = self.conduct_debate(&current_output).await?;

            // Check if approved
            if debate_result.trim().to_uppercase().contains("APPROVED") {
                eprintln!("[Research] Output approved by debate after {} iteration(s)", iteration + 1);
                break;
            }

            // Refine based on debate conclusions
            eprintln!("[Research] Iteration {}: Refining based on debate", iteration + 1);
            self.send_progress(ResearchProgress::Refining(iteration + 1, max_iterations));
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

        let max_rounds = self.config.config.max_debate_rounds;
        let mut debate_history = String::new();
        let mut last_advocate_arg;
        let mut last_skeptic_arg = String::new();

        // Conduct multiple rounds of debate
        for round in 1..=max_rounds {
            self.send_progress(ResearchProgress::DebateRound(round, max_rounds));
            eprintln!("[Research] Debate round {}/{}", round, max_rounds);

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

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

            // Add to history
            debate_history.push_str(&format!("\n--- Round {} ---\n", round));
            debate_history.push_str(&format!("**Advocate:**\n{}\n\n", last_advocate_arg));

            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Skeptic's turn
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

            // Add to history
            debate_history.push_str(&format!("**Skeptic:**\n{}\n\n", last_skeptic_arg));
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

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

        Ok(final_decision)
    }

    /// Document writing loop with document critic
    async fn document_writing_loop(&self, original_query: &str, research_content: &str) -> Result<String> {
        let mut current_document = String::new();
        let max_iterations = self.config.config.max_document_iterations;

        for iteration in 0..max_iterations {
            // Write or rewrite the document
            self.send_progress(ResearchProgress::WritingDocument(iteration + 1, max_iterations));

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

        Ok(final_document)
    }

    /// Write or rewrite a document from research findings
    async fn write_document(&self, original_query: &str, research_content: &str, previous_document: Option<&str>) -> Result<String> {
        // Add delay to avoid rate limiting
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

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
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

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
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

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
