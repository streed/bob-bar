# Research Mode

Research mode implements an orchestrator-worker architecture for complex queries that benefit from multi-agent collaboration.

## Architecture

### Components

1. **Lead Agent** - Decomposes complex queries into 2-3 focused sub-questions
2. **Worker Agents** (2-3 concurrent) - Specialized agents that research sub-questions in parallel using Tokio mpsc channels
3. **Critic Agent** - Evaluates research outputs for quality, accuracy, and completeness
4. **Refiner Agent** - Improves outputs based on critic feedback

### Worker Types

Workers can be specialized for different tasks:
- **Web Research Specialist** - Focuses on web search and online information
- **Data Analyst** - Works with APIs and structured data
- **General Researcher** - All-purpose research with access to all tools

## Configuration

### agents.json

Place this file in your config directory (e.g., `~/.config/bob-bar/agents.json`):

```json
{
  "agents": {
    "lead": {
      "name": "Lead Agent",
      "role": "query_decomposer",
      "description": "Decomposes complex queries",
      "system_prompt": "Your custom prompt...",
      "available_tools": []
    },
    "workers": [
      {
        "name": "Web Research Specialist",
        "role": "web_researcher",
        "description": "Web research specialist",
        "system_prompt": "Your custom prompt...",
        "available_tools": ["search", "weather"]
      }
    ],
    "critic": {
      "name": "Critic Agent",
      "role": "critic",
      "description": "Quality evaluator",
      "system_prompt": "Your custom prompt...",
      "available_tools": []
    },
    "refiner": {
      "name": "Refiner Agent",
      "role": "refiner",
      "description": "Output improver",
      "system_prompt": "Your custom prompt...",
      "available_tools": ["search"]
    }
  },
  "config": {
    "max_refinement_iterations": 5,
    "worker_count": 3,
    "enable_parallel_workers": true
  }
}
```

### Agent Tool Access

Each agent specifies which tools from `tools.json` it can access via the `available_tools` array. This allows you to:
- Give search tools only to research-focused agents
- Restrict data tools to analytical agents
- Control which agents can call external APIs

## Usage

1. **Enable Research Mode**: Click the `[Research: OFF]` button to toggle to `[Research: ON]`

2. **Submit Complex Query**: Enter a query that benefits from multi-faceted research:
   ```
   "Compare the weather patterns in Tokyo and Seattle,
   analyze their tech industries, and suggest which city
   would be better for a software engineer"
   ```

3. **Automatic Process**:
   - Lead agent breaks query into sub-questions
   - Worker agents research in parallel using available tools
   - Results are combined
   - Critic reviews the output
   - Refiner improves based on criticism (up to 5 iterations)

## Refinement Loop

The refinement loop continues until:
- Critic approves the output (responds with "APPROVED")
- Max iterations reached (default: 5, configurable)

Each iteration:
1. Critic evaluates current output
2. If issues found, provides specific criticism
3. Refiner addresses concerns and improves output
4. Repeat

## Channel-Based Communication

Workers use Tokio mpsc channels for coordination:
- Workers execute concurrently in separate tasks
- Results are collected via channel receivers
- Non-blocking architecture keeps UI responsive

## Tool Integration

Workers can access tools defined in `tools.json`:
- HTTP tools (APIs, web services)
- MCP tools (Model Context Protocol servers)
- Tools are filtered by agent's `available_tools` list

## Example Workflow

```
User Query: "What's the weather in Tokyo and latest GitHub trends?"

↓

Lead Agent Decomposes:
1. "What is the current weather in Tokyo?"
2. "What are the trending repositories on GitHub?"

↓

Worker 1 (Web Research Specialist):
- Uses weather tool → Returns Tokyo weather

Worker 2 (Data Analyst):
- Uses GitHub API → Returns trending repos

↓

Combined Output:
# Research Results
## Web Research Specialist
**Question:** Current weather in Tokyo
Tokyo: 18°C, Partly cloudy...

## Data Analyst
**Question:** Trending GitHub repositories
Top repos: rust-lang/rust, microsoft/vscode...

↓

Critic: "Good data but missing context on why repos are trending"

↓

Refiner: Adds context about repo features and recent updates

↓

Critic: "APPROVED"

↓

Final Output Delivered
```

## Performance Considerations

- Workers run in parallel using Tokio tasks
- Each worker has its own OllamaClient instance
- Tool executors are shared via Arc<Mutex<...>>
- UI remains responsive during research
- Progress visible via loading indicator

## Debugging

Enable debug mode to see research orchestration logs:
```bash
cargo run -- --debug
```

Logs will show:
- Query decomposition results
- Worker assignments
- Tool calls and results
- Critic feedback
- Refinement iterations
