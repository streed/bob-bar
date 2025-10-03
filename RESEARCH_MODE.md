# Research Mode - Publication-Quality Multi-Agent Research System

Research Mode is a sophisticated multi-agent system designed to produce **publication-quality research** with rigorous fact-checking, source verification, and comprehensive documentation.

## Overview

Research Mode transforms complex queries into thoroughly researched, well-sourced documents through a multi-stage process involving specialized agents, multi-round debates, and iterative refinement.

### Key Features

- **🔬 Publication-Quality Output** - Every claim cited with verifiable sources
- **🔍 Multi-Agent Debate** - Advocate, skeptic, and synthesizer validate research quality
- **📚 URL-Based References** - Full URLs tracked for independent verification
- **⚡ Parallel Research** - Multiple specialized workers research concurrently
- **🔄 Iterative Refinement** - Debate → Refine loop until quality standards met
- **📄 Professional Documents** - Structured markdown output with inline citations
- **🎯 Configurable Models** - Separate model for research vs. regular queries

## Architecture

### Research Pipeline

```
User Query
    ↓
┌─────────────────────────────────────────┐
│ 1. Lead Coordinator                     │
│    Decomposes into 5-8 verifiable       │
│    sub-questions                         │
└─────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────┐
│ 2. Parallel Workers (3-6)               │
│    • Web Research Specialist            │
│    • Technical Documentation Analyst    │
│    • Data & Metrics Specialist          │
│    • Comparative Analysis Specialist    │
│    • Current Events Researcher          │
│                                          │
│    Each uses tools to gather cited facts│
└─────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────┐
│ 3. Multi-Round Debate (2+ rounds)       │
│    Round 1:                             │
│      • Advocate defends research        │
│      • Skeptic identifies gaps          │
│    Round 2+:                            │
│      • Advocate rebuts critiques        │
│      • Skeptic verifies responses       │
│    Final:                               │
│      • Synthesizer makes decision       │
└─────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────┐
│ 4. Refinement (if needed)               │
│    Refiner addresses unresolved issues  │
│    using tools to find missing sources  │
└─────────────────────────────────────────┘
    ↓ (repeat debate/refine up to 5x)
    ↓
┌─────────────────────────────────────────┐
│ 5. Document Writing (3 iterations max)  │
│    • Writer creates structured document │
│    • Document Critic reviews quality    │
│    • Iterate until approved             │
└─────────────────────────────────────────┘
    ↓
┌─────────────────────────────────────────┐
│ 6. References Section Added             │
│    • Web Sources (clickable URLs)       │
│    • Additional Sources (documents)     │
└─────────────────────────────────────────┘
    ↓
Final Publication-Quality Document
```

## Configuration

### config.toml

```toml
[ollama]
host = "http://localhost:11434"
model = "llama2"                      # Regular queries
research_model = "llama2:70b"         # Research mode (optional, uses model if not set)
context_window = 128000               # Token window size
max_tool_turns = 5                    # Tool iteration limit

[research]
max_refinement_iterations = 5         # Debate/refine loop iterations
max_document_iterations = 3           # Document writing iterations
max_debate_rounds = 2                 # Back-and-forth debate rounds
worker_count = 3                      # Parallel workers (3-6 recommended)
```

### agents.json

Complete agent configuration with specialized roles:

```json
{
  "agents": {
    "lead": {
      "name": "Lead Research Coordinator",
      "role": "query_decomposer",
      "description": "Decomposes queries into verifiable sub-questions",
      "system_prompt": "...",
      "available_tools": []
    },
    "workers": [
      {
        "name": "Web Research Specialist",
        "role": "web_researcher",
        "description": "Researches factual information from authoritative web sources",
        "system_prompt": "...",
        "available_tools": ["web_search"]
      },
      {
        "name": "Technical Documentation Analyst",
        "role": "technical_analyst",
        "description": "Extracts precise technical specifications",
        "system_prompt": "...",
        "available_tools": ["web_search"]
      },
      {
        "name": "Data & Metrics Specialist",
        "role": "data_specialist",
        "description": "Gathers precise quantitative data",
        "system_prompt": "...",
        "available_tools": ["weather", "web_search"]
      },
      {
        "name": "Comparative Analysis Specialist",
        "role": "comparative_analyst",
        "description": "Conducts rigorous side-by-side comparisons",
        "system_prompt": "...",
        "available_tools": ["web_search"]
      },
      {
        "name": "Current Events Researcher",
        "role": "news_researcher",
        "description": "Researches recent verified events with temporal precision",
        "system_prompt": "...",
        "available_tools": ["web_search"]
      }
    ],
    "debate_agents": [
      {
        "name": "Research Quality Advocate",
        "role": "advocate",
        "description": "Defends research quality through verification",
        "system_prompt": "...",
        "available_tools": ["web_search"]
      },
      {
        "name": "Research Quality Skeptic",
        "role": "skeptic",
        "description": "Identifies factual gaps and verification issues",
        "system_prompt": "...",
        "available_tools": ["web_search"]
      },
      {
        "name": "Research Quality Synthesizer",
        "role": "synthesizer",
        "description": "Makes evidence-based judgment on research quality",
        "system_prompt": "...",
        "available_tools": ["web_search"]
      }
    ],
    "refiner": {
      "name": "Research Refiner",
      "role": "refiner",
      "description": "Addresses factual gaps with verified sources",
      "system_prompt": "...",
      "available_tools": ["web_search", "weather"]
    },
    "writer": {
      "name": "Research Document Writer",
      "role": "writer",
      "description": "Creates fact-checkable documents",
      "system_prompt": "...",
      "available_tools": []
    },
    "document_critic": {
      "name": "Document Quality Critic",
      "role": "document_critic",
      "description": "Ensures publication standards",
      "system_prompt": "...",
      "available_tools": []
    }
  },
  "config": {
    "max_refinement_iterations": 5,
    "max_document_iterations": 3,
    "max_debate_rounds": 2,
    "worker_count": 3,
    "enable_parallel_workers": true
  }
}
```

See `agents.example.json` for complete prompt templates.

## Agent Roles & Responsibilities

### Lead Research Coordinator
- Decomposes complex queries into 5-8 specific, verifiable sub-questions
- Routes questions to specialized workers
- Avoids speculation-inducing questions
- Emphasizes fact-checkable sub-questions

### Worker Specialists

**Web Research Specialist**
- 5-tier source quality hierarchy (primary → general web)
- Cross-verifies claims across multiple sources
- Mandatory URL citations: `[Source: https://example.com]`
- Forbidden: speculation, unsourced claims

**Technical Documentation Analyst**
- Official documentation priority
- Exact version numbers, section citations
- No assumptions about undocumented behavior
- Cross-checks against multiple documentation sources

**Data & Metrics Specialist**
- Exact numbers with units, timeframes, methodology
- Sample sizes and confidence intervals
- Flags self-reported vs. independently verified data
- Cross-checks numbers across sources

**Comparative Analysis Specialist**
- Evidence-based comparisons (no subjective judgments)
- Same metrics for all subjects
- Independent benchmarks preferred over vendor claims
- Sources needed for BOTH sides of comparisons

**Current Events Researcher**
- Exact dates (YYYY-MM-DD)
- Cross-checks breaking news across independent sources
- Flags rumors/speculation explicitly
- Temporal precision

### Debate Agents

**Research Quality Advocate**
- Defends through verification, not rhetoric
- Uses web_search to validate claims during debate
- Only defends verifiable claims
- Acknowledges legitimate gaps

**Research Quality Skeptic**
- Identifies citation gaps, verification failures
- Uses web_search to fact-check questionable claims
- Prioritizes: accuracy > completeness > style
- Specific, evidence-based critiques

**Research Quality Synthesizer**
- Tracks each issue through all debate rounds
- Independent verification with web_search when needed
- Approves only publication-ready research
- "Would you trust this as a factual reference?" test

### Support Agents

**Research Refiner**
- Addresses specific synthesizer requirements
- Uses tools to find authoritative sources
- Cross-checks before adding new information
- Every new fact gets proper citation

**Research Document Writer**
- Preserves EVERY citation from research
- Citations immediately after claims
- Professional, objective tone
- Fact-checkable by following citations

**Document Quality Critic**
- Scans for unsourced claims
- Evaluates citation quality and specificity
- "Would this pass peer review?" test
- Only approves publication-ready documents

## Source Citation System

### Citation Format

**Preferred (URL):**
```markdown
Python 3.12 was released in October 2023 [Source: https://python.org/downloads].
```

**Alternative (Document):**
```markdown
async/await introduced in Python 3.5 [Source: Python 3.5 Documentation, What's New].
```

**With Date:**
```markdown
23°C recorded in Tokyo [Source: https://api.openweathermap.org, 2024-01-15].
```

### References Section

Automatically generated at document end:

```markdown
## References

### Web Sources

The following websites and online resources were consulted:

1. <https://python.org/downloads>
2. <https://python.org/whatsnew/3.5>
3. <https://api.openweathermap.org>

### Additional Sources

Other sources referenced:

4. Python 3.12 Official Documentation
5. TPC-C Benchmark v5.11
```

## Multi-Round Debate System

### How It Works

**Round 1: Initial Positions**
- Advocate presents strongest defenses
- Skeptic presents most significant concerns

**Round 2+: Rebuttals**
- Advocate responds to specific criticisms with evidence
- Skeptic verifies responses and persists on unresolved issues
- Both can use web_search to verify claims

**Final Synthesis:**
- Synthesizer reviews complete debate transcript
- Tracks which concerns were addressed vs. unresolved
- Makes evidence-based approval decision

### Approval Criteria

**APPROVED if:**
- All critical issues resolved with proper sources
- Key claims have authoritative citations
- No factual errors or unsupported assertions
- Research is fact-checkable by independent party

**IMPROVEMENTS NEEDED if:**
- Critical unsourced claims remain
- Factual errors or contradictions unresolved
- Major gaps in core content
- Weak sources for critical claims

## Usage

### Enable Research Mode

Click the `[Research: OFF]` button to toggle to `[Research: ON]`

### Submit Query

Complex queries work best:
```
Compare Python and Rust performance characteristics,
including benchmark data, memory usage patterns, and
real-world production use cases.
```

### Watch Progress

Progress indicators show:
- "Decomposing query into sub-questions..."
- "Dispatching N research workers..."
- "✓ Worker completed"
- "Debate round X/Y in progress..."
- "Refining output (iteration X/Y)"
- "Writing document (iteration X/Y)"

### Review Output

Final document includes:
- Executive Summary
- Well-organized main content with inline citations
- Key Findings
- References section with clickable URLs

## Quality Standards

Every agent follows strict standards:

✅ **Primary sources over secondary**
✅ **Cross-verification of key claims**
✅ **Mandatory URL citations when available**
✅ **No speculation or unsourced assertions**
✅ **Specific version numbers and dates**
✅ **Independent verifiability**

## Performance Tuning

### Worker Count
- **3 workers**: Balanced (default)
- **6 workers**: Faster, more comprehensive
- **More**: Diminishing returns, potential rate limiting

### Debate Rounds
- **1 round**: Faster, less rigorous
- **2 rounds**: Balanced (default)
- **3+ rounds**: More thorough verification

### Context Window
- **128k**: Standard models (default)
- **256k+**: Advanced models (kimi, etc.)
- Larger windows = better context retention

### Research Model
Set a larger/more capable model for research:
```toml
research_model = "llama2:70b"  # Or gpt-oss:120b-cloud, kimi-k2:1t-cloud
```

## Debugging

Enable debug mode:
```bash
bob-bar --debug
```

Debug output shows:
- Query decomposition with sub-questions
- Worker assignments and tool calls
- Debate transcripts (all rounds)
- Synthesizer decisions
- Refinement iterations
- Source extraction counts
- Citation tracking

## Best Practices

### For Users

1. **Ask Comprehensive Questions** - Research mode excels at multi-faceted queries
2. **Enable for Complex Topics** - Use for topics requiring verification
3. **Check References** - Click URLs to verify claims independently
4. **Use Appropriate Models** - Larger models produce better research

### For Configuration

1. **Match Workers to Query Types** - Adjust worker_count based on complexity
2. **Set Appropriate Iterations** - More iterations = higher quality but slower
3. **Configure Research Model** - Use most capable model you have available
4. **Tune Debate Rounds** - Balance thoroughness vs. speed

## Example Workflow

```
Query: "What are the performance differences between PostgreSQL and MySQL?"

1. Lead Coordinator decomposes:
   - "What are PostgreSQL's documented performance benchmarks?"
   - "What are MySQL's documented performance benchmarks?"
   - "How do independent benchmarks compare PostgreSQL vs MySQL?"
   - "What are the memory usage patterns of PostgreSQL?"
   - "What are the memory usage patterns of MySQL?"
   - "What production use cases exist for each?"

2. Workers research (parallel):
   - Technical Analyst → PostgreSQL docs (with URLs)
   - Technical Analyst → MySQL docs (with URLs)
   - Comparative Analyst → TPC benchmarks (with URLs)
   - Data Specialist → Memory metrics (with URLs)
   - Web Researcher → Production case studies (with URLs)

3. Debate Round 1:
   - Advocate: "Research has comprehensive benchmark data with proper citations"
   - Skeptic: "Missing citation for PostgreSQL memory claim on line 42"

4. Debate Round 2:
   - Advocate: "Added source: https://postgresql.org/docs/15/runtime-config"
   - Skeptic: "Concern resolved. All claims now sourced."

5. Synthesizer: "APPROVED - All critical concerns addressed"

6. Document Writer creates structured markdown with inline citations

7. References section added with all URLs

8. Final output delivered
```

## Troubleshooting

### "No sources found in document"
- Agents may not be using proper `[Source: ...]` format
- Check agent prompts emphasize URL citations
- Enable debug mode to see raw agent outputs

### "Maximum iterations reached"
- Increase `max_refinement_iterations` if needed
- Check if synthesizer is being too strict
- Review debate transcripts in debug mode

### "Unknown tool" errors
- Verify tools.json has required tools
- Check agent available_tools lists match tool names
- See tools.example.json for reference

### Poor quality output
- Use larger/more capable research model
- Increase debate rounds for more verification
- Check worker prompts emphasize fact-checking

## Advanced: Custom Agents

You can customize agent behavior by editing `~/.config/bob-bar/agents.json`:

1. **Modify System Prompts** - Change how agents approach tasks
2. **Adjust Available Tools** - Control which tools each agent can use
3. **Add New Workers** - Create specialized workers for specific domains
4. **Tune Debate Agents** - Adjust rigor vs. speed trade-offs

See `agents.example.json` for complete prompt templates and guidelines.

---

**Research Mode transforms bob-bar into a publication-quality research assistant capable of producing thoroughly sourced, fact-checked documents that meet academic and professional standards.**
