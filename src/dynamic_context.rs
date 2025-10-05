use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use crate::shared_memory::SharedMemory;

/// Per-agent dynamic context that stores short-term working memory
/// This is different from SharedMemory which is for cross-agent long-term memory
pub struct DynamicContext {
    /// The original query assigned to this agent
    original_query: String,

    /// The agent's system prompt
    #[allow(dead_code)]
    agent_prompt: String,

    /// Current iteration number
    iteration: usize,

    /// Short-term notes and findings (cleared between iterations or tasks)
    working_notes: Vec<WorkingNote>,

    /// Key-value store for arbitrary agent state
    state: HashMap<String, String>,

    /// Reference to shared memory for pulling relevant context each iteration
    shared_memory: Option<Arc<SharedMemory>>,

    /// Last iteration when we pulled from shared memory
    last_memory_sync: usize,
}

#[derive(Clone, Debug)]
pub struct WorkingNote {
    pub content: String,
    pub note_type: NoteType,
    pub iteration: usize,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub enum NoteType {
    Observation,      // Something noticed
    PartialAnswer,    // Incomplete answer/finding
    FollowUp,         // Question to explore
    ToolResult,       // Result from a tool call
    Thought,          // Agent reasoning
}

impl DynamicContext {
    /// Create a new dynamic context for an agent
    pub fn new(query: String, agent_prompt: String, shared_memory: Option<Arc<SharedMemory>>) -> Self {
        Self {
            original_query: query,
            agent_prompt,
            iteration: 0,
            working_notes: Vec::new(),
            state: HashMap::new(),
            shared_memory,
            last_memory_sync: 0,
        }
    }

    /// Increment iteration counter (called at start of each tool turn)
    #[allow(dead_code)]
    pub fn next_iteration(&mut self) {
        self.iteration += 1;
    }

    /// Get current iteration number
    #[allow(dead_code)]
    pub fn current_iteration(&self) -> usize {
        self.iteration
    }

    /// Add a working note (short-term observation/finding)
    #[allow(dead_code)]
    pub fn add_note(&mut self, content: String, note_type: NoteType) {
        self.working_notes.push(WorkingNote {
            content,
            note_type,
            iteration: self.iteration,
        });
    }

    /// Get all working notes
    #[allow(dead_code)]
    pub fn get_notes(&self) -> &[WorkingNote] {
        &self.working_notes
    }

    /// Get notes from current iteration only
    #[allow(dead_code)]
    pub fn get_current_iteration_notes(&self) -> Vec<&WorkingNote> {
        self.working_notes
            .iter()
            .filter(|n| n.iteration == self.iteration)
            .collect()
    }

    /// Clear all working notes (useful when starting fresh or completing task)
    #[allow(dead_code)]
    pub fn clear_notes(&mut self) {
        self.working_notes.clear();
    }

    /// Set arbitrary state value
    #[allow(dead_code)]
    pub fn set_state(&mut self, key: String, value: String) {
        self.state.insert(key, value);
    }

    /// Get state value
    #[allow(dead_code)]
    pub fn get_state(&self, key: &str) -> Option<&String> {
        self.state.get(key)
    }

    /// Sync with shared memory - pull relevant context once per iteration
    pub async fn sync_from_shared_memory(&mut self) -> Result<Option<String>> {
        // Only sync once per iteration
        if self.last_memory_sync >= self.iteration {
            return Ok(None);
        }

        if let Some(ref memory) = self.shared_memory {
            self.last_memory_sync = self.iteration;

            let mut sections = Vec::new();

            // 1. Always get the latest plan from the lead researcher
            let plans = memory.get_by_type(crate::shared_memory::MemoryType::Plan).await;
            if let Some(latest_plan) = plans.last() {
                sections.push(format!(
                    "=== Research Plan (by {}) ===\n{}\n================================",
                    latest_plan.created_by,
                    latest_plan.content
                ));
            }

            // 2. Get all feedback from supervisor/lead (newest first)
            let mut feedback_items = memory.get_by_type(crate::shared_memory::MemoryType::Feedback).await;
            feedback_items.reverse(); // Show newest feedback first
            if !feedback_items.is_empty() {
                let feedback_text = feedback_items
                    .iter()
                    .map(|f| format!("• {} (by {})", f.content, f.created_by))
                    .collect::<Vec<_>>()
                    .join("\n");

                sections.push(format!(
                    "=== Leader Feedback & Adjustments ===\n{}\n=====================================",
                    feedback_text
                ));
            }

            // 3. Get MOST RECENT discoveries and insights from other workers
            // Combine all recent findings (newest first for freshness)
            let mut all_findings = Vec::new();

            let mut discoveries = memory.get_by_type(crate::shared_memory::MemoryType::Discovery).await;
            discoveries.reverse(); // Newest first
            all_findings.extend(discoveries.into_iter().take(3)); // Top 3 recent discoveries

            let mut insights = memory.get_by_type(crate::shared_memory::MemoryType::Insight).await;
            insights.reverse(); // Newest first
            all_findings.extend(insights.into_iter().take(2)); // Top 2 recent insights

            let mut deadends = memory.get_by_type(crate::shared_memory::MemoryType::Deadend).await;
            deadends.reverse(); // Newest first
            all_findings.extend(deadends.into_iter().take(2)); // Top 2 recent deadends

            if !all_findings.is_empty() {
                let findings_text = all_findings
                    .iter()
                    .map(|m| {
                        format!(
                            "• [{}] {} (by {})",
                            m.memory_type.as_str(),
                            m.content,
                            m.created_by
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                sections.push(format!(
                    "=== Recent findings from other agents ===\n{}\n=========================================",
                    findings_text
                ));
            }

            if sections.is_empty() {
                return Ok(None);
            }

            Ok(Some(sections.join("\n\n")))
        } else {
            Ok(None)
        }
    }

    /// Build complete context string for injection into prompts
    pub async fn build_prompt_context(&mut self) -> Result<String> {
        let mut sections = Vec::new();

        // 1. Add global context (date, system info)
        sections.push(Self::get_global_context());

        // 2. Add original query
        sections.push(format!("Your assigned task: {}", self.original_query));

        // 3. Sync and add shared memory context (once per iteration)
        if let Some(memory_context) = self.sync_from_shared_memory().await? {
            sections.push(memory_context);
        }

        // 4. Add working notes if any
        if !self.working_notes.is_empty() {
            let notes_text = self
                .working_notes
                .iter()
                .map(|n| {
                    format!(
                        "[Iteration {}, {:?}] {}",
                        n.iteration,
                        n.note_type,
                        n.content
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");

            sections.push(format!(
                "=== Your working notes ===\n{}\n==========================",
                notes_text
            ));
        }

        // 5. Add any custom state
        if !self.state.is_empty() {
            let state_text = self
                .state
                .iter()
                .map(|(k, v)| format!("{}: {}", k, v))
                .collect::<Vec<_>>()
                .join("\n");

            sections.push(format!("=== State ===\n{}\n=============", state_text));
        }

        Ok(sections.join("\n\n"))
    }

    /// Get global context (date, system info) - static method
    fn get_global_context() -> String {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Convert timestamp to date components (UTC)
        let days_since_epoch = (timestamp / 86400) as i64;
        let z = days_since_epoch + 719468;
        let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
        let doe = (z - era * 146097) as u32;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = if m <= 2 { y + 1 } else { y };
        let month = m;
        let day = d;

        let month_names = [
            "January", "February", "March", "April", "May", "June",
            "July", "August", "September", "October", "November", "December",
        ];
        let month_name = month_names[(month - 1) as usize];

        let day_of_week_names = [
            "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
        ];
        let day_of_week = ((days_since_epoch + 3) % 7) as usize;
        let day_name = day_of_week_names[day_of_week];

        format!(
            "=== Context ===\nCurrent date: {} {}, {} ({})\nSystem: {} ({})\n===============",
            month_name,
            day,
            year,
            day_name,
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    }

    /// Get the original query
    #[allow(dead_code)]
    pub fn get_query(&self) -> &str {
        &self.original_query
    }

    /// Get the agent prompt
    #[allow(dead_code)]
    pub fn get_agent_prompt(&self) -> &str {
        &self.agent_prompt
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_context() {
        let ctx = DynamicContext::new(
            "Test query".to_string(),
            "Test prompt".to_string(),
            None,
        );
        assert_eq!(ctx.get_query(), "Test query");
        assert_eq!(ctx.get_agent_prompt(), "Test prompt");
        assert_eq!(ctx.current_iteration(), 0);
    }

    #[tokio::test]
    async fn test_iterations() {
        let mut ctx = DynamicContext::new(
            "Test".to_string(),
            "Prompt".to_string(),
            None,
        );
        assert_eq!(ctx.current_iteration(), 0);

        ctx.next_iteration();
        assert_eq!(ctx.current_iteration(), 1);

        ctx.next_iteration();
        assert_eq!(ctx.current_iteration(), 2);
    }

    #[tokio::test]
    async fn test_working_notes() {
        let mut ctx = DynamicContext::new(
            "Test".to_string(),
            "Prompt".to_string(),
            None,
        );

        ctx.add_note("First observation".to_string(), NoteType::Observation);
        ctx.add_note("Tool result".to_string(), NoteType::ToolResult);

        let notes = ctx.get_notes();
        assert_eq!(notes.len(), 2);
        assert_eq!(notes[0].content, "First observation");
        assert_eq!(notes[1].content, "Tool result");
    }

    #[tokio::test]
    async fn test_state() {
        let mut ctx = DynamicContext::new(
            "Test".to_string(),
            "Prompt".to_string(),
            None,
        );

        ctx.set_state("key1".to_string(), "value1".to_string());
        ctx.set_state("key2".to_string(), "value2".to_string());

        assert_eq!(ctx.get_state("key1"), Some(&"value1".to_string()));
        assert_eq!(ctx.get_state("key2"), Some(&"value2".to_string()));
        assert_eq!(ctx.get_state("nonexistent"), None);
    }

    #[tokio::test]
    async fn test_build_prompt_context() {
        let mut ctx = DynamicContext::new(
            "What is the capital of France?".to_string(),
            "You are a helpful assistant".to_string(),
            None,
        );

        ctx.add_note("Paris is mentioned".to_string(), NoteType::Observation);
        ctx.set_state("sources_checked".to_string(), "wikipedia".to_string());

        let result = ctx.build_prompt_context().await;
        assert!(result.is_ok());

        let context = result.unwrap();
        assert!(context.contains("Current date:"));
        assert!(context.contains("What is the capital of France?"));
        assert!(context.contains("Paris is mentioned"));
        assert!(context.contains("sources_checked"));
    }

    #[tokio::test]
    async fn test_global_context() {
        let context = DynamicContext::get_global_context();
        assert!(context.contains("Current date:"));
        assert!(context.contains("System:"));
    }
}
