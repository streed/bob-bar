# Research Mode - Quick Reference Card

## What is Research Mode?

An orchestrator-worker architecture that:
- **Decomposes** complex queries into sub-questions (Lead Agent)
- **Researches** in parallel with specialized workers (2-3 concurrent agents)
- **Refines** outputs through critic-refiner loop (up to 5 iterations)
- **Cites** sources automatically with bibliography generation

## How to Use

### 1. Enable Research Mode
Click `[Research: OFF]` button → turns to `[Research: ON]`

### 2. Ask Complex Questions
Good candidates:
- Multi-faceted: *"Weather in Tokyo + GitHub trends"*
- Comparative: *"Python vs Rust for web development"*
- Research-heavy: *"Latest developments in AI and their applications"*

### 3. Review Output
- Worker findings with inline citations `[Source: name]`
- Refinement iterations visible in debug mode
- Professional bibliography at end

## Configuration

### Quick Tuning

Edit `~/.config/bob-bar/config.toml`:

```toml
[research]
max_refinement_iterations = 5  # Quality iterations
worker_count = 3              # Parallel workers
```

### Presets

**High Quality (Slower):**
```toml
max_refinement_iterations = 10
worker_count = 4
```

**Fast (Lower Quality):**
```toml
max_refinement_iterations = 2
worker_count = 2
```

**Balanced (Default):**
```toml
max_refinement_iterations = 5
worker_count = 3
```

## Agent Roles

### Lead Agent
- Breaks queries into 2-3 sub-questions
- Assigns to appropriate workers
- No tools needed

### Workers (3 Types)

**Web Research Specialist**
- Tools: search, web_fetch
- Focus: Online sources, recent information
- Citations: `[Source: website.com]`

**Data Analyst**
- Tools: weather, web_fetch
- Focus: APIs, structured data, metrics
- Citations: `[Source: API Name]`

**General Researcher**
- Tools: search, weather, web_fetch
- Focus: Comprehensive research
- Citations: `[Source: any source]`

### Critic Agent
- Evaluates 7 criteria (Completeness, Accuracy, Sources, Clarity, Depth, Relevance, Consistency)
- Provides specific criticisms
- Only approves excellent outputs

### Refiner Agent
- Addresses ALL critic feedback
- Can use tools for additional research
- Makes substantial improvements

## Citation Format

All agents use:
```
[Source: name or URL]
```

Examples:
- `Python is popular [Source: python.org]`
- `Temperature: 18°C [Source: Weather API]`
- `Study shows [Source: Nature 2024]`

## Output Structure

```markdown
# Research Results for: [Your Query]

## Web Research Specialist
**Question:** [Sub-question 1]

[Answer with sources] [Source: example.com]

## Data Analyst
**Question:** [Sub-question 2]

[Answer with data] [Source: API Name]

---

## Bibliography

1. API Name
2. example.com
3. Nature 2024
```

## Debug Mode

See what's happening:
```bash
cargo run --release -- --debug
```

Shows:
- Config values loaded
- Iteration progress
- Critic feedback
- Approval status

## Common Issues

### Critic Too Lenient
**Symptom:** Always approves immediately
**Fix:** Already fixed with rigorous prompts
**Verify:** Should see 2-4 issues on first iteration

### No Bibliography
**Symptom:** Missing bibliography section
**Cause:** No citations in output
**Fix:** Agents should cite with `[Source: name]`

### Hitting Max Iterations
**Symptom:** Always reaches limit
**Cause:** Critic too strict or unclear
**Fix:** Increase `max_refinement_iterations`

### Slow Performance
**Cause:** Too many workers or iterations
**Fix:** Reduce both in config.toml

## Tips

### For Best Results
1. Ask questions that benefit from multiple perspectives
2. Use research mode for important queries
3. Check bibliography for source quality
4. Adjust iterations based on importance

### When NOT to Use
- Simple factual lookups
- Single-tool queries
- Time-sensitive questions
- Already have the answer

### Performance
- **Simple:** 8-15 seconds
- **Medium:** 15-30 seconds
- **Complex:** 30-60 seconds

Research mode is slower but more thorough.

## Key Features

✅ Parallel worker execution (Tokio channels)
✅ Quality assurance (Critic-refiner loop)
✅ Source tracking (Automatic bibliography)
✅ Configurable behavior (config.toml)
✅ Specialized agents (Domain expertise)
✅ Rigorous evaluation (7 criteria)
✅ Professional output (Academic formatting)

## Files to Know

- `~/.config/bob-bar/config.toml` - Main configuration
- `~/.config/bob-bar/agents.json` - Agent prompts and roles
- `~/.config/bob-bar/tools.json` - Available tools

## Example Session

```
User: "What's the weather in Tokyo and who are the top Rust contributors?"

[Research: ON] button clicked

Lead Agent decomposes:
1. "What is the current weather in Tokyo?"
2. "Who are the top contributors to the Rust language?"

Worker 1 (Data Analyst):
- Calls weather API
- Returns: "Tokyo: 18°C, Partly cloudy [Source: Weather API]"

Worker 2 (Web Research Specialist):
- Searches GitHub/web
- Returns: "Top contributors include... [Source: GitHub]"

Critic reviews:
- "ISSUE: Weather lacks context (humidity, forecast)
   IMPROVEMENT NEEDED: Add more weather details"

Refiner improves:
- Calls weather API again
- Adds humidity, forecast
- Updates output

Critic reviews again:
- "APPROVED"

Final output with bibliography:
---
## Bibliography
1. GitHub
2. Weather API
---
```

## Need Help?

Run with debug flag to see what's happening:
```bash
cargo run --release -- --debug
```

Check documentation:
- `RESEARCH_MODE.md` - Full feature guide
- `AGENT_PROMPT_REFINEMENTS.md` - Agent details
- `BIBLIOGRAPHY_AND_CONFIG_UPDATES.md` - Bibliography system
- `FINAL_IMPROVEMENTS_SUMMARY.md` - Complete changes

---

*Research mode: Higher quality through multi-agent collaboration and rigorous review.*
