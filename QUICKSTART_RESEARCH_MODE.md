# Quick Start: Research Mode

## Setup (5 minutes)

### 1. Copy Configuration Files

```bash
# Navigate to your config directory
cd ~/.config/bob-bar/

# Copy the agents configuration template
cp /path/to/w-ai-land-iced/agents.example.json agents.json

# Verify tools.json exists (should already be there)
ls tools.json
```

### 2. Review Agent Configuration

Open `~/.config/bob-bar/agents.json` and verify:

- **Lead Agent**: Decomposes queries (no tools needed)
- **Workers**: At least 2-3 workers with different tool access
- **Critic**: Evaluates quality (no tools needed)
- **Refiner**: Improves output (has access to tools)

### 3. Configure Tools

Ensure your `tools.json` has the tools referenced in `agents.json`:

```json
{
  "tools": {
    "http": [
      {
        "name": "search",
        "description": "Web search tool",
        ...
      },
      {
        "name": "weather",
        "description": "Weather API",
        ...
      }
    ],
    "mcp": []
  }
}
```

### 4. Launch Application

```bash
# From project directory
cargo run --release

# Or with debug mode to see research orchestration
cargo run --release -- --debug
```

## First Research Query

### 1. Enable Research Mode

Look for the toggle button next to the input field:
```
[Research: OFF]  ← Click this
```

Should change to:
```
[Research: ON]   ← Research mode active
```

### 2. Enter a Complex Query

Good candidates for research mode:

**Multi-faceted Questions:**
```
What's the current weather in Tokyo, and what are the
top trending GitHub repositories this week?
```

**Comparative Analysis:**
```
Compare Python and Rust for web development, including
performance, ecosystem, and learning curve.
```

**Current + Historical:**
```
What's the weather like in Seattle today, and how does
it compare to historical averages for this time of year?
```

### 3. Watch the Process

With debug mode (`--debug` flag), you'll see:
```
[Research] Decomposing query...
[Research] 3 sub-questions created
[Research] Spawning 3 workers...
[Research] Worker 1: Researching "What's the weather in Tokyo?"
[Research] Worker 2: Researching "What are trending GitHub repos?"
[Research] Combining results...
[Research] Iteration 1: Refining based on criticism
[Research] Output approved by critic after 2 iteration(s)
```

### 4. Review Results

The output will be structured like:
```markdown
# Research Results for: [Your Query]

## Web Research Specialist
**Question:** What are trending GitHub repos?

[Answer with data...]

## Data Analyst
**Question:** What's the weather in Tokyo?

[Answer with weather data...]

[Refined and critic-approved output]
```

## Testing Research Mode

### Test 1: Simple Query (Baseline)
```
What is the weather in Tokyo?
```
- Should work but won't show benefits of research mode
- Good for testing basic functionality

### Test 2: Multi-Domain Query
```
What's the weather in San Francisco and who created Python?
```
- Tests worker specialization
- One worker uses weather API, another uses web search
- Shows parallel execution

### Test 3: Complex Research Query
```
Analyze the popularity of Rust vs Go programming languages
based on GitHub activity, community trends, and recent developments.
```
- Tests decomposition into multiple sub-questions
- Shows refinement loop in action
- Demonstrates critic feedback

### Test 4: Tool-Intensive Query
```
Get current weather for Tokyo, New York, and London, then
compare them and explain which has the most pleasant conditions.
```
- Tests multiple tool calls
- Tests result synthesis
- Tests refinement for subjective analysis

## Troubleshooting

### Toggle Button Not Appearing
- Check if `agents.json` exists in config directory
- Review console for loading errors
- Verify JSON syntax is valid

### "Research error: No JSON array found"
- Lead agent didn't return proper format
- Try adjusting lead agent system prompt
- Check if LLM supports JSON output

### Workers Not Finding Tools
- Verify tool names in `available_tools` match `tools.json`
- Check API keys in `api_keys.toml`
- Review tool execution logs with `--debug`

### Refinement Loop Hitting Max Iterations
- Critic is too strict or unclear
- Adjust critic system prompt
- Increase `max_refinement_iterations` in config
- Simplify query or adjust refiner prompt

### Performance Issues
- Reduce `worker_count` from 3 to 2
- Set `enable_parallel_workers: false` for sequential execution
- Check Ollama server response times
- Consider faster model for worker agents

## Configuration Tips

### Optimal Worker Count
- **Simple queries**: 2 workers sufficient
- **Complex queries**: 3 workers recommended
- **Very complex**: Consider 4-5 workers (requires config edit)

### Tool Assignment Strategy
- **Specialist approach**: Each worker has unique tools
- **Generalist approach**: All workers have all tools
- **Hybrid**: Mix of specialists and generalists (recommended)

### Refinement Tuning
- **Quality-focused**: Higher max iterations (7-10)
- **Speed-focused**: Lower max iterations (2-3)
- **Balanced**: Default 5 iterations

### System Prompt Optimization
- Be specific about output format expectations
- Include examples in prompts
- Specify criticism criteria for critic agent
- Define quality standards for refiner

## Advanced Usage

### Custom Agent Roles

Add specialized agents for your domain:

```json
{
  "name": "Code Analyzer",
  "role": "code_analyst",
  "description": "Analyzes code and technical topics",
  "system_prompt": "You are a code analysis expert...",
  "available_tools": ["github_user", "search"]
}
```

### Dynamic Tool Selection

Workers can adaptively choose tools based on query:
- Ensure workers have access to relevant tool sets
- Use clear descriptions in tool definitions
- Let LLM decide which tool to use

### Iterative Refinement Patterns

Common refinement patterns:
1. **Add Sources**: Critic requests citations
2. **Add Detail**: Critic requests more depth
3. **Fix Inaccuracies**: Critic catches errors
4. **Improve Structure**: Critic requests better formatting
5. **Add Context**: Critic requests background information

## Performance Benchmarks

Approximate timings (depends on LLM speed):

| Query Type | Normal Mode | Research Mode |
|-----------|-------------|---------------|
| Simple    | 2-5s        | 8-15s         |
| Medium    | 5-10s       | 15-30s        |
| Complex   | 10-20s      | 30-60s        |

Research mode is slower but provides:
- Better accuracy through multiple perspectives
- More comprehensive answers
- Higher quality through refinement
- Specialized tool usage

## When to Use Research Mode

### ✅ Good Use Cases
- Multi-faceted questions requiring different tools
- Comparative analysis needing multiple data sources
- Complex research requiring verification
- Queries benefiting from specialist perspectives

### ❌ Not Recommended
- Simple factual lookups
- Single-tool queries
- Time-sensitive questions
- Queries requiring instant response

## Next Steps

1. **Experiment** with different query types
2. **Tune** agent prompts for your needs
3. **Add** custom tools to `tools.json`
4. **Create** specialized worker agents
5. **Monitor** research quality with `--debug`
6. **Iterate** on configuration based on results

Enjoy your enhanced research capabilities! 🔬
