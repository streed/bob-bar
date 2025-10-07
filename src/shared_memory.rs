use anyhow::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex;

use crate::embeddings;

#[allow(dead_code)]
const SIMILARITY_THRESHOLD: f32 = 0.90; // 90% similarity = duplicate

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memory {
    pub memory_type: MemoryType,
    pub content: String,
    pub metadata: HashMap<String, String>,
    pub created_by: String,
    pub created_at: SystemTime,
    #[serde(skip)]
    #[allow(dead_code)]
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MemoryType {
    Discovery,
    Insight,
    Deadend,
    QueryResult,
    Plan,
    Feedback,
    Context,
}

impl MemoryType {
    pub fn as_str(&self) -> &str {
        match self {
            MemoryType::Discovery => "discovery",
            MemoryType::Insight => "insight",
            MemoryType::Deadend => "deadend",
            MemoryType::QueryResult => "query_result",
            MemoryType::Plan => "plan",
            MemoryType::Feedback => "feedback",
            MemoryType::Context => "context",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "discovery" => Some(MemoryType::Discovery),
            "insight" => Some(MemoryType::Insight),
            "deadend" => Some(MemoryType::Deadend),
            "query_result" => Some(MemoryType::QueryResult),
            "plan" => Some(MemoryType::Plan),
            "feedback" => Some(MemoryType::Feedback),
            "context" => Some(MemoryType::Context),
            _ => None,
        }
    }
}

pub struct SharedMemory {
    ollama_host: String,
    embedding_model: String,
    #[allow(dead_code)]
    embedding_dimensions: usize,
    db: Arc<Mutex<Connection>>,
}

impl SharedMemory {
    pub fn new(
        ollama_host: String,
        embedding_model: String,
        embedding_dimensions: usize,
    ) -> Result<Self> {
        // Register sqlite-vec as an auto-loading extension
        // This needs to be done once, but it's safe to call multiple times
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }

        // Get persistent database path
        let db_path = crate::config::Config::get_config_dir().join("communication.sqlite");

        // Ensure config directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Open persistent SQLite database (vec extension will auto-load)
        let db = Connection::open(&db_path)?;

        // Verify vec0 extension is loaded by trying to create a test query
        let vec_test = db.query_row(
            "SELECT COUNT(*) FROM pragma_module_list",
            [],
            |row| row.get::<_, i32>(0)
        );

        match vec_test {
            Ok(count) => {
                // Now check if vec0 is in the list
                let has_vec0 = db.query_row(
                    "SELECT COUNT(*) FROM pragma_module_list WHERE name='vec0'",
                    [],
                    |row| row.get::<_, i32>(0)
                );

                match has_vec0 {
                    Ok(1) => {
                        eprintln!("[SharedMemory] ✓ vec0 extension loaded (found in {} total modules)", count);
                    }
                    Ok(0) => {
                        eprintln!("[SharedMemory] ✗ WARNING: vec0 extension NOT found among {} modules", count);
                        eprintln!("[SharedMemory] Vector search will NOT work!");
                    }
                    Ok(n) => {
                        eprintln!("[SharedMemory] Unexpected: found {} vec0 modules", n);
                    }
                    Err(e) => {
                        eprintln!("[SharedMemory] ✗ WARNING: Could not check for vec0: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("[SharedMemory] ✗ WARNING: Could not query modules: {}", e);
            }
        }

        // Create memories table if it doesn't exist
        // Using INTEGER PRIMARY KEY makes it an alias for rowid (auto-incrementing)
        db.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY,
                memory_type TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT NOT NULL,
                created_by TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;

        // Create vector table if it doesn't exist
        // Note: Virtual tables don't support IF NOT EXISTS, so check manually
        let table_exists: bool = db
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='vec_memories'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .map(|count| count > 0)
            .unwrap_or(false);

        if table_exists {
            // Check if the existing table has the old TEXT schema
            // Try to detect by checking the sql definition
            let table_sql: String = db
                .query_row(
                    "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_memories'",
                    [],
                    |row| row.get(0)
                )
                .unwrap_or_default();

            // If it contains "TEXT PRIMARY KEY", we need to recreate with INTEGER
            if table_sql.contains("TEXT PRIMARY KEY") {
                eprintln!("[SharedMemory] Detected old vec_memories schema (TEXT). Recreating with INTEGER...");
                db.execute("DROP TABLE vec_memories", [])?;
                db.execute(
                    &format!(
                        "CREATE VIRTUAL TABLE vec_memories USING vec0(
                            memory_id INTEGER PRIMARY KEY,
                            embedding FLOAT[{}]
                        )",
                        embedding_dimensions
                    ),
                    [],
                )?;
                eprintln!("[SharedMemory] ✓ vec_memories recreated with INTEGER PRIMARY KEY");
            }
        } else {
            // Table doesn't exist, create it
            db.execute(
                &format!(
                    "CREATE VIRTUAL TABLE vec_memories USING vec0(
                        memory_id INTEGER PRIMARY KEY,
                        embedding FLOAT[{}]
                    )",
                    embedding_dimensions
                ),
                [],
            )?;
        }

        // Create tool_calls table for tracking tool usage
        db.execute(
            "CREATE TABLE IF NOT EXISTS tool_calls (
                id INTEGER PRIMARY KEY,
                query_id TEXT,
                agent_name TEXT NOT NULL,
                tool_type TEXT NOT NULL,
                tool_name TEXT NOT NULL,
                parameters TEXT NOT NULL,
                created_at INTEGER NOT NULL
            )",
            [],
        )?;

        // Create index for query_id lookups
        db.execute(
            "CREATE INDEX IF NOT EXISTS idx_tool_calls_query_id ON tool_calls(query_id)",
            [],
        )?;

        Ok(Self {
            ollama_host,
            embedding_model,
            embedding_dimensions,
            db: Arc::new(Mutex::new(db)),
        })
    }

    /// Store a new memory with automatic embedding generation
    pub async fn store_memory(
        &self,
        memory_type: MemoryType,
        content: String,
        created_by: String,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<i64> {
        // Generate embedding
        let embedding = embeddings::generate_embedding(
            &self.ollama_host,
            &self.embedding_model,
            &content,
        )
        .await?;

        let metadata_json = serde_json::to_string(&metadata.unwrap_or_default())?;
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;

        // Store in database
        let db = self.db.lock().await;

        // Insert memory (id will be auto-generated by SQLite)
        db.execute(
            "INSERT INTO memories (memory_type, content, metadata, created_by, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                memory_type.as_str(),
                &content,
                &metadata_json,
                &created_by,
                created_at
            ],
        )?;

        // Get the auto-generated rowid
        let id = db.last_insert_rowid();

        // Store embedding as blob
        let embedding_blob: Vec<u8> = embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        db.execute(
            "INSERT INTO vec_memories (memory_id, embedding) VALUES (?1, ?2)",
            params![id, &embedding_blob],
        )?;

        drop(db);

        Ok(id)
    }

    /// Update existing memory or store new one if not found
    /// This is useful for supervisor feedback which should replace previous feedback
    pub async fn update_or_store_memory(
        &self,
        memory_type: MemoryType,
        content: String,
        created_by: String,
        metadata: Option<HashMap<String, String>>,
    ) -> Result<i64> {
        // Try to find existing memory with same type, creator, and query_id
        let query_id = metadata.as_ref()
            .and_then(|m| m.get("query_id"))
            .map(|s| s.as_str());

        let db = self.db.lock().await;

        // Find existing memory
        let existing_id: Option<i64> = if let Some(qid) = query_id {
            let mut stmt = db.prepare(
                "SELECT id FROM memories
                 WHERE memory_type = ?1
                   AND created_by = ?2
                   AND json_extract(metadata, '$.query_id') = ?3
                 LIMIT 1"
            )?;

            let result = stmt.query_row(
                params![memory_type.as_str(), &created_by, qid],
                |row| row.get(0)
            ).ok();

            if result.is_some() {
                eprintln!("[Memory] Found existing {} from {} for query {}, will update",
                    memory_type.as_str(), created_by, qid);
            } else {
                eprintln!("[Memory] No existing {} from {} for query {}, will create new",
                    memory_type.as_str(), created_by, qid);
            }

            result
        } else {
            eprintln!("[Memory] No query_id in metadata, will create new {} from {}",
                memory_type.as_str(), created_by);
            None
        };

        drop(db);

        if let Some(id) = existing_id {
            // Update existing memory
            let embedding = embeddings::generate_embedding(
                &self.ollama_host,
                &self.embedding_model,
                &content,
            )
            .await?;

            let metadata_json = serde_json::to_string(&metadata.unwrap_or_default())?;
            let created_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)?
                .as_secs() as i64;

            let db = self.db.lock().await;

            // Update memory content and timestamp
            db.execute(
                "UPDATE memories
                 SET content = ?1, metadata = ?2, created_at = ?3
                 WHERE id = ?4",
                params![&content, &metadata_json, created_at, id],
            )?;

            // Update embedding
            let embedding_blob: Vec<u8> = embedding
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect();

            db.execute(
                "UPDATE vec_memories
                 SET embedding = ?1
                 WHERE memory_id = ?2",
                params![&embedding_blob, id],
            )?;

            drop(db);

            Ok(id)
        } else {
            // No existing memory found, store new one
            self.store_memory(memory_type, content, created_by, metadata).await
        }
    }

    /// Search for similar memories by content
    pub async fn search_similar(
        &self,
        query: &str,
        memory_type: Option<MemoryType>,
        top_k: usize,
    ) -> Result<Vec<Memory>> {
        // Generate query embedding
        let query_embedding = embeddings::generate_embedding(
            &self.ollama_host,
            &self.embedding_model,
            query,
        )
        .await?;

        let query_blob: Vec<u8> = query_embedding
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        let db = self.db.lock().await;

        // Vector search query
        let type_filter = if let Some(ref mt) = memory_type {
            format!("AND m.memory_type = '{}'", mt.as_str())
        } else {
            String::new()
        };

        let sql = format!(
            "SELECT m.memory_type, m.content, m.metadata, m.created_by, m.created_at
             FROM memories m
             JOIN (
                 SELECT memory_id, distance
                 FROM vec_memories
                 WHERE embedding MATCH ?1
                 ORDER BY distance
                 LIMIT ?2
             ) v ON m.id = v.memory_id
             WHERE 1=1 {}
             ORDER BY v.distance",
            type_filter
        );

        let mut stmt = db.prepare(&sql)?;
        let memories = stmt
            .query_map(params![&query_blob, top_k as i64], |row| {
                let metadata_json: String = row.get(2)?;
                let metadata: HashMap<String, String> =
                    serde_json::from_str(&metadata_json).unwrap_or_default();

                let created_at_secs: i64 = row.get(4)?;
                let created_at = UNIX_EPOCH + std::time::Duration::from_secs(created_at_secs as u64);

                Ok(Memory {
                    memory_type: MemoryType::from_str(&row.get::<_, String>(0)?)
                        .unwrap_or(MemoryType::Context),
                    content: row.get(1)?,
                    metadata,
                    created_by: row.get(3)?,
                    created_at,
                    embedding: None, // Don't return embedding to save memory
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(memories)
    }

    /// Check if a query has been executed before (deduplication)
    #[allow(dead_code)]
    pub async fn check_duplicate_query(&self, query: &str) -> Result<Option<Memory>> {
        let similar = self
            .search_similar(query, Some(MemoryType::QueryResult), 1)
            .await?;

        if let Some(memory) = similar.first() {
            // Note: We'd need to store similarity scores to check threshold
            // For now, just return if we found something
            return Ok(Some(memory.clone()));
        }

        Ok(None)
    }

    /// Get all memories by type
    pub async fn get_by_type(&self, memory_type: MemoryType) -> Vec<Memory> {
        let db = match self.db.lock().await {
            db => db,
        };

        let sql = "SELECT memory_type, content, metadata, created_by, created_at
                   FROM memories WHERE memory_type = ?1";

        let mut stmt = match db.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        stmt.query_map(params![memory_type.as_str()], |row| {
            let metadata_json: String = row.get(2)?;
            let metadata: HashMap<String, String> =
                serde_json::from_str(&metadata_json).unwrap_or_default();

            let created_at_secs: i64 = row.get(4)?;
            let created_at = UNIX_EPOCH + std::time::Duration::from_secs(created_at_secs as u64);

            Ok(Memory {
                memory_type: MemoryType::from_str(&row.get::<_, String>(0)?)
                    .unwrap_or(MemoryType::Context),
                content: row.get(1)?,
                metadata,
                created_by: row.get(3)?,
                created_at,
                embedding: None,
            })
        })
        .ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    // Unused - commented out after removing id field
    // /// Get all memories by agent
    // pub async fn get_by_agent(&self, agent_name: &str) -> Vec<Memory> {
    //     let db = match self.db.lock().await {
    //         db => db,
    //     };

    //     let sql = "SELECT id, memory_type, content, metadata, created_by, created_at
    //                FROM memories WHERE created_by = ?1";

    //     let mut stmt = match db.prepare(sql) {
    //         Ok(s) => s,
    //         Err(_) => return Vec::new(),
    //     };

    //     stmt.query_map(params![agent_name], |row| {
    //         let metadata_json: String = row.get(3)?;
    //         let metadata: HashMap<String, String> =
    //             serde_json::from_str(&metadata_json).unwrap_or_default();

    //         let created_at_secs: i64 = row.get(5)?;
    //         let created_at = UNIX_EPOCH + std::time::Duration::from_secs(created_at_secs as u64);

    //         Ok(Memory {
    //             id: row.get(0)?,
    //             memory_type: MemoryType::from_str(&row.get::<_, String>(1)?)
    //                 .unwrap_or(MemoryType::Context),
    //             content: row.get(2)?,
    //             metadata,
    //             created_by: row.get(4)?,
    //             created_at,
    //             embedding: None,
    //         })
    //     })
    //     .ok()
    //     .map(|rows| rows.filter_map(|r| r.ok()).collect())
    //     .unwrap_or_default()
    // }

    // Unused - commented out after removing id field
    // /// Get a specific memory by ID
    // pub async fn get_memory(&self, id: &str) -> Option<Memory> {
    //     let db = self.db.lock().await;

    //     let sql = "SELECT id, memory_type, content, metadata, created_by, created_at
    //                FROM memories WHERE id = ?1";

    //     db.query_row(sql, params![id], |row| {
    //         let metadata_json: String = row.get(3)?;
    //         let metadata: HashMap<String, String> =
    //             serde_json::from_str(&metadata_json).unwrap_or_default();

    //         let created_at_secs: i64 = row.get(5)?;
    //         let created_at = UNIX_EPOCH + std::time::Duration::from_secs(created_at_secs as u64);

    //         Ok(Memory {
    //             id: row.get(0)?,
    //             memory_type: MemoryType::from_str(&row.get::<_, String>(1)?)
    //                 .unwrap_or(MemoryType::Context),
    //             content: row.get(2)?,
    //             metadata,
    //             created_by: row.get(4)?,
    //             created_at,
    //             embedding: None,
    //         })
    //     })
    //     .ok()
    // }

    // Unused - commented out after removing id field
    // /// Get all memories (for debugging/inspection)
    // pub async fn get_all_memories(&self) -> Vec<Memory> {
    //     let db = match self.db.lock().await {
    //         db => db,
    //     };

    //     let sql = "SELECT id, memory_type, content, metadata, created_by, created_at FROM memories";

    //     let mut stmt = match db.prepare(sql) {
    //         Ok(s) => s,
    //         Err(_) => return Vec::new(),
    //     };

    //     stmt.query_map([], |row| {
    //         let metadata_json: String = row.get(3)?;
    //         let metadata: HashMap<String, String> =
    //             serde_json::from_str(&metadata_json).unwrap_or_default();

    //         let created_at_secs: i64 = row.get(5)?;
    //         let created_at = UNIX_EPOCH + std::time::Duration::from_secs(created_at_secs as u64);

    //         Ok(Memory {
    //             id: row.get(0)?,
    //             memory_type: MemoryType::from_str(&row.get::<_, String>(1)?)
    //                 .unwrap_or(MemoryType::Context),
    //             content: row.get(2)?,
    //             metadata,
    //             created_by: row.get(4)?,
    //             created_at,
    //             embedding: None,
    //         })
    //     })
    //     .ok()
    //     .map(|rows| rows.filter_map(|r| r.ok()).collect())
    //     .unwrap_or_default()
    // }

    /// Get memory statistics
    pub async fn get_stats(&self) -> MemoryStats {
        let db = self.db.lock().await;
        let mut stats = MemoryStats::default();

        if let Ok(count) = db.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get::<_, i64>(0)) {
            stats.total_count = count as usize;
        }

        for memory_type in &[
            MemoryType::Discovery,
            MemoryType::Insight,
            MemoryType::Deadend,
            MemoryType::QueryResult,
            MemoryType::Plan,
            MemoryType::Feedback,
            MemoryType::Context,
        ] {
            if let Ok(count) = db.query_row(
                "SELECT COUNT(*) FROM memories WHERE memory_type = ?1",
                params![memory_type.as_str()],
                |row| row.get::<_, i64>(0),
            ) {
                match memory_type {
                    MemoryType::Discovery => stats.discovery_count = count as usize,
                    MemoryType::Insight => stats.insight_count = count as usize,
                    MemoryType::Deadend => stats.deadend_count = count as usize,
                    MemoryType::QueryResult => stats.query_result_count = count as usize,
                    MemoryType::Plan => stats.plan_count = count as usize,
                    MemoryType::Feedback => stats.feedback_count = count as usize,
                    MemoryType::Context => stats.context_count = count as usize,
                }
            }
        }

        stats
    }

    /// Clear all memories (for testing/reset)
    pub async fn clear(&self) -> Result<()> {
        let db = self.db.lock().await;
        db.execute("DELETE FROM memories", [])?;
        db.execute("DELETE FROM vec_memories", [])?;
        Ok(())
    }

    /// Record a tool call
    pub async fn record_tool_call(
        &self,
        query_id: Option<String>,
        agent_name: String,
        tool_type: String,
        tool_name: String,
        parameters: String,
    ) -> Result<()> {
        let db = self.db.lock().await;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_secs() as i64;

        db.execute(
            "INSERT INTO tool_calls (query_id, agent_name, tool_type, tool_name, parameters, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                query_id,
                agent_name,
                tool_type,
                tool_name,
                parameters,
                timestamp
            ],
        )?;

        Ok(())
    }

    /// Get all tool calls for a specific query_id
    pub async fn get_tool_calls(&self, query_id: Option<&str>) -> Result<Vec<ToolCall>> {
        let db = self.db.lock().await;

        let (sql, param): (&str, Box<dyn rusqlite::ToSql>) = if let Some(qid) = query_id {
            (
                "SELECT agent_name, tool_type, tool_name, parameters, created_at FROM tool_calls WHERE query_id = ?1 ORDER BY created_at",
                Box::new(qid.to_string())
            )
        } else {
            (
                "SELECT agent_name, tool_type, tool_name, parameters, created_at FROM tool_calls WHERE query_id IS NULL ORDER BY created_at",
                Box::new(rusqlite::types::Null)
            )
        };

        let mut stmt = db.prepare(sql)?;
        let tool_calls: Vec<ToolCall> = if query_id.is_some() {
            stmt.query_map([param], |row| {
                Ok(ToolCall {
                    agent_name: row.get(0)?,
                    tool_type: row.get(1)?,
                    tool_name: row.get(2)?,
                    parameters: row.get(3)?,
                    created_at: UNIX_EPOCH + std::time::Duration::from_secs(row.get::<_, i64>(4)? as u64),
                })
            })?.collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map([], |row| {
                Ok(ToolCall {
                    agent_name: row.get(0)?,
                    tool_type: row.get(1)?,
                    tool_name: row.get(2)?,
                    parameters: row.get(3)?,
                    created_at: UNIX_EPOCH + std::time::Duration::from_secs(row.get::<_, i64>(4)? as u64),
                })
            })?.collect::<Result<Vec<_>, _>>()?
        };

        Ok(tool_calls)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolCall {
    pub agent_name: String,
    pub tool_type: String,
    pub tool_name: String,
    pub parameters: String,
    pub created_at: SystemTime,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct MemoryStats {
    pub total_count: usize,
    pub discovery_count: usize,
    pub insight_count: usize,
    pub deadend_count: usize,
    pub query_result_count: usize,
    pub plan_count: usize,
    pub feedback_count: usize,
    pub context_count: usize,
}

impl std::fmt::Display for MemoryStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Total: {} (Discoveries: {}, Insights: {}, Deadends: {}, Cached Queries: {})",
            self.total_count,
            self.discovery_count,
            self.insight_count,
            self.deadend_count,
            self.query_result_count
        )
    }
}
