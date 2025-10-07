# Memory System

Bob-bar's shared memory system enables coordination between specialized agents without direct communication. This document explains how the memory system works in detail.

## Overview

The memory system is built on SQLite with the vec0 extension for vector embeddings, providing:

- **Structured Storage**: Typed memories (discoveries, insights, deadends, etc.)
- **Semantic Search**: Find related information via vector similarity
- **Persistence**: Memories survive across agent executions within a research run
- **Isolation**: Each research run starts with cleared memories

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     Agents                                   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ Worker 1 │  │ Worker 2 │  │Supervisor│  │ Refiner  │   │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘   │
└───────┼─────────────┼─────────────┼─────────────┼──────────┘
        │             │             │             │
        ▼             ▼             ▼             ▼
┌─────────────────────────────────────────────────────────────┐
│                Memory Tools (src/tools.rs)                   │
│  memory_store()  memory_search()  memory_get_discoveries()  │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│              SharedMemory (src/shared_memory.rs)             │
│  ┌───────────────────────────────────────────────────────┐  │
│  │  Public API                                           │  │
│  │  - store_memory()                                     │  │
│  │  - update_or_store_memory()                          │  │
│  │  - search_memories()                                  │  │
│  │  - get_memories_by_type()                            │  │
│  │  - clear()                                            │  │
│  └───────────────────────────────────────────────────────┘  │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│                    SQLite Database                           │
│  ┌──────────────────────┐  ┌──────────────────────────┐    │
│  │  memories            │  │  vec_memories (vec0)     │    │
│  │  - id                │  │  - memory_id             │    │
│  │  - query_id          │  │  - embedding[768]        │    │
│  │  - memory_type       │  └──────────────────────────┘    │
│  │  - content           │                                    │
│  │  - created_by        │  ┌──────────────────────────┐    │
│  │  - created_at        │  │  tool_calls              │    │
│  │  - metadata (JSON)   │  │  - query_id              │    │
│  └──────────────────────┘  │  - agent_name            │    │
│                             │  - tool_name             │    │
│                             │  - tool_type             │    │
│                             │  - parameters            │    │
│                             │  - result                │    │
│                             │  - success               │    │
│                             │  - timestamp             │    │
│                             └──────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
```

## Database Schema

### memories Table

**Location**: `src/shared_memory.rs:83-95`

```sql
CREATE TABLE IF NOT EXISTS memories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query_id TEXT,          -- Links memory to specific research session
    memory_type TEXT,       -- discovery, insight, deadend, etc.
    content TEXT,           -- The actual memory content
    created_by TEXT,        -- Agent that created this memory
    created_at INTEGER,     -- Unix timestamp
    metadata TEXT           -- JSON blob for additional data
);

CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memory_type);
CREATE INDEX IF NOT EXISTS idx_memories_query ON memories(query_id);
CREATE INDEX IF NOT EXISTS idx_memories_created ON memories(created_at);
```

**Example Rows**:

```sql
-- Discovery from worker
id: 1
query_id: "query_1234567890_5678"
memory_type: "discovery"
content: "Python 3.12 released October 2023 [Source: Python.org](https://python.org/downloads/)"
created_by: "web_researcher"
created_at: 1234567890
metadata: '{"query_id":"query_1234567890_5678"}'

-- Insight from worker
id: 2
query_id: "query_1234567890_5678"
memory_type: "insight"
content: "Performance benchmarks consistently show 10-100x range for Rust vs Python"
created_by: "data_specialist"
created_at: 1234567891
metadata: '{"query_id":"query_1234567890_5678"}'

-- Deadend from worker
id: 3
query_id: "query_1234567890_5678"
memory_type: "deadend"
content: "Searched 'rust benchmarks' on old-benchmark-site.com - site offline, no results"
created_by: "technical_analyst"
created_at: 1234567892
metadata: '{"query_id":"query_1234567890_5678","reason":"Site unavailable"}'

-- Feedback from supervisor (updated, not inserted)
id: 4
query_id: "query_1234567890_5678"
memory_type: "feedback"
content: "Good progress. Worker 2: Need more quantitative data. Worker 3: Verify benchmark versions."
created_by: "supervisor"
created_at: 1234567920  -- Updated timestamp
metadata: '{"query_id":"query_1234567890_5678","iteration":3}'

-- Plan from lead coordinator
id: 5
query_id: "query_1234567890_5678"
memory_type: "plan"
content: "Strategy: Focus on independent benchmarks from TPC, SPEC, academic sources. Prioritize quantitative data."
created_by: "lead_coordinator"
created_at: 1234567850
metadata: '{"query_id":"query_1234567890_5678","worker_count":6}'
```

### vec_memories Table

**Location**: `src/shared_memory.rs:97-105`

Uses sqlite-vec0 extension for vector similarity search:

```sql
CREATE VIRTUAL TABLE IF NOT EXISTS vec_memories USING vec0(
    memory_id INTEGER PRIMARY KEY,
    embedding FLOAT[768]
);
```

**How it works**:

1. When memory is stored, content is converted to embedding via Ollama:
   ```rust
   let embedding = self.get_embedding(&content).await?;
   // Returns: [0.123, -0.456, 0.789, ..., 0.234] (768 floats)
   ```

2. Embedding stored in vec_memories:
   ```sql
   INSERT INTO vec_memories(memory_id, embedding)
   VALUES (1, '[0.123, -0.456, 0.789, ...]');
   ```

3. Semantic search finds similar memories:
   ```sql
   SELECT m.*, vec_distance_cosine(v.embedding, ?1) as distance
   FROM memories m
   JOIN vec_memories v ON v.memory_id = m.id
   WHERE distance < 0.3
   ORDER BY distance
   LIMIT 10;
   ```

**Why useful**: Agent can find related memories even with different wording.

Example:
```
Query: "memory_search('benchmark methodology')"

Finds memories containing:
- "PyPerformance suite measures execution speed"
- "SPEC CPU2017 benchmark standards"
- "Methodology: controlled environment, 10 runs averaged"

Even though exact phrase "benchmark methodology" doesn't appear.
```

### tool_calls Table

**Location**: `src/shared_memory.rs:107-120`

```sql
CREATE TABLE IF NOT EXISTS tool_calls (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    query_id TEXT,
    agent_name TEXT,
    tool_type TEXT,      -- builtin, http, mcp
    tool_name TEXT,      -- web_search, wikipedia, etc.
    parameters TEXT,     -- JSON of parameters passed
    result TEXT,         -- Tool output (may be summarized)
    success INTEGER,     -- 1 for success, 0 for failure
    timestamp INTEGER
);

CREATE INDEX IF NOT EXISTS idx_tool_calls_query ON tool_calls(query_id);
CREATE INDEX IF NOT EXISTS idx_tool_calls_agent ON tool_calls(agent_name);
```

**Purpose**: Track which tools were used, for debugging and optimization

**Example**:
```sql
INSERT INTO tool_calls VALUES (
    1,
    "query_1234567890_5678",
    "web_researcher",
    "builtin",
    "web_search",
    '{"query":"Python 3.12 benchmarks","max_results":5}',
    '[{"url":"https://python.org/...","title":"Python 3.12 Release","snippet":"..."}]',
    1,
    1234567890
);
```

## Memory Types

**Location**: `src/shared_memory.rs:16-43`

```rust
#[derive(Debug, Clone, Copy)]
pub enum MemoryType {
    Discovery,    // Factual findings with sources
    Insight,      // Patterns or observations across research
    Deadend,      // Failed searches or dead ends
    Feedback,     // Supervisor feedback to workers
    Plan,         // Research plan and strategy
    Context,      // Background information
    QueryResult,  // Direct tool outputs
}
```

### Discovery

**Purpose**: Store factual findings with source citations

**Format**:
```
"Fact statement [Source: Name](URL)"
```

**Examples**:
```
"Python 3.12 released October 2023 [Source: Python.org](https://python.org/downloads/release/python-3120/)"

"Rust shows 50x speedup in CPU-bound tasks [Source: Benchmarks.rs](https://bench.rust-lang.org/)"

"PyPerformance is the official Python benchmark suite [Source: Python Docs](https://docs.python.org/3/whatsnew/3.12.html#optimizations)"
```

**Storage**:
```rust
memory_store(
    type="discovery",
    content="Python 3.12 released October 2023 [Source: Python.org](https://...)",
    agent="web_researcher"
)
```

**Retrieval**:
```rust
memory_get_discoveries()
// Returns all discoveries from all agents
```

**Use Case**: Workers store facts as they research, other workers can see findings

### Insight

**Purpose**: Capture patterns, trends, or meta-observations

**Format**:
```
"Observation: Pattern or trend noticed"
```

**Examples**:
```
"Observation: All benchmarks show 10-100x range for Rust vs Python, suggesting consistent performance characteristics"

"Pattern: Official sources provide quantitative data, third-party sources tend toward qualitative comparisons"

"Trend: Newer Python versions (3.11+) closing performance gap with JIT optimizations"
```

**Storage**:
```rust
memory_store(
    type="insight",
    content="Observation: All benchmarks show 10-100x range",
    agent="data_specialist"
)
```

**Retrieval**:
```rust
memory_get_insights()
```

**Use Case**: Help synthesize findings, identify patterns across disparate facts

### Deadend

**Purpose**: Record failed searches to avoid duplication

**Format**:
```
"Searched [where] for [what] - [why it failed]"
```

**Examples**:
```
"Searched 'rust performance' on old-benchmark-site.com - site offline"

"Tried web_search('Python 3.12 secret optimizations') - only found speculation, no official sources"

"Searched semantic_scholar for 'Rust Python comparison' - papers too old (pre-2020), not relevant to current versions"
```

**Storage**:
```rust
memory_store(
    type="deadend",
    content="Searched 'rust benchmarks' on old-site.com - site offline",
    agent="technical_analyst"
)
```

**Retrieval**:
```rust
memory_get_deadends()
```

**Use Case**: Prevent other workers from trying the same failed approaches

### Feedback

**Purpose**: Supervisor guidance to workers

**Format**: Free-form text with specific directives

**Example**:
```
"Good progress on Python benchmarks. Need more Rust data.

Worker 2: Expand on memory usage comparison with specific numbers.
Worker 3: Verify benchmark versions - cite specific PyPerformance release.
Worker 5: Found good sources but need more recent data (post-2023).

Focus areas:
- Quantitative metrics over general statements
- Official sources (Python.org, Rust docs) preferred
- Specify versions for all benchmarks"
```

**Storage** (special - uses UPDATE not INSERT):
```rust
memory.update_or_store_memory(
    MemoryType::Feedback,
    "Good progress...",
    "supervisor",
    metadata
)
// First call: INSERT
// Subsequent calls: UPDATE existing row
```

**Retrieval**:
```rust
memory_get_feedback()
```

**Use Case**: Supervisor monitors progress, provides real-time guidance

**Why UPDATE instead of INSERT**: Supervisor updates every 15 seconds. Using INSERT would create 20+ feedback rows per research run. UPDATE keeps only the latest feedback, preventing database bloat.

### Plan

**Purpose**: Store research strategy and approach

**Format**: Strategy text + question breakdown

**Example**:
```
"Strategy: Focus on independent benchmarks from TPC, SPEC, and academic sources.
Prioritize quantitative data over anecdotes. Cross-verify vendor claims with third-party testing.

Questions assigned:
1. What are Python's documented benchmarks? → technical_analyst
2. What are Rust's documented benchmarks? → technical_analyst
3. What metrics differ? → data_specialist
4. Python case studies? → web_researcher
5. Rust case studies? → web_researcher
6. Independent comparisons? → comparative_analyst"
```

**Storage**:
```rust
shared_memory.store_memory(
    MemoryType::Plan,
    plan.clone(),
    "lead_coordinator".to_string(),
    Some(metadata)
).await?;
```

**Retrieval**:
```rust
memory_get_plan()
```

**Use Case**: Workers understand overall research scope and strategy

### Context

**Purpose**: Background information for context

**Format**: Free-form explanatory text

**Example**:
```
"Python 3.11+ introduced significant performance improvements through:
- Faster startup via frozen modules
- Cheaper function calls (30% faster)
- Inline caching for common operations
- Specialized adaptive interpreter

This context is important for understanding why Python 3.11/3.12 benchmarks differ significantly from earlier versions."
```

**Storage**:
```rust
memory_store(
    type="context",
    content="Python 3.11+ introduced significant performance improvements...",
    agent="web_researcher"
)
```

**Retrieval**:
```rust
memory_get_context()
```

**Use Case**: Provide background that helps interpret findings

### QueryResult

**Purpose**: Store raw tool outputs for reference

**Format**: Tool output (may be JSON or text)

**Example**:
```json
{
  "tool": "web_search",
  "query": "Python 3.12 benchmarks",
  "results": [
    {"url": "https://...", "title": "...", "snippet": "..."},
    {"url": "https://...", "title": "...", "snippet": "..."}
  ]
}
```

**Storage**:
```rust
memory_store(
    type="query_result",
    content=serde_json::to_string(&result)?,
    agent="web_researcher"
)
```

**Retrieval**: Via semantic search

**Use Case**: Reference raw data later, verify interpretations

## Memory Operations

### Storing Memory

**API**: `store_memory()`
**Location**: `src/shared_memory.rs:201-283`

```rust
pub async fn store_memory(
    &self,
    memory_type: MemoryType,
    content: String,
    created_by: String,
    metadata: Option<HashMap<String, String>>,
) -> Result<i64>
```

**Process**:

1. **Insert into memories table**:
```rust
db.execute(
    "INSERT INTO memories (query_id, memory_type, content, created_by, created_at, metadata)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    params![
        query_id,
        memory_type.as_str(),
        content,
        created_by,
        timestamp,
        metadata_json
    ]
)?;
```

2. **Generate embedding**:
```rust
let embedding = self.get_embedding(&content).await?;
// Calls Ollama API: POST /api/embeddings
// Model: nomic-embed-text
// Returns: 768-dimensional vector
```

3. **Store embedding**:
```rust
let embedding_json = serde_json::to_string(&embedding)?;
db.execute(
    "INSERT INTO vec_memories(memory_id, embedding) VALUES (?1, ?2)",
    params![memory_id, embedding_json]
)?;
```

**Return**: Memory ID (for reference)

### Updating Memory

**API**: `update_or_store_memory()`
**Location**: `src/shared_memory.rs:284-364`

```rust
pub async fn update_or_store_memory(
    &self,
    memory_type: MemoryType,
    content: String,
    created_by: String,
    metadata: Option<HashMap<String, String>>,
) -> Result<i64>
```

**Process**:

1. **Check if memory exists**:
```rust
let existing_id: Option<i64> = db.query_row(
    "SELECT id FROM memories
     WHERE memory_type = ?1
       AND created_by = ?2
       AND json_extract(metadata, '$.query_id') = ?3
     LIMIT 1",
    params![memory_type.as_str(), created_by, query_id],
    |row| row.get(0)
).ok();
```

2. **If exists, UPDATE**:
```rust
// Update content and timestamp
db.execute(
    "UPDATE memories
     SET content = ?1, created_at = ?2, metadata = ?3
     WHERE id = ?4",
    params![content, new_timestamp, metadata_json, existing_id]
)?;

// Update embedding
let embedding = self.get_embedding(&content).await?;
db.execute(
    "UPDATE vec_memories SET embedding = ?1 WHERE memory_id = ?2",
    params![embedding_json, existing_id]
)?;
```

3. **If not exists, INSERT**:
```rust
self.store_memory(memory_type, content, created_by, metadata).await
```

**Use Case**: Supervisor feedback updates (keeps only latest version)

### Searching Memory

**API**: `search_memories()`
**Location**: `src/shared_memory.rs:404-478`

```rust
pub async fn search_memories(
    &self,
    query: &str,
    limit: usize,
    query_id: Option<String>,
) -> Result<Vec<Memory>>
```

**Process**:

1. **Generate query embedding**:
```rust
let query_embedding = self.get_embedding(query).await?;
```

2. **Vector similarity search**:
```rust
let sql = "
    SELECT m.id, m.query_id, m.memory_type, m.content,
           m.created_by, m.created_at, m.metadata,
           vec_distance_cosine(v.embedding, ?1) as distance
    FROM memories m
    JOIN vec_memories v ON v.memory_id = m.id
    WHERE (?2 IS NULL OR m.query_id = ?2)
    ORDER BY distance ASC
    LIMIT ?3
";

db.query_map(
    sql,
    params![query_embedding_json, query_id, limit],
    |row| { /* parse results */ }
)?;
```

**Similarity Metric**: Cosine distance
- 0.0 = identical vectors (most similar)
- 1.0 = perpendicular vectors
- 2.0 = opposite vectors (least similar)

**Example**:
```rust
memory_search("benchmark methodology", limit=10)

// Finds memories with similar semantic meaning:
// distance=0.12: "PyPerformance suite measures execution speed"
// distance=0.18: "SPEC CPU2017 benchmark standards"
// distance=0.25: "Controlled environment, 10 runs averaged"
```

### Retrieving by Type

**APIs**: `get_memories_by_type()` and specialized getters
**Location**: `src/shared_memory.rs:480-555`

```rust
pub async fn get_memories_by_type(
    &self,
    memory_type: MemoryType,
    query_id: Option<String>,
) -> Result<Vec<Memory>>
```

**Process**:
```rust
let sql = "
    SELECT id, query_id, memory_type, content, created_by, created_at, metadata
    FROM memories
    WHERE memory_type = ?1
      AND (?2 IS NULL OR query_id = ?2)
    ORDER BY created_at DESC
";

db.query_map(sql, params![memory_type.as_str(), query_id], |row| {
    Ok(Memory {
        id: row.get(0)?,
        query_id: row.get(1)?,
        memory_type: row.get(2)?,
        content: row.get(3)?,
        created_by: row.get(4)?,
        created_at: row.get(5)?,
        metadata: row.get(6)?,
    })
})?
```

**Specialized Getters**:
```rust
// All discoveries
get_discoveries(query_id) → get_memories_by_type(Discovery, query_id)

// All insights
get_insights(query_id) → get_memories_by_type(Insight, query_id)

// All deadends
get_deadends(query_id) → get_memories_by_type(Deadend, query_id)

// Latest feedback (only 1 row due to update pattern)
get_feedback(query_id) → get_memories_by_type(Feedback, query_id)

// Research plan
get_plan(query_id) → get_memories_by_type(Plan, query_id)
```

### Clearing Memory

**API**: `clear()`
**Location**: `src/shared_memory.rs:646-651`

```rust
pub async fn clear(&self) -> Result<()> {
    let db = self.db.lock().await;
    db.execute("DELETE FROM memories", [])?;
    db.execute("DELETE FROM vec_memories", [])?;
    Ok(())
}
```

**When Used**: Start of each research run

**Why**: Ensures clean slate, prevents contamination from previous queries

## Memory Tools

Agents access memory through tool calls. These tools are available via the tool executor.

**Location**: `src/tools.rs:868-939`

### memory_store

**Description**: Store a new memory

**Parameters**:
```json
{
  "type": "discovery",
  "content": "Python 3.12 released [Source: Python.org](https://...)",
  "agent": "web_researcher"
}
```

**Implementation**:
```rust
shared_memory.store_memory(
    MemoryType::from_str(&type_str)?,
    content,
    agent,
    Some(metadata)
).await?;
```

**Returns**: Confirmation message

### memory_search

**Description**: Semantic search across all memories

**Parameters**:
```json
{
  "query": "benchmark methodology",
  "limit": 10
}
```

**Implementation**:
```rust
let memories = shared_memory.search_memories(&query, limit, query_id).await?;

// Format results
for memory in memories {
    result += &format!(
        "[{}] by {}: {}\n",
        memory.memory_type,
        memory.created_by,
        memory.content
    );
}
```

**Returns**: Formatted list of similar memories

### memory_get_discoveries

**Description**: Get all discovery-type memories

**Parameters**: None

**Implementation**:
```rust
let discoveries = shared_memory.get_discoveries(query_id).await?;
```

**Returns**: All stored discoveries with sources

**Example Output**:
```
Discovery by web_researcher: Python 3.12 released October 2023 [Source: Python.org](https://...)
Discovery by technical_analyst: PyPerformance shows 20% speedup [Source: PyPerformance](https://...)
Discovery by data_specialist: Rust executes 50x faster in CPU tasks [Source: Benchmarks.rs](https://...)
```

### memory_get_insights

**Description**: Get all insight-type memories

### memory_get_deadends

**Description**: Get all deadend-type memories

### memory_get_feedback

**Description**: Get latest supervisor feedback

### memory_get_plan

**Description**: Get research plan

### memory_get_context

**Description**: Get background context

All follow same pattern as `memory_get_discoveries`.

## Context Assembly

**Location**: `src/research.rs:1280-1333`

Before each worker executes, context is assembled from memory:

```rust
async fn build_worker_context(query_id: &str, shared_memory: &SharedMemory) -> String {
    let mut context = String::from("========== RESEARCH CONTEXT ==========\n\n");

    // 1. Get research plan
    if let Ok(plans) = shared_memory.get_plan(Some(query_id.to_string())).await {
        if !plans.is_empty() {
            context.push_str("📋 PLAN (your assigned scope):\n");
            context.push_str(&plans[0].content);
            context.push_str("\n\n");
        }
    }

    // 2. Get supervisor feedback (latest guidance)
    if let Ok(feedback) = shared_memory.get_feedback(Some(query_id.to_string())).await {
        if !feedback.is_empty() {
            context.push_str("👁️ SUPERVISOR FEEDBACK:\n");
            context.push_str(&feedback[0].content);
            context.push_str("\n\n");
        }
    }

    // 3. Get discoveries from other workers
    if let Ok(discoveries) = shared_memory.get_discoveries(Some(query_id.to_string())).await {
        if !discoveries.is_empty() {
            context.push_str("🔍 RELEVANT DISCOVERIES (from other agents):\n");
            for discovery in discoveries.iter().take(10) {
                context.push_str(&format!("- {}\n", discovery.content));
            }
            context.push_str("\n");
        }
    }

    // 4. Get deadends to avoid
    if let Ok(deadends) = shared_memory.get_deadends(Some(query_id.to_string())).await {
        if !deadends.is_empty() {
            context.push_str("⚠️ APPROACHES TO AVOID (deadends):\n");
            for deadend in deadends.iter().take(5) {
                context.push_str(&format!("- {}\n", deadend.content));
            }
            context.push_str("\n");
        }
    }

    context.push_str("========== END CONTEXT ==========\n\n");
    context
}
```

This context is prepended to the worker's prompt, giving them automatic awareness of:
- What they should focus on (plan)
- Latest guidance from supervisor (feedback)
- What others have found (discoveries)
- What to avoid (deadends)

## Performance Characteristics

### Storage Performance

**Single memory insertion**: ~5-20ms
- SQLite insert: ~1ms
- Ollama embedding generation: ~5-15ms (depends on model)
- Vector storage: ~1ms

**Typical research run**:
- ~50-100 memory inserts
- Total time: ~500-2000ms (0.5-2 seconds)

### Search Performance

**Semantic search**: ~10-50ms depending on database size
- Embedding generation: ~5-15ms
- Vector similarity (cosine distance): ~5-20ms for 100 memories
- Result formatting: ~5-15ms

**Type retrieval**: ~1-5ms
- Simple SQL query with index
- No embedding needed

### Memory Footprint

**Per memory**:
- Text: Variable (typically 100-500 characters = 100-500 bytes)
- Embedding: 768 floats × 4 bytes = 3,072 bytes
- Total: ~3.2-3.5 KB per memory

**Typical research run**:
- 50-100 memories × 3.5 KB = 175-350 KB
- Negligible compared to LLM context (megabytes)

### Database Size

Fresh database:
```
memories: 0 rows
vec_memories: 0 rows
tool_calls: 0 rows
Size: ~20 KB (schema only)
```

After research run:
```
memories: 50-100 rows
vec_memories: 50-100 rows
tool_calls: 200-500 rows
Size: ~500 KB - 2 MB
```

After 100 research runs (without clearing):
```
memories: 5,000-10,000 rows
vec_memories: 5,000-10,000 rows
tool_calls: 20,000-50,000 rows
Size: ~50-200 MB
```

**Solution**: Clear memories at start of each run (already implemented)

## Export and Analysis

### Memory Export

**Configuration**: `export_memories = true` in config.toml

**Location**: `src/research.rs:372-391`

```rust
if self.config.config.export_memories {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let filename = format!("memories_export_{}.json", timestamp);
    let filepath = Path::new(&filename);

    shared_memory.export_to_json(filepath).await?;
    eprintln!("[Research] Exported memories to {}", filename);
}
```

**Export Format** (JSON):
```json
{
  "export_timestamp": 1234567890,
  "query_id": "query_1234567890_5678",
  "memories": [
    {
      "id": 1,
      "query_id": "query_1234567890_5678",
      "memory_type": "discovery",
      "content": "Python 3.12 released [Source](url)",
      "created_by": "web_researcher",
      "created_at": 1234567890,
      "metadata": "{\"query_id\":\"...\"}"
    },
    ...
  ],
  "tool_calls": [
    {
      "id": 1,
      "query_id": "query_1234567890_5678",
      "agent_name": "web_researcher",
      "tool_type": "builtin",
      "tool_name": "web_search",
      "parameters": "{\"query\":\"Python 3.12\"}",
      "result": "[...]",
      "success": 1,
      "timestamp": 1234567890
    },
    ...
  ]
}
```

**Use Cases**:
- Debugging research runs
- Analyzing agent behavior
- Understanding memory usage patterns
- Training data for improving prompts

## Best Practices

### For Agent Prompts

**✓ DO**:
- Store discoveries immediately after finding facts
- Include source URLs in discovery content
- Store deadends when searches fail
- Use memory_search before new research

**✗ DON'T**:
- Store every tool result (only meaningful findings)
- Store duplicate discoveries
- Store unverified claims
- Skip memory storage (defeats purpose)

### For System Design

**✓ DO**:
- Clear memories at start of each research run
- Use update_or_store for frequently updated memories (like feedback)
- Index frequently queried fields
- Export memories for analysis when debugging

**✗ DON'T**:
- Let memories accumulate across runs
- Store large binary data in content field
- Query without query_id filter (slows down search)
- Rely on exact text matching (use semantic search)

### For Tool Usage

**Efficient pattern**:
```
1. Check memory first: memory_get_discoveries()
2. Search semantically: memory_search("topic")
3. If not found, do research: web_search(...)
4. Store finding: memory_store(type="discovery", ...)
```

**Inefficient pattern**:
```
1. Do research: web_search(...)
2. Do more research: web_search(...) [duplicating others]
3. Never store findings
4. Miss what others found
```

## Troubleshooting

### No discoveries being stored

**Problem**: Workers not calling memory_store

**Solution**: Check worker system prompt includes memory workflow instructions

**Location**: `src/research.rs:1358-1378`

### Semantic search not finding relevant memories

**Problem**: Embeddings not properly generated or stored

**Check**:
```sql
SELECT COUNT(*) FROM vec_memories;
-- Should match COUNT(*) FROM memories
```

**Debug**:
```rust
// Check embedding generation
let embedding = shared_memory.get_embedding("test").await?;
assert_eq!(embedding.len(), 768); // Should be 768 dimensions
```

### Database locked errors

**Problem**: Multiple writes attempting simultaneously

**Solution**: Already handled via async mutex in SharedMemory

**If persists**: Check for long-running transactions

### Memory table growing too large

**Problem**: Not clearing between research runs

**Solution**: Verify `shared_memory.clear()` is called at start

**Location**: `src/research.rs:223-231`
