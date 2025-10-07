# Architecture Overview

This document provides a high-level overview of bob-bar's architecture and how components interact.

## System Components

```
┌─────────────────────────────────────────────────────────────────┐
│                         Bob-Bar GUI                             │
│                    (Iced Application)                           │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Ollama Client                                │
│              (LLM Communication Layer)                          │
└────────────┬───────────────────────────┬────────────────────────┘
             │                           │
             ▼                           ▼
┌────────────────────────┐   ┌──────────────────────────────────┐
│   Research Engine      │   │     Tool Executor                │
│  - Multi-Agent System  │   │  - Web Search                    │
│  - Planning            │   │  - Wikipedia                     │
│  - Worker Execution    │   │  - Semantic Scholar              │
│  - Debate System       │   │  - arXiv Search                  │
│  - Document Writing    │   │  - Weather API                   │
└────────┬───────────────┘   │  - News Search                   │
         │                   │  - Memory Tools                  │
         │                   └──────────┬───────────────────────┘
         │                              │
         ▼                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Shared Memory System                         │
│                  (SQLite + Vector Search)                       │
│  - Discoveries      - Insights        - Deadends                │
│  - Feedback         - Plans           - Context                 │
│  - Query Results    - Tool Call Logs                            │
└─────────────────────────────────────────────────────────────────┘
```

## Core Modules

### 1. Main Application (`src/main.rs`)

**Purpose**: GUI and application lifecycle management

**Responsibilities**:
- Initialize Iced GUI framework
- Handle user input (text entry, image paste)
- Route requests to appropriate handlers (chat vs research)
- Display streaming responses
- Manage application state

**Key Types**:
- `BobBar`: Main application state
- `Message`: Events (user input, responses, notifications)
- `view()`: Renders UI components

### 2. Ollama Client (`src/ollama.rs`)

**Purpose**: LLM communication and tool execution orchestration

**Responsibilities**:
- Send prompts to Ollama API
- Parse streaming responses
- Detect and execute tool calls
- Handle multi-turn conversations with tools
- Format messages for different LLM models

**Key Functions**:
- `query_streaming()`: Send prompt, stream response, handle tools
- `execute_tool()`: Route tool calls to appropriate executor
- `parse_tool_calls()`: Extract tool invocations from LLM response

**Tool Execution Flow**:
```
1. Send prompt to LLM
2. Receive response (may contain tool calls)
3. Parse tool calls from response
4. Execute each tool via ToolExecutor
5. Append tool results to conversation
6. Send back to LLM with results
7. Repeat until LLM responds without tools (max turns)
```

### 3. Research Engine (`src/research.rs`)

**Purpose**: Multi-agent research orchestration

**Responsibilities**:
- Decompose queries into sub-questions
- Manage agent lifecycle (lead, workers, supervisor, debate, writer)
- Coordinate shared memory access
- Execute research pipeline stages
- Synthesize final documents

**Key Types**:
- `ResearchEngine`: Main research orchestrator
- `AgentsConfig`: Agent definitions from agents.json
- `WorkerResult`: Output from individual workers
- `SubQuestion`: Question assigned to a worker

**Pipeline Stages**:
1. **Planning** → Plan critic → Approved plan
2. **Worker Execution** → Supervisor monitoring
3. **Combination** → Merge worker outputs
4. **Debate** → Advocate/Skeptic/Synthesizer review
5. **Refinement** → Fix gaps identified in debate
6. **Document Writing** → Document critic → Final document

### 4. Shared Memory (`src/shared_memory.rs`)

**Purpose**: Persistent coordination layer for agents

**Responsibilities**:
- Store and retrieve typed memories (discoveries, insights, etc.)
- Generate and store vector embeddings
- Perform semantic similarity search
- Track tool usage and performance
- Export memory snapshots

**Key Types**:
- `SharedMemory`: SQLite wrapper with vector support
- `MemoryType`: Enum for different memory categories
- `Memory`: Retrieved memory with metadata

**Database Schema**:
```sql
-- Main memory table
CREATE TABLE memories (
    id INTEGER PRIMARY KEY,
    query_id TEXT,
    memory_type TEXT,    -- discovery, insight, deadend, etc.
    content TEXT,
    created_by TEXT,     -- agent name
    created_at INTEGER,  -- unix timestamp
    metadata TEXT        -- JSON blob
);

-- Vector embeddings table (vec0 extension)
CREATE VIRTUAL TABLE vec_memories USING vec0(
    memory_id INTEGER PRIMARY KEY,
    embedding FLOAT[768]  -- nomic-embed-text dimensions
);
```

**Memory Types**:
- **Discovery**: Factual findings with sources
- **Insight**: Patterns or observations
- **Deadend**: Failed searches to avoid duplication
- **Feedback**: Supervisor guidance to workers
- **Plan**: Research strategy and approach
- **Context**: Background information
- **QueryResult**: Direct tool output

### 5. Tool System (`src/tools.rs`)

**Purpose**: External tool integration and execution

**Responsibilities**:
- Execute builtin tools (web_search, wikipedia, etc.)
- Execute HTTP tools (custom APIs)
- Execute MCP tools (Model Context Protocol)
- Format tool results for LLM consumption
- Smart summarization of large responses

**Tool Types**:

**Builtin Tools** (src/tools.rs:849-1050):
- `web_search`: Brave Search API
- `news_search`: News API
- `wikipedia`: Wikipedia API
- `semantic_scholar`: Academic papers
- `arxiv_search`: arXiv papers
- `weather`: OpenWeather API
- `web_fetch`: Fetch and convert webpage to markdown

**Memory Tools** (src/tools.rs:868-939):
- `memory_store`: Store discoveries/insights/deadends
- `memory_search`: Semantic search across memories
- `memory_get_discoveries`: Get all discoveries
- `memory_get_insights`: Get all insights
- `memory_get_deadends`: Get failed searches
- `memory_get_feedback`: Get supervisor feedback
- `memory_get_plan`: Get research plan
- `memory_get_context`: Get background context

**HTTP Tools**: Custom API endpoints defined in config

**MCP Tools**: Model Context Protocol server tools

### 6. Configuration (`src/config.rs`)

**Purpose**: Centralized configuration management

**Configuration Files**:
- `~/.config/bob-bar/config.toml`: Main settings
- `~/.config/bob-bar/agents.json`: Agent definitions
- `~/.config/bob-bar/notifications.json`: Notification handlers

**Key Settings**:
```toml
[ollama]
host = "http://localhost:11434"
model = "gpt-oss:120b-cloud"
research_model = "gpt-oss:120b-cloud"
embedding_model = "nomic-embed-text"
embedding_dimensions = 768

# Iteration limits
max_plan_iterations = 3          # Plan review cycles
max_refinement_iterations = 5    # Research refinement cycles
max_document_iterations = 3      # Document writing cycles
max_debate_rounds = 2            # Debate rounds
max_tool_turns = 5               # Max tool calls per query

# Context and summarization
context_window = 128000
summarization_threshold = 5000
summarization_threshold_research = 10000

[research]
min_worker_count = 3
max_worker_count = 10
export_memories = false
```

## Data Flow

### Research Query Flow

```
User Query
    │
    ▼
┌─────────────────────────┐
│ 1. Clear Memories       │ DELETE FROM memories
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ 2. Planning Phase       │
│  - Generate plan        │ Lead agent creates questions
│  - Plan critic review   │ Critic evaluates coverage
│  - Refine (iterate)     │ Up to max_plan_iterations
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ 3. Store Plan           │ INSERT INTO memories (type=plan)
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ 4. Worker Execution     │
│  - Spawn N workers      │ Parallel execution
│  - Each has tools       │ web_search, memory_store, etc.
│  - Store discoveries    │ Workers call memory_store
│  - Supervisor monitors  │ Every 15s, updates feedback
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ 5. Combine Results      │ Merge worker outputs
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ 6. Debate               │
│  - Advocate argues      │ Supports findings
│  - Skeptic challenges   │ Questions claims
│  - Synthesizer decides  │ Final verdict
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ 7. Refinement           │ Fix gaps from debate
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ 8. Document Writing     │
│  - Writer drafts        │ Synthesis with citations
│  - Critic reviews       │ Check coverage, sources
│  - Revise (iterate)     │ Up to max_document_iterations
└────────┬────────────────┘
         │
         ▼
┌─────────────────────────┐
│ 9. Add References       │ Extract and format sources
└────────┬────────────────┘
         │
         ▼
    Final Document
```

## Agent Communication

Agents don't directly communicate. Instead, they use **shared memory** as a coordination mechanism:

### Direct Memory (Structured)

```rust
// Worker stores a discovery
memory_store(
    type="discovery",
    content="Python 3.12 released [Source: Python.org](https://...)",
    agent="web_researcher"
)

// Another worker retrieves it
memory_get_discoveries()
// Returns all discoveries from all workers
```

### Semantic Search (Unstructured)

```rust
// Worker searches for related findings
memory_search(query="Python performance benchmarks")
// Returns: Memories ranked by embedding similarity
// Uses vec0 extension for fast vector search
```

### Context Injection

Before each worker executes, they receive automatic context:
```
========== RESEARCH CONTEXT ==========

📋 PLAN (what you should focus on):
Research Python performance metrics...

👁️ SUPERVISOR FEEDBACK:
Focus on verified benchmark sources, not anecdotes...

🔍 RELEVANT DISCOVERIES (from other agents):
- Discovery 1: Python 3.12 is 20% faster [Source](...)
- Discovery 2: Benchmark data from PyPerformance [Source](...)

⚠️ DEADENDS TO AVOID:
- Searched "python speed comparison" on Example.com - no results
```

This context is assembled from memory and prepended to the worker's prompt.

## Concurrency Model

Bob-bar uses Rust async/await with Tokio:

### Parallel Worker Execution

```rust
// Spawn all workers in parallel
let handles: Vec<_> = sub_questions.iter().map(|sq| {
    tokio::spawn(async move {
        execute_worker(sq).await
    })
}).collect();

// Also spawn supervisor in parallel
let supervisor_handle = tokio::spawn(async move {
    supervise_workers().await
});

// Wait for all to complete
let results = join_all(handles).await;
```

### Sequential Tool Execution

Within a single agent's conversation, tool calls are sequential:
```
1. Agent requests: web_search("Python 3.12")
2. Execute tool → get result
3. Append result to conversation
4. Agent requests: memory_store(...)
5. Execute tool → get confirmation
6. Append confirmation
7. Agent provides final answer
```

### Memory Locking

SQLite database is protected by async mutex:
```rust
pub struct SharedMemory {
    db: Arc<Mutex<Connection>>, // Only one writer at a time
}

// All access requires lock:
let db = self.db.lock().await;
db.execute("INSERT INTO memories...", params)?;
```

## Error Handling

Bob-bar uses Rust's `Result` type throughout:

```rust
type Result<T> = anyhow::Result<T>;
```

**Error Propagation**:
- Early pipeline stages: Return error, abort research
- Worker failures: Log error, continue with remaining workers
- Tool failures: Return error message to LLM, let it retry/adapt
- Memory failures: Log warning, continue (graceful degradation)

**Example**:
```rust
// Critical error - abort
let plan = self.decompose_query_and_plan(query).await?;

// Non-critical - log and continue
if let Err(e) = shared_memory.store_memory(...).await {
    eprintln!("Warning: Failed to store memory: {}", e);
}
```

## Extension Points

Bob-bar is designed to be extensible:

### Adding New Agents

1. Define agent in `~/.config/bob-bar/agents.json`
2. Add to appropriate section (workers, debate_agents, etc.)
3. Specify system_prompt and available_tools
4. Agent is automatically loaded and available

### Adding New Tools

**Builtin Tools**: Add to `src/tools.rs:execute_builtin_tool()`

**HTTP Tools**: Add to config with endpoint and parameters

**MCP Tools**: Start MCP server, bob-bar auto-discovers tools

### Adding New Memory Types

1. Add variant to `MemoryType` enum in `src/shared_memory.rs`
2. Implement `as_str()` and `from_str()` for new type
3. Add tool functions if needed (e.g., `memory_get_X`)

## Performance Considerations

### Token Usage

Bob-bar is designed for large context windows (128K+):
- Worker context includes: plan, feedback, discoveries, deadends
- Can use 20-30K tokens per worker easily
- Summarization kicks in when results exceed threshold

### Memory Database Size

- Each research run starts fresh (memories cleared)
- During research: ~50-200 memory entries typical
- Vector embeddings: 768 dimensions × 4 bytes × N entries
- Use `export_memories = true` to save snapshots

### API Rate Limiting

- 500ms delay between sequential LLM calls
- Parallel worker calls happen simultaneously
- Tools may have their own rate limits (configurable)

### Caching

- Ollama caches model weights in memory
- Database has no explicit cache (relies on SQLite)
- Tool results not cached (always fresh data)

## Security Considerations

**API Keys**: Stored in environment variables
```bash
export BRAVE_API_KEY="..."
export NEWS_API_KEY="..."
export OPENWEATHER_API_KEY="..."
```

**Local Execution**: All LLM inference via local Ollama
- No data sent to external LLM APIs
- Research results stay on your machine

**Tool Execution**: Tools run in application process
- Be cautious with HTTP tools from untrusted sources
- MCP tools isolated by protocol design

**Database**: SQLite file in `~/.local/share/bob-bar/`
- Readable by any process with user permissions
- No encryption (research data stored in plaintext)
