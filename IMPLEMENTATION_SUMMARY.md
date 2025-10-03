# Research Mode Implementation Summary

## Overview

Successfully implemented a research mode feature with orchestrator-worker architecture for bob-bar AI assistant.

## Features Implemented

### 1. UI Toggle
- Added research mode toggle button on the right side of input field
- Shows `[Research: ON]` or `[Research: OFF]`
- Only visible when `agents.json` is configured
- Disabled during query processing

### 2. Orchestrator-Worker Architecture

#### Lead Agent
- **Purpose**: Query decomposition
- **Function**: Breaks complex queries into 2-3 focused sub-questions
- **Output**: JSON array of sub-questions
- **Location**: `src/research.rs::decompose_query()`

#### Worker Agents (Parallel Execution)
- **Count**: Configurable (default: 3 workers)
- **Concurrency**: Tokio mpsc channels for coordination
- **Specialization**: Each worker has specific role and tool access
- **Types**:
  - Web Research Specialist (search, weather tools)
  - Data Analyst (weather, github_user tools)
  - General Researcher (all tools)
- **Assignment**: Round-robin distribution of sub-questions
- **Location**: `src/research.rs::execute_workers()`

#### Critic Agent
- **Purpose**: Quality evaluation
- **Function**: Reviews research outputs for accuracy and completeness
- **Output**: "APPROVED" or specific criticism
- **Location**: `src/research.rs::get_criticism()`

#### Refiner Agent
- **Purpose**: Output improvement
- **Function**: Addresses critic feedback to enhance quality
- **Tools**: Can use search and data tools for additional research
- **Location**: `src/research.rs::refine_output()`

### 3. Configuration System

#### agents.json
- **Location**: `~/.config/bob-bar/agents.json` (or config directory)
- **Structure**:
  ```json
  {
    "agents": {
      "lead": {...},
      "workers": [{...}, {...}, {...}],
      "critic": {...},
      "refiner": {...}
    },
    "config": {
      "max_refinement_iterations": 5,
      "worker_count": 3,
      "enable_parallel_workers": true
    }
  }
  ```
- **Agent Properties**:
  - `name`: Display name
  - `role`: Functional role identifier
  - `description`: Agent purpose
  - `system_prompt`: Custom instructions
  - `available_tools`: Array of tool names from tools.json

#### Tool Integration
- Workers access tools defined in existing `tools.json`
- Tool filtering based on `available_tools` configuration
- Supports HTTP tools and MCP servers
- Shared tool executor via `Arc<Mutex<ToolExecutor>>`

### 4. Refinement Loop
- **Max Iterations**: Configurable (default: 5)
- **Process**:
  1. Critic evaluates output
  2. If not approved, provides specific feedback
  3. Refiner improves output based on criticism
  4. Repeat until approved or max iterations
- **Termination**: Either critic approval or iteration limit
- **Location**: `src/research.rs::refinement_loop()`

### 5. Concurrency Implementation

#### Tokio mpsc Channels
```rust
let (tx, mut rx) = mpsc::channel(sub_questions.len());
```
- One sender per worker task
- Centralized receiver for result collection
- Non-blocking architecture

#### Parallel Worker Execution
```rust
for sub_q in sub_questions {
    let handle = tokio::spawn(async move {
        // Worker execution
        let _ = tx.send(worker_result).await;
    });
    handles.push(handle);
}
```

#### Result Collection
```rust
while let Some(result) = rx.recv().await {
    results.push(result);
}
```

## File Changes

### New Files
1. `src/research.rs` - Core research orchestration module (361 lines)
2. `agents.json` - Agent configuration template
3. `RESEARCH_MODE.md` - User documentation
4. `IMPLEMENTATION_SUMMARY.md` - This file

### Modified Files
1. `src/main.rs`:
   - Added `mod research;`
   - Added `ToggleResearchMode` message variant
   - Added `research_mode: bool` field to App
   - Added `research_orchestrator: Option<Arc<Mutex<ResearchOrchestrator>>>` field
   - Updated `App::new()` to initialize research orchestrator
   - Updated `Message::Submit` to route to research mode when enabled
   - Added toggle button to UI view
   - Updated input row layout

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                      User Query                          │
└───────────────────┬─────────────────────────────────────┘
                    │
                    ▼
┌─────────────────────────────────────────────────────────┐
│                   Lead Agent                             │
│  (Decomposes into 2-3 sub-questions)                    │
└───────────────────┬─────────────────────────────────────┘
                    │
        ┌───────────┼───────────┐
        ▼           ▼           ▼
┌──────────┐  ┌──────────┐  ┌──────────┐
│ Worker 1 │  │ Worker 2 │  │ Worker 3 │
│  (Web    │  │  (Data   │  │ (General)│
│ Research)│  │ Analyst) │  │          │
└────┬─────┘  └────┬─────┘  └────┬─────┘
     │             │             │
     └─────────────┴─────────────┘
                   │
       (Tokio mpsc channels)
                   │
                   ▼
┌─────────────────────────────────────────────────────────┐
│              Combined Results                            │
└───────────────────┬─────────────────────────────────────┘
                    │
                    ▼
        ┌───────────────────────┐
        │    Refinement Loop    │
        │  (max 5 iterations)   │
        └───────────────────────┘
                    │
        ┌───────────┴───────────┐
        ▼                       ▼
┌──────────────┐        ┌──────────────┐
│ Critic Agent │───────▶│Refiner Agent │
│  (Evaluate)  │ feedback│  (Improve)   │
└──────────────┘        └──────┬───────┘
        │                      │
        │         ┌────────────┘
        │         │
        ▼         ▼
    APPROVED?  Refined Output
        │         │
        └─────────┘
              │
              ▼
      Final Response
```

## Usage Example

1. Copy `agents.json` to config directory:
   ```bash
   cp agents.json ~/.config/bob-bar/
   ```

2. Ensure `tools.json` is configured with desired tools

3. Run application:
   ```bash
   cargo run --release
   ```

4. Click `[Research: OFF]` to enable research mode

5. Enter complex query:
   ```
   What's the current weather in Tokyo and who are the top
   contributors to the Rust programming language on GitHub?
   ```

6. Watch as:
   - Lead agent creates sub-questions
   - Workers research in parallel
   - Results are combined and refined
   - Final polished answer is presented

## Performance Characteristics

- **Parallel Execution**: Workers run concurrently via Tokio
- **Non-blocking UI**: Iced Task system keeps GUI responsive
- **Resource Sharing**: Single tool executor shared across workers
- **Memory Efficient**: Arc and Mutex for shared state
- **Scalable**: Worker count configurable per query complexity

## Testing Recommendations

1. **Simple Query**: "What is the weather in Tokyo?"
   - Should work in both normal and research mode
   - Research mode will decompose unnecessarily but still work

2. **Complex Query**: "Compare weather, tech ecosystem, and cost of living in Tokyo vs Seattle"
   - Better suited for research mode
   - Tests multi-worker coordination

3. **Tool Integration**: Query requiring specific tools
   - Tests tool filtering per agent
   - Validates worker specialization

4. **Refinement Loop**: Submit ambiguous query
   - Tests critic evaluation
   - Validates refinement iterations

## Future Enhancements

1. **Progress Indicators**: Show which workers are active
2. **Streaming Results**: Display worker results as they arrive
3. **Dynamic Worker Scaling**: Adjust worker count based on query complexity
4. **Agent Memory**: Share context between refinement iterations
5. **Custom Agent Types**: User-defined specialized agents
6. **Result Caching**: Cache sub-question results for similar queries

## Dependencies

No new dependencies added. Uses existing:
- `tokio` - Async runtime and mpsc channels
- `serde_json` - Configuration parsing
- `anyhow` - Error handling
- `iced` - UI framework

## Compatibility

- Works alongside existing normal mode
- Backward compatible (research mode optional)
- No breaking changes to existing features
- Graceful degradation if agents.json missing
