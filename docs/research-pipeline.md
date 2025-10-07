# Research Pipeline

This document provides a detailed walkthrough of bob-bar's multi-agent research pipeline, from query submission to final document.

## Pipeline Overview

```
┌──────────────────┐
│  User Query      │ "Compare Python vs Rust performance"
└────────┬─────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 1: PLANNING (Iterative with Plan Critic)              │
├─────────────────────────────────────────────────────────────┤
│ 1. Clear previous memories from database                     │
│ 2. Lead coordinator decomposes query into sub-questions     │
│ 3. Plan critic reviews for coverage, quality, assignments   │
│ 4. If approved: proceed. If not: refine and review again    │
│ 5. Repeat up to max_plan_iterations (default: 3)           │
└────────┬────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 2: EXECUTION (Parallel Workers + Supervisor)          │
├─────────────────────────────────────────────────────────────┤
│ 1. Store approved plan in shared memory                     │
│ 2. Spawn N workers in parallel (3-10 based on complexity)  │
│ 3. Spawn supervisor in parallel (monitors every 15s)        │
│ 4. Workers execute with tools, store discoveries           │
│ 5. Supervisor provides feedback, identifies gaps            │
│ 6. Collect results as workers complete                      │
└────────┬────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 3: SYNTHESIS (Combination + Debate)                   │
├─────────────────────────────────────────────────────────────┤
│ 1. Combine worker results into single output                │
│ 2. Debate: Advocate argues strengths                        │
│ 3. Debate: Skeptic challenges weaknesses                    │
│ 4. Debate: Synthesizer makes final assessment               │
│ 5. Repeat debate for max_debate_rounds (default: 2)        │
└────────┬────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 4: REFINEMENT (Gap Filling)                           │
├─────────────────────────────────────────────────────────────┤
│ 1. Refiner agent receives debate conclusions                │
│ 2. Checks memory for existing answers first                 │
│ 3. Fills gaps with additional research if needed            │
│ 4. Iterates up to max_refinement_iterations (default: 5)   │
└────────┬────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 5: DOCUMENTATION (Iterative with Document Critic)     │
├─────────────────────────────────────────────────────────────┤
│ 1. Writer synthesizes research into comprehensive document  │
│ 2. Document critic reviews for depth, sources, clarity      │
│ 3. If approved: proceed. If not: writer revises             │
│ 4. Repeat up to max_document_iterations (default: 3)       │
│ 5. Add references section with all source URLs              │
└────────┬────────────────────────────────────────────────────┘
         │
         ▼
┌──────────────────┐
│  Final Document  │ 1500-3000+ word comprehensive report
└──────────────────┘
```

## Phase 1: Planning

### Step 1.1: Clear Memories

**Location**: `src/research.rs:223-231`

```rust
// Clear previous memories from database to start fresh
if let Some(ref shared_memory) = self.shared_memory {
    shared_memory.clear().await?;
}
```

**Why**: Each research run should start with a clean slate. Previous research shouldn't pollute the current query's memory space.

**Database Operations**:
```sql
DELETE FROM memories;
DELETE FROM vec_memories;
```

### Step 1.2: Initial Plan Generation

**Agent**: Lead Research Coordinator
**Location**: `src/research.rs:440-468`

**Input**:
```
System Prompt: Lead coordinator instructions
Worker Count Guidance: 3-10 workers based on complexity
Query: "Compare Python vs Rust performance"
```

**Output** (JSON + strategy):
```json
[
  {"question": "What are Python's documented performance benchmarks?", "worker": "technical_analyst"},
  {"question": "What are Rust's documented performance benchmarks?", "worker": "technical_analyst"},
  {"question": "What performance metrics differ between Python and Rust?", "worker": "data_specialist"},
  {"question": "What are verified case studies of Python performance?", "worker": "web_researcher"},
  {"question": "What are verified case studies of Rust performance?", "worker": "web_researcher"},
  {"question": "How do Python and Rust compare in benchmarks?", "worker": "comparative_analyst"}
]

Strategy: Focus on independent benchmarks from TPC, SPEC, and academic sources.
Prioritize quantitative data over anecdotes. Cross-verify vendor claims.
```

**Agent Behavior**:
- Analyzes query complexity
- Creates 3-10 sub-questions based on complexity
- Assigns each question to most appropriate specialist
- Provides research strategy explaining approach

### Step 1.3: Plan Critic Review

**Agent**: Research Plan Critic
**Location**: `src/research.rs:501-522`

**Input**:
```
Original Query: "Compare Python vs Rust performance"
Research Plan: [JSON questions + strategy from step 1.2]
```

**Evaluation Criteria**:
1. **Coverage**: Does plan cover all major aspects?
2. **Question Quality**: Are questions specific and verifiable?
3. **Worker Assignment**: Right specialist for each question?
4. **Efficiency**: No redundancy, appropriate question count?
5. **Strategy**: Clear and actionable approach?

**Output** (two possibilities):

**Approved**:
```
APPROVED

Plan provides comprehensive coverage with well-structured questions.
Benchmark focus is appropriate. Worker assignments are optimal.
```

**Needs Improvement**:
```
IMPROVEMENTS NEEDED

Critical Issues:
1. Missing aspect: No coverage of memory usage comparison
2. Question quality: "Python performance" too vague - specify context

Suggested Changes:
- ADD: "What are memory usage patterns for Python vs Rust?" assigned to data_specialist
- MODIFY: Change "Python performance" to "Python execution speed in CPU-bound tasks"
- REASSIGN: Question 3 should go to comparative_analyst instead of web_researcher

Strategy Improvements:
Specify benchmark versions and test conditions for comparability.
```

### Step 1.4: Plan Refinement

**Agent**: Lead Research Coordinator (again)
**Location**: `src/research.rs:471-498`

**Input**:
```
Original Query: "Compare Python vs Rust performance"
Previous Plan: [JSON + strategy]
Critic Feedback: [Improvements needed from step 1.3]
```

**Output**: Revised JSON + strategy addressing feedback

**Iteration**:
- Repeats steps 1.2-1.4 up to `max_plan_iterations` (default: 3)
- Exits early if plan is APPROVED
- Uses final plan even if not approved after max iterations

### Step 1.5: Plan Storage

**Location**: `src/research.rs:239-247`

```rust
shared_memory.store_memory(
    MemoryType::Plan,
    plan.clone(),
    "lead_coordinator".to_string(),
    Some(metadata)
).await?;
```

**Database**:
```sql
INSERT INTO memories (query_id, memory_type, content, created_by, metadata, created_at)
VALUES ('query_123_456', 'plan', 'Strategy: Focus on...', 'lead_coordinator', '{"query_id":"..."}', 1234567890);
```

**Purpose**: Workers can retrieve the plan to understand research scope and strategy.

## Phase 2: Execution

### Step 2.1: Worker Spawning

**Location**: `src/research.rs:263-326`

**Process**:
```rust
// For each sub-question, spawn a worker
for sub_question in sub_questions {
    let handle = tokio::spawn(async move {
        execute_worker(sub_question).await
    });
    handles.push(handle);
}

// Also spawn supervisor
let supervisor_handle = tokio::spawn(async move {
    supervise_workers().await
});
```

**Example**: For 6 sub-questions, spawns:
- 6 worker tasks (parallel execution)
- 1 supervisor task (monitoring)
- All run concurrently

### Step 2.2: Worker Execution

**Location**: `src/research.rs:1245-1393`

Each worker receives:

**Context** (automatically assembled):
```
========== RESEARCH CONTEXT ==========

📋 PLAN (your assigned scope):
Question: What are Python's documented performance benchmarks?
Strategy: Focus on independent benchmarks...

👁️ SUPERVISOR FEEDBACK:
Use official Python.org sources and PyPerformance suite.
Ensure benchmarks specify Python version.

🔍 RELEVANT DISCOVERIES (from other agents):
(none yet - first worker to execute)

⚠️ DEADENDS TO AVOID:
(none yet)

========== END CONTEXT ==========
```

**Worker Prompt**:
```
[System Prompt: Technical Documentation Analyst]

MANDATORY WORKFLOW - FOLLOW EXACTLY:
1. Call research tool
2. IMMEDIATELY: memory_store(type="discovery", content="Fact [Source: Name](URL)", agent="your_role")
3. Call another research tool
4. IMMEDIATELY: memory_store(...)
5. Repeat until you have 3-5 discoveries
6. Write final comprehensive answer

[Context from above]

Question: What are Python's documented performance benchmarks?
```

**Worker Execution Flow**:
```
1. Worker calls web_search("Python official benchmarks")
   → Returns: [{url, title, snippet}, ...]

2. Worker calls memory_store(
       type="discovery",
       content="PyPerformance is the official suite [Source: Python.org](https://...)",
       agent="technical_analyst"
   )
   → Stored in database

3. Worker calls web_search("PyPerformance results")
   → Returns more results

4. Worker calls memory_store(
       type="discovery",
       content="Python 3.12 is 20% faster [Source: PyPerformance](https://...)",
       agent="technical_analyst"
   )
   → Stored in database

5. Worker calls semantic_scholar("Python performance optimization")
   → Returns academic papers

6. Worker calls memory_store(
       type="discovery",
       content="Optimization techniques study [Source: ACM](https://...)",
       agent="technical_analyst"
   )
   → Stored in database

7. Worker synthesizes final answer:
   "Python's official performance benchmarking suite is PyPerformance [Source](url1).
    Python 3.12 shows 20% speed improvement [Source](url2).
    Academic research shows optimization patterns [Source](url3)..."

8. Return final answer (800-1500 words with inline citations)
```

### Step 2.3: Supervisor Monitoring

**Agent**: Research Supervisor
**Location**: `src/research.rs:926-1086`

**Execution**:
```rust
loop {
    sleep(15 seconds);

    // Check how many workers completed
    let completed = check_completed_count();

    if completed >= total_workers {
        break; // All done
    }

    // Analyze progress
    let discoveries = memory_get_discoveries();
    let deadends = memory_get_deadends();
    let plan = memory_get_plan();

    // Generate feedback
    let feedback = analyze_and_provide_feedback(discoveries, deadends, plan);

    // Update feedback in memory (replaces previous feedback)
    memory.update_or_store_memory(
        MemoryType::Feedback,
        feedback,
        "supervisor",
        metadata
    );
}
```

**Supervisor Analysis**:
```
Looking at:
- How many discoveries stored so far?
- Are workers following the plan?
- Any deadends suggesting wrong approach?
- Gaps in coverage?
- Quality of sources being cited?

Feedback example:
"Good progress on Python benchmarks. Need more Rust data.
Worker 3: Expand on memory usage comparison with specific numbers.
Worker 5: Verify benchmark versions - cite specific PyPerformance release.
Focus on quantitative metrics, not general statements."
```

**Feedback Storage**:
```sql
-- First supervisor update
INSERT INTO memories VALUES (..., 'feedback', 'Good progress...', 'supervisor', ...);

-- 15 seconds later - UPDATES existing row (not new insert)
UPDATE memories
SET content = 'Updated feedback...',
    created_at = <new_timestamp>
WHERE memory_type = 'feedback'
  AND created_by = 'supervisor'
  AND query_id = 'query_123';
```

**Why update instead of insert**: Keeps only latest feedback, prevents memory table from filling with supervisor updates.

### Step 2.4: Worker Context Updates

**Location**: `src/research.rs:1280-1333`

Every time supervisor updates feedback, workers executing later see:

```
👁️ SUPERVISOR FEEDBACK:
Good progress on Python benchmarks. Need more Rust data.
Worker 3: Expand on memory usage comparison with specific numbers...

🔍 RELEVANT DISCOVERIES:
- PyPerformance is the official suite [Source: Python.org](https://...)
- Python 3.12 is 20% faster [Source: PyPerformance](https://...)
- Rust shows 50x faster execution [Source: Benchmarks.rs](https://...)
```

Workers can see what others found and supervisor's guidance in real-time.

### Step 2.5: Result Collection

**Location**: `src/research.rs:326-344`

```rust
// Wait for all workers to complete
let results = join_all(worker_handles).await;

// Filter out failures (workers that errored)
let successful_results: Vec<WorkerResult> = results
    .into_iter()
    .filter_map(|r| r.ok())
    .collect();

// Stop supervisor
supervisor_cancel.cancel();
```

**WorkerResult**:
```rust
struct WorkerResult {
    worker_name: String,         // "Technical Documentation Analyst"
    question: String,            // "What are Python's benchmarks?"
    answer: String,              // 800-1500 word response with citations
}
```

## Phase 3: Synthesis

### Step 3.1: Combine Results

**Location**: `src/research.rs:1448-1470`

```rust
let mut output = format!("# Research Results for: {}\n\n", original_query);

for result in results {
    // Summarize if too long for context
    let answer = self.summarize_worker_result(result, num_workers).await?;

    output.push_str(&format!(
        "## {}\n**Question:** {}\n\n{}\n\n",
        result.worker_name,
        result.question,
        answer
    ));
}
```

**Combined Output Example**:
```markdown
# Research Results for: Compare Python vs Rust performance

## Technical Documentation Analyst
**Question:** What are Python's documented performance benchmarks?

PyPerformance is Python's official benchmarking suite [Source](url).
Python 3.12 shows 20% improvement over 3.11 [Source](url).
Benchmarks include: richards, pystone, nbody... [Source](url).

## Technical Documentation Analyst
**Question:** What are Rust's documented performance benchmarks?

Rust benchmarks tracked via Rust Performance [Source](url).
Shows 50x faster than Python in CPU-bound tasks [Source](url).
...

## Data & Metrics Specialist
**Question:** What performance metrics differ?

Execution speed: Rust 10-100x faster [Source](url).
Memory usage: Rust uses 50% less RAM [Source](url).
...

(continues for all 6 workers)
```

### Step 3.2: Debate Round 1 - Advocate

**Agent**: Research Quality Advocate
**Location**: `src/research.rs:1591-1675`

**Input**:
```
Research Output: [Combined results from step 3.1]
```

**Advocate's Role**: Support the findings, identify strengths

**Output Example**:
```
STRENGTHS:

1. Comprehensive Source Coverage: All claims backed by authoritative sources
   - Official benchmarks from Python.org and Rust Performance
   - Academic sources from ACM and IEEE
   - Independent benchmarks from third parties

2. Quantitative Data: Specific numbers provided
   - "20% faster" - Python 3.12 vs 3.11
   - "50x faster" - Rust vs Python CPU-bound
   - "50% less memory" - Rust memory usage

3. Methodological Rigor: Benchmarks properly cited
   - PyPerformance version specified
   - Test conditions documented
   - Reproducible results

4. Balanced Coverage: Both languages examined equally
   - 2 workers on Python, 2 on Rust, 2 on comparison
   - No obvious bias

OVERALL: Strong foundation for comprehensive answer. Factual accuracy high.
```

### Step 3.3: Debate Round 1 - Skeptic

**Agent**: Research Verification Skeptic
**Location**: `src/research.rs:1677-1747`

**Input**:
```
Research Output: [Combined results]
Advocate's Argument: [Strengths from step 3.2]
```

**Skeptic's Role**: Challenge weaknesses, identify gaps

**Output Example**:
```
CONCERNS:

1. Incomplete Context:
   - "50x faster" lacks context: What workload? Which Python version?
   - Need to specify: CPU-bound vs I/O-bound vs memory-bound

2. Missing Critical Comparisons:
   - No discussion of compilation time (Rust slower to compile)
   - No coverage of development speed/productivity
   - Missing real-world application performance (not just microbenchmarks)

3. Source Quality Issues:
   - Worker 3: Used general "benchmarks.rs" instead of official source
   - Need primary sources for "50% less memory" claim

4. Potential Misrepresentation:
   - Comparing optimized Rust vs unoptimized Python?
   - Are benchmark conditions equivalent?

VERDICT: Data exists but needs qualification and additional context.
Recommendation: Refine to add context and verify contested claims.
```

### Step 3.4: Debate Round 1 - Synthesizer

**Agent**: Debate Synthesizer
**Location**: `src/research.rs:1749-1779`

**Input**:
```
Research Output: [Combined results]
Debate Transcript:
  Advocate: [Strengths]
  Skeptic: [Concerns]
```

**Synthesizer's Role**: Final verdict, actionable recommendations

**Output Example**:
```
ASSESSMENT:

The research provides a solid quantitative foundation but requires refinement:

ACCEPT:
- Comprehensive source coverage (Python.org, academic papers)
- Quantitative benchmarks with citations
- Balanced worker allocation

CONCERNS REQUIRING ATTENTION:
1. Add context to performance claims
   - Specify workload types (CPU/IO/memory-bound)
   - Note Python version and optimization level
   - Clarify benchmark conditions

2. Address missing comparisons
   - Compilation time differences
   - Development productivity tradeoffs
   - Real-world application performance vs microbenchmarks

3. Verify "50% less memory" claim
   - Find primary source
   - Specify measurement methodology

RECOMMENDATION: REFINE with focus on contextualizing claims and filling gaps.
```

### Step 3.5: Debate Round 2 (Optional)

**Configuration**: `max_debate_rounds = 2` (default)

If multiple rounds configured:
- Advocate argues based on synthesizer feedback
- Skeptic challenges based on what was addressed
- Synthesizer makes final call

After max rounds, use final synthesizer decision.

## Phase 4: Refinement

### Step 4.1: Refiner Execution

**Agent**: Research Refiner
**Location**: `src/research.rs:1899-1924`

**Input**:
```
Original Output: [Combined worker results]
Debate Conclusions: [Synthesizer's assessment]
```

**Refiner's Process**:

```
1. Check Memory First (before new research):
   - memory_get_plan() → understand scope
   - memory_get_feedback() → see supervisor notes
   - memory_get_discoveries() → workers may have already found it
   - memory_search("compilation time") → semantic search
   - memory_get_deadends() → avoid failed approaches

2. Analyze Gaps:
   Gap: "Add context to performance claims - specify workload"
   Memory check: memory_search("workload types")
   Result: Worker 3 mentioned "CPU-bound" but not stored as discovery
   Action: Extract and emphasize, no new research needed

3. Fill Remaining Gaps:
   Gap: "Verify 50% less memory claim - find primary source"
   Memory check: memory_search("memory usage")
   Result: Not in discoveries
   Action: web_search("Rust memory usage benchmarks primary source")
          → Find authoritative source
          → memory_store(type="discovery", ...)

4. Generate Refined Output:
   Original claim: "Rust is 50x faster"
   Refined claim: "Rust executes CPU-bound tasks 50x faster than Python in
                   PyPerformance benchmarks [Source](url), though Python excels
                   in I/O-bound operations where async performance is similar [Source](url)"
```

**Refiner Output**:
```markdown
# Refined Research: Compare Python vs Rust Performance

## Executive Summary

Python and Rust have distinct performance characteristics suited to different use cases.

**CPU-Bound Tasks**: Rust executes 10-100x faster [Source: Benchmarks.rs](url).
PyPerformance suite shows Rust completing richards benchmark in 0.05s vs Python's 5s [Source](url).

**Memory Usage**: Rust applications typically use 40-60% less memory [Source: SPEC](url),
measured across identical workloads with equivalent functionality.

**Compilation vs Interpretation**: Rust requires compilation (1-60s depending on project size) [Source](url),
while Python executes immediately. This affects development iteration speed.

**Real-World Performance**: Discord migrated from Python to Rust, seeing 10x throughput
improvement and 50% memory reduction [Source: Discord Engineering](url).
However, Python remains dominant for ML/AI due to NumPy/PyTorch optimizations [Source](url).

...continues with refined, contextualized content...
```

**Iteration**:
- Can iterate up to `max_refinement_iterations` (default: 5)
- Each iteration refines based on new research
- Typically completes in 1-2 iterations

## Phase 5: Documentation

### Step 5.1: Initial Document Draft

**Agent**: Research Document Writer
**Location**: `src/research.rs:1839-1873`

**Input**:
```
Original Query: "Compare Python vs Rust performance"
Research Content: [Refined output from Phase 4]
```

**Writer's Task**:

```
Transform research findings into publication-quality document:

1. Structure:
   - Executive Summary (2-3 paragraphs)
   - Introduction (scope, methodology)
   - Main Content (organized by themes)
   - Key Findings (bullet list)
   - Conclusion

2. Citation Preservation:
   - Keep ALL inline citations: [Source: Name](URL)
   - Citations immediately after each fact
   - Never remove URLs from citations

3. Expansion:
   - Add explanatory text around facts
   - Provide examples and context
   - Connect related facts
   - Target: 1500-3000+ words

4. Quality:
   - Every claim has citation
   - Technical terms defined
   - Logical flow
   - Professional tone
```

**Document Example**:
```markdown
# Python vs Rust Performance Comparison

## Executive Summary

Python and Rust represent two different philosophies in programming language design,
each optimized for distinct use cases. This research examines their performance
characteristics across multiple dimensions using verified benchmarks and real-world
case studies.

Rust delivers 10-100x faster execution in CPU-bound tasks [Source: PyPerformance Benchmarks](url),
primarily due to its compiled nature and zero-cost abstractions [Source: Rust Book](url).
However, Python maintains advantages in development speed and rapid prototyping [Source: IEEE Study](url).

The choice between languages depends on performance requirements, development timeline,
and ecosystem needs rather than universal superiority of either option.

## Introduction

This document synthesizes research from official benchmarking suites, academic studies,
and industry case studies to provide a comprehensive comparison...

## Performance Characteristics

### CPU-Bound Workloads

Rust's compiled nature provides significant advantages in CPU-intensive tasks.
The PyPerformance benchmark suite demonstrates this across multiple tests [Source](url).

**Richards Benchmark**: Measures object-oriented simulation performance.
- Python 3.12: 5.2 seconds [Source: PyPerformance](url)
- Rust: 0.05 seconds [Source: Rust Benchmarks](url)
- Speedup: 104x

**N-Body Simulation**: Tests numerical computation performance.
- Python 3.12: 8.1 seconds [Source: PyPerformance](url)
- Rust: 0.12 seconds [Source: Rust Benchmarks](url)
- Speedup: 67x

These results reflect fundamental architectural differences. Rust compiles to native
machine code with LLVM optimizations [Source: Rust Reference](url), while Python
interprets bytecode in a virtual machine [Source: Python Internals](url).

### Memory Usage Patterns

...continues with comprehensive coverage...

## Key Findings

- **CPU Performance**: Rust 10-100x faster in compute-intensive tasks [Source](url)
- **Memory Efficiency**: Rust uses 40-60% less memory for equivalent functionality [Source](url)
- **Development Speed**: Python enables 30-50% faster prototyping [Source](url)
- **Compilation Overhead**: Rust compilation adds 1-60s to iteration cycle [Source](url)

## Conclusion

Performance comparison reveals clear tradeoffs rather than absolute superiority...

[1500-3000 words total with inline citations throughout]
```

### Step 5.2: Document Critic Review

**Agent**: Document Quality Critic
**Location**: `src/research.rs:1876-1896`

**Input**:
```
Original Query: "Compare Python vs Rust performance"
Document: [Draft from step 5.1]
```

**Evaluation Framework**:

```
1. Factual Verification (CRITICAL):
   ✓ Every claim has [Source: Name](URL) citation?
   ✓ URLs included in markdown format?
   ✓ Key data includes units, dates, versions?
   ✗ Found unsourced claim: "Python is easier to learn"

2. Completeness & Depth (CRITICAL):
   ✓ Original query fully answered?
   ✗ Document only 800 words (need 1500+ for complex topics)
   ✓ Multiple facts per key aspect?
   ✗ Missing coverage: Concurrency performance comparison

3. Clarity (IMPORTANT):
   ✓ Technical terms defined?
   ✓ Logical flow?
   ✓ Examples illustrate concepts?

4. Structure (IMPORTANT):
   ✓ Clear hierarchical organization?
   ✓ Informative section headings?
   ✗ Executive summary too brief

5. Professional Standards (IMPORTANT):
   ✓ Objective tone?
   ✓ Proper markdown formatting?
   ✓ No marketing language?
```

**Output** (two possibilities):

**If Approved**:
```
APPROVED

Document meets publication standards. Factual claims are well-sourced,
content is comprehensive and clear.

Minor suggestions: Could add more real-world case studies, but current
depth is sufficient for query requirements.
```

**If Improvements Needed**:
```
IMPROVEMENTS NEEDED

Critical Issues:
1. Insufficient depth: Document is 800 words, need 1500+ for this complex comparison
   → Expand sections with more examples, context, and detailed explanations

2. Missing citations: "Python is easier to learn" stated without source
   → Add citation or remove claim

3. Coverage gap: No discussion of concurrency performance (async Python vs Rust)
   → Add section comparing async/threading performance

Important Issues:
1. Executive summary too brief: Only 1 paragraph, need 2-3
   → Expand to cover key findings upfront

2. Section "Memory Usage Patterns" has only 2 data points
   → Add more comprehensive memory analysis with multiple scenarios

These issues prevent publication as current document lacks the depth
and completeness expected for this query's complexity.
```

### Step 5.3: Document Revision

**Agent**: Research Document Writer (again)
**Location**: `src/research.rs:1797-1801`

**Input**:
```
Original Query: "Compare Python vs Rust performance"
Research Content: [Original research findings]
Previous Document: [Draft from step 5.1]
Critic Feedback: [Issues from step 5.2]
```

**Writer's Revision Process**:

```
Review feedback:
1. Insufficient depth (800 → 1500+ words)
2. Missing citation for "easier to learn"
3. Missing concurrency comparison
4. Executive summary too brief
5. Memory section needs expansion

Actions:
1. Expand all sections with more details, examples, context
2. Add citation: "Python is easier to learn [Source: Stack Overflow Survey](url)"
3. Add new section: "## Concurrency and Parallelism"
   - Research from memory or new search if needed
4. Expand executive summary to 3 paragraphs
5. Add 3-4 more memory usage examples

Generate revised document...
```

**Iteration**:
- Repeats steps 5.1-5.3 up to `max_document_iterations` (default: 3)
- Exits early if APPROVED
- Uses final version even if not approved after max iterations

### Step 5.4: Add References Section

**Location**: `src/research.rs:1473-1542`

After document is approved (or max iterations reached):

```rust
fn add_sources_section(&self, text: &str) -> String {
    // Extract all [Source: Name](URL) citations
    let sources = self.extract_sources(text);

    // Separate URLs from other sources
    let mut urls = Vec::new();
    let mut other_sources = Vec::new();

    for source in sources {
        if source.starts_with("http") {
            urls.push(source);
        } else {
            other_sources.push(source);
        }
    }

    // Append references section
    output += "\n\n---\n\n## References\n\n";
    output += "### Web Sources\n\n";

    for (i, url) in urls.iter().enumerate() {
        output += &format!("{}. <{}>\n", i + 1, url);
    }

    output
}
```

**Final Document**:
```markdown
# Python vs Rust Performance Comparison

[Full document content with inline citations...]

---

## References

### Web Sources

The following websites and online resources were consulted:

1. <https://pyperformance.readthedocs.io/>
2. <https://bench.rust-lang.org/>
3. <https://www.spec.org/cpu2017/results/>
4. <https://discord.com/blog/why-discord-is-switching-from-go-to-rust>
5. <https://ieeexplore.ieee.org/document/9123456>
... (all URLs extracted from inline citations)
```

**Note**: Inline citations like `[Source: PyPerformance](https://pyperformance.readthedocs.io/)`
remain in the document body. The References section provides a complete list for easy access.

## Quality Checkpoints

Throughout the pipeline, quality is enforced at multiple stages:

| Stage | Checkpoint | Enforces |
|-------|-----------|----------|
| Planning | Plan Critic | Coverage, question quality, worker assignments |
| Execution | Supervisor | Discovery storage, following plan, source quality |
| Synthesis | Debate | Factual accuracy, source verification, gaps |
| Refinement | Iterative | Gap filling, contextualization |
| Documentation | Document Critic | Depth, citations, completeness |

**Result**: Final documents are comprehensive (1500-3000+ words), fully cited with URLs,
and verified through multiple review stages.

## Performance Metrics

Typical research run for complex query:

```
Planning:        2-3 iterations × 15-30s  = 30-90s
Worker Execution: 6 workers × 60-120s     = 60-120s (parallel)
Supervisor:      Monitoring parallel       = 60-120s (parallel)
Debate:          2 rounds × 30s × 3 agents = 180s
Refinement:      1-2 iterations × 45s      = 45-90s
Documentation:   2-3 iterations × 45s      = 90-135s

Total: ~8-12 minutes for comprehensive research
```

Memory storage during run:
- ~10-20 discoveries (workers)
- ~5-10 insights (workers)
- ~3-8 deadends (workers)
- 1 plan (lead)
- 1 feedback (supervisor, updated multiple times)
- ~50-100 total memory entries

## Configuration Impact

Adjusting iteration limits affects quality vs speed:

**Fast Mode** (quick answers):
```toml
max_plan_iterations = 1      # Accept first plan
max_debate_rounds = 0         # Skip debate
max_refinement_iterations = 1 # Minimal refinement
max_document_iterations = 1   # Accept first draft
```
Result: 3-5 minute research, lower quality

**Quality Mode** (deep research):
```toml
max_plan_iterations = 3       # Thorough planning
max_debate_rounds = 3         # Multiple debate rounds
max_refinement_iterations = 5 # Extensive refinement
max_document_iterations = 3   # Polished document
```
Result: 10-15 minute research, publication quality

**Default** (balanced):
```toml
max_plan_iterations = 3
max_debate_rounds = 2
max_refinement_iterations = 5
max_document_iterations = 3
```
Result: 8-12 minutes, high quality
