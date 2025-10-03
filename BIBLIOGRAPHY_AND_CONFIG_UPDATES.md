# Bibliography and Configuration Updates

## Summary of Changes

### 1. Bibliography System

Added automatic bibliography generation that extracts sources from research outputs and presents them in a dedicated section.

#### Features

**Automatic Source Detection:**
- Parses `[Source: name]` citations
- Extracts markdown link URLs `[text](url)`
- Detects API/tool mentions (Weather API, GitHub API, etc.)
- Removes duplicates and sorts alphabetically
- Appends formatted bibliography to final output

**Citation Format for Agents:**
All agent prompts now include specific citation instructions:
```
[Source: website.com] or [Source: Full Source Name]
```

**Example Output:**
```markdown
# Research Results

... research content with citations [Source: example.com] ...

---

## Bibliography

1. example.com
2. GitHub API
3. Nature 2024
4. Weather API
```

#### Implementation

**File:** `src/research.rs`
- New method: `add_bibliography()` (lines 252-315)
- Uses regex to extract sources from multiple citation formats
- Automatically called after refinement loop
- Creates sorted, numbered bibliography

**Dependencies Added:**
- `regex = "1.10"` in Cargo.toml

**Pattern Matching:**
1. `[Source: url]` or `[Source: name]` - inline citations
2. `(Source: url)` - parenthetical citations
3. `[text](http://url)` - markdown links
4. API/tool mentions like "weather API", "GitHub API"

### 2. Configuration System Integration

Research mode settings now configurable via `config.toml` in addition to `agents.json`.

#### New Config Section

**`config.toml`:**
```toml
[research]
# Maximum number of refinement iterations
max_refinement_iterations = 5

# Number of parallel worker agents
worker_count = 3
```

#### Priority System

1. **agents.json** - Default values for agent behavior
2. **config.toml** - Override research parameters globally
3. Config.toml values take precedence over agents.json

#### Implementation

**File:** `src/config.rs`
- New `ResearchConfig` struct (lines 40-45)
- Added to main Config struct
- Default implementations
- Serde deserialization with defaults

**File:** `src/research.rs`
- New method: `override_config()` (lines 84-88)
- Merges toml config with json config
- Called during orchestrator initialization

**File:** `src/main.rs`
- Calls `override_config()` when initializing research orchestrator
- Displays config values in debug mode

### 3. Agent Prompt Updates

All agents updated with explicit citation format instructions.

#### Web Research Specialist

**Added:**
```
Citation Format (IMPORTANT):
- Use inline citations in this exact format: [Source: website.com]
- Examples:
  * "Python is widely used [Source: python.org]"
- Place citations immediately after claims
- All sources will be automatically collected into a bibliography
```

#### Data Analyst

**Added:**
```
Citation Format for Data Sources (IMPORTANT):
- Cite each data point: [Source: API Name] or [Source: Data Provider]
- Examples:
  * "Temperature: 18°C [Source: Weather API]"
- All sources will be collected into a bibliography automatically
```

#### General Researcher

**Added:**
```
Citation Requirements (CRITICAL):
- Use this exact format: [Source: source name or URL]
- Cite ALL factual claims, data points, and quotes
- Citations will be automatically compiled into a bibliography
```

#### Refiner Agent

**Added:**
```
Citation Format (MANDATORY):
- When adding sources, use: [Source: source name or URL]
- Add citations for ALL new facts and data points
- Sources will be collected into a bibliography automatically
```

## Usage

### Configuring Research Mode

**Option 1: Via config.toml (Recommended)**
```toml
[research]
max_refinement_iterations = 7  # Increase for higher quality
worker_count = 2              # Decrease for faster results
```

**Option 2: Via agents.json**
```json
{
  "config": {
    "max_refinement_iterations": 5,
    "worker_count": 3,
    "enable_parallel_workers": true
  }
}
```

**Precedence:** config.toml overrides agents.json

### Citation Guidelines for Users

When using research mode, outputs will automatically include citations:

**During Research:**
- Agents add inline citations using `[Source: name]` format
- Citations appear immediately after claims
- Multiple sources can be cited for the same claim

**In Final Output:**
- All unique sources collected
- Sorted alphabetically
- Numbered in bibliography section
- Separated from main content with horizontal rule

### Testing Bibliography

**Test Query:**
```
What is Python and who created it?
```

**Expected Output:**
```markdown
# Research Results for: What is Python and who created it?

## Web Research Specialist
Python is a high-level programming language [Source: python.org].
Created by Guido van Rossum in 1991 [Source: Wikipedia].

---

## Bibliography

1. python.org
2. Wikipedia
```

## Configuration Examples

### High-Quality Research (Slower)
```toml
[research]
max_refinement_iterations = 10
worker_count = 4
```

### Fast Research (Lower Quality)
```toml
[research]
max_refinement_iterations = 2
worker_count = 2
```

### Balanced (Default)
```toml
[research]
max_refinement_iterations = 5
worker_count = 3
```

## Files Modified

1. **src/research.rs**
   - Added `use std::collections::HashSet`
   - Added `add_bibliography()` method
   - Added `override_config()` method
   - Updated `research()` to call `add_bibliography()`

2. **src/config.rs**
   - Added `ResearchConfig` struct
   - Added to main `Config` struct
   - Added default implementations
   - Added helper functions for defaults

3. **src/main.rs**
   - Call `override_config()` during init
   - Display research config in debug mode

4. **Cargo.toml**
   - Added `regex = "1.10"`

5. **~/.config/bob-bar/agents.json**
   - Updated all agent prompts with citation format
   - Explicit bibliography instructions

6. **config.example.toml**
   - Added [research] section
   - Documented all options

## Debug Output

With `--debug` flag:
```
=== Research Mode ===
Research orchestrator initialized from: /home/user/.config/bob-bar/agents.json
Max refinement iterations: 5
Worker count: 3
=====================
```

## Benefits

### For Users
- ✅ Easy to verify information sources
- ✅ Professional-looking research output
- ✅ Transparent about data origins
- ✅ Can check source credibility
- ✅ Academic-style citation support

### For System
- ✅ Encourages agents to cite sources
- ✅ Makes critic more effective (can check for missing sources)
- ✅ Automatic deduplication of sources
- ✅ Configurable behavior without code changes
- ✅ Clean separation of concerns

## Best Practices

### For Citation Quality

1. **Be Specific:** `[Source: python.org/docs]` better than `[Source: documentation]`
2. **Cite Data:** All numbers and metrics should have sources
3. **Cite Claims:** Factual statements need attribution
4. **Avoid Generic:** Don't cite "the internet" or "search"

### For Configuration

1. **Start with defaults** (5 iterations, 3 workers)
2. **Increase iterations** for critical research
3. **Reduce iterations** for quick lookups
4. **Match workers to query** complexity

### For Agents

1. **Cite inline** immediately after claims
2. **Use consistent format** `[Source: name]`
3. **Include tool sources** when using APIs
4. **Multiple sources** for important claims

## Troubleshooting

### Bibliography Not Appearing

**Cause:** No citations in output
**Fix:** Check agent prompts include citation format
**Verify:** Agents are actually calling tools

### Duplicate Sources

**System:** Automatically deduplicates
**Note:** `python.org` and `Python.org` treated as different

### Sources Not Extracted

**Cause:** Wrong citation format
**Fix:** Use exact format `[Source: name]`
**Check:** Regex patterns in `add_bibliography()`

### Config Not Applied

**Cause:** Config.toml syntax error
**Fix:** Validate TOML syntax
**Debug:** Run with `--debug` flag to see loaded values

## Future Enhancements

Potential improvements:

1. **Numbered Citations:** Like academic papers `[1]`, `[2]`
2. **Footnotes:** Place bibliography as footnotes
3. **Citation Validation:** Check if URLs are valid
4. **Citation Grouping:** Group by source type (web, API, paper)
5. **Export Bibliography:** Separate file in BibTeX format
6. **Citation Links:** Make bibliography entries clickable
