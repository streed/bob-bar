# Agent Prompt Refinements

## Overview

All agent prompts have been significantly enhanced to provide clearer instructions, better structure, and more rigorous evaluation criteria. These refinements ensure higher quality research outputs through the critic-refiner loop.

## Changes by Agent

### 1. Lead Agent (Query Decomposer)

**Before:**
- Simple instruction to break queries into sub-questions
- No guidance on quality or structure

**After:**
- Detailed guidelines for creating effective sub-questions
- Emphasis on independence and specificity
- Concrete examples of good decomposition
- Clear format requirements (JSON array only)
- Coverage verification for all aspects

**Key Improvements:**
- Sub-questions are now more targeted and researchable
- Better distribution across specialist agents
- Reduced redundancy between questions
- More consistent JSON output format

### 2. Web Research Specialist

**Before:**
- Basic instruction to use web tools
- Vague "cite sources" directive

**After:**
- Specific methodology for finding and verifying information
- Source quality criteria (official docs, academic papers, reputable sites)
- Structured formatting requirements
- Cross-referencing approach for verification
- Explicit citation format with examples
- Context and recency awareness

**Key Improvements:**
- More authoritative sources
- Better source citations with inline references
- Distinction between facts and opinions
- Awareness of information age/relevance
- Structured output format

### 3. Data Analyst

**Before:**
- Generic "use APIs and present findings"
- No guidance on analysis depth

**After:**
- Detailed methodology from data gathering to insight extraction
- Specific presentation guidelines (tables, units, context)
- Emphasis on quantitative analysis with numbers
- Requirement to explain meaning, not just present data
- Trend and pattern identification
- Data limitations awareness

**Key Improvements:**
- More analytical (not just descriptive)
- Better data presentation with tables
- Quantitative focus with specific metrics
- Context for all numbers and trends
- Source attribution for data points

### 4. General Researcher

**Before:**
- Very generic "be comprehensive and accurate"
- No strategic approach

**After:**
- Strategic research approach (determine info type, select tools)
- Multi-source verification methodology
- Balance for controversial topics
- Quality standards checklist
- Adaptive depth based on question type
- Synthesis into coherent narrative

**Key Improvements:**
- More strategic tool selection
- Better source verification
- Balanced perspectives
- Context and detail balance
- Logical organization

### 5. Critic Agent (Most Critical Change)

**Before:**
- Generic "identify issues"
- Unclear approval criteria
- No structured evaluation framework

**After:**
- **7 specific evaluation criteria:**
  1. Completeness
  2. Accuracy
  3. Sources
  4. Clarity
  5. Depth
  6. Relevance
  7. Consistency
- Structured criticism format (ISSUE + IMPROVEMENT NEEDED)
- Explicit instruction to be critical and find problems
- High bar for approval ("genuinely excellent")
- 2-4 specific actionable criticisms required

**Key Improvements:**
- Much more likely to provide constructive feedback
- Specific criteria prevent vague criticism
- Structured format makes issues clear
- Higher standards ensure quality
- Less likely to approve mediocre outputs

### 6. Refiner Agent

**Before:**
- Simple "improve output" instruction
- No guidance on addressing criticism

**After:**
- Explicit instruction to address EVERY issue
- Permission/encouragement to use tools for additional research
- Prohibition against minor edits (substantial improvements required)
- Specific actions for different criticism types
- Structure maintenance while enhancing content
- Clear examples of what to do

**Key Improvements:**
- More substantial improvements (not superficial edits)
- Tool usage for gathering missing information
- Better responsiveness to criticism
- Maintains structure while improving quality
- Clearer expectations for refinement depth

## Impact on Research Quality

### Before Refinements:
- Critic often approved mediocre outputs
- Workers provided generic information without sources
- Lead agent produced redundant sub-questions
- Refiner made only superficial changes
- Overall output quality was inconsistent

### After Refinements:
- **Critic is rigorous:** Finds specific issues to improve
- **Workers are specialized:** Each follows clear methodology
- **Lead creates better questions:** More focused and independent
- **Refiner makes substantial improvements:** Addresses all criticism
- **Output quality is higher:** More sources, better analysis, clearer structure

## Expected Behavior Changes

### Critic Agent Behavior

**Now more likely to criticize for:**
- Missing sources or citations
- Superficial analysis lacking depth
- Incomplete answers (missing aspects)
- Unsupported claims
- Poor structure or unclear presentation
- Irrelevant information
- Logical inconsistencies

**Only approves when:**
- ALL 7 criteria are met at high standard
- Sources properly cited
- Analysis is deep and insightful
- Answer is complete and clear
- Information is relevant and consistent

### Refinement Loop Behavior

**Expected pattern:**
1. **Iteration 1:** Critic finds 2-4 issues (very common now)
2. **Refinement 1:** Refiner adds sources, depth, missing info
3. **Iteration 2:** Critic finds 1-2 remaining issues (if any)
4. **Refinement 2:** Refiner polishes and addresses final concerns
5. **Iteration 3:** Critic approves OR identifies edge cases
6. **Result:** High-quality, well-sourced, comprehensive output

### Worker Behavior

**Web Research Specialist:**
- Cites sources inline: `[Source: example.com]`
- Notes information recency
- Distinguishes facts from opinions
- Uses clear headings and structure

**Data Analyst:**
- Leads with key metrics
- Uses tables for comparative data
- Includes units and context
- Explains significance of numbers

**General Researcher:**
- Selects appropriate tools strategically
- Verifies claims across sources
- Provides balanced perspectives
- Adapts depth to question

## Testing the Refinements

### Test Query 1: Simple Factual
```
What is the weather in Tokyo?
```
**Expected:**
- Data Analyst uses weather API
- Provides specific metrics (temp, conditions, humidity)
- Cites API source
- Critic likely approves (straightforward data)

### Test Query 2: Research-Heavy
```
Compare the popularity and ecosystem of Python vs Rust
```
**Expected:**
- Lead decomposes into Python aspects, Rust aspects, comparison
- Workers research with sources
- Initial output has some gaps
- Critic requests deeper analysis or more sources
- Refiner adds tool-based research
- 2-3 refinement iterations
- Final output is comprehensive with citations

### Test Query 3: Multi-Domain
```
What's the weather in Seattle and who are the main contributors to the Rust language?
```
**Expected:**
- Clean separation of sub-questions
- Data Analyst handles weather
- Web Research Specialist handles contributors
- Both cite sources
- Critic checks for completeness on both aspects
- May request more detail on contributors
- 1-2 refinement iterations

## Configuration Tuning

### If Critic is Too Strict
Edit `~/.config/bob-bar/agents.json`:
```json
"system_prompt": "... Only respond with 'APPROVED' if the output meets at least 5 of 7 criteria ..."
```

### If Hitting Max Iterations Too Often
Increase max iterations:
```json
"config": {
  "max_refinement_iterations": 7
}
```

### If Want Faster Results
Lower standards slightly:
```json
"system_prompt": "... respond with 'APPROVED' if output is good (not necessarily excellent) ..."
```

## Monitoring Effectiveness

### With Debug Mode
```bash
cargo run --release -- --debug
```

Look for:
```
[Research] Iteration 1: Refining based on criticism
```

Check if:
- Critic is providing specific, actionable feedback
- Refinements are addressing the criticism
- Quality improves iteration to iteration
- Approval happens when quality is genuinely high

### Signs of Good Configuration
- ✅ Critic finds 1-4 issues on first iteration
- ✅ Refiner makes substantial changes
- ✅ Quality visibly improves each iteration
- ✅ Approval happens within 2-4 iterations
- ✅ Final output has sources and depth

### Signs of Issues
- ❌ Critic always approves immediately (too lenient)
- ❌ Critic never approves (too strict)
- ❌ Refiner makes no meaningful changes
- ❌ Iterations make output worse
- ❌ Always hits max iterations

## Best Practices

### For Agent Prompts
1. Be specific about expectations
2. Provide concrete examples
3. Define quality criteria explicitly
4. Use numbered guidelines
5. Specify output format
6. Encourage thoroughness

### For Critic
1. Define evaluation dimensions
2. Set high but achievable standards
3. Require structured criticism
4. Be explicit about approval criteria
5. Focus on actionable feedback

### For Refiner
1. Emphasize substantial improvements
2. Enable tool usage for research
3. Require addressing all criticism
4. Maintain structure while improving
5. Provide guidance on how to improve

## Prompt Engineering Tips

### Making Critic More/Less Strict

**More Strict:**
```
Only output 'APPROVED' if the output would be publishable in an academic journal.
```

**Less Strict:**
```
Output 'APPROVED' if the output adequately addresses the question with reasonable quality.
```

### Focusing Criticism

**For Source-Heavy Domains:**
```
Pay special attention to source quality and citation completeness.
```

**For Technical Accuracy:**
```
Verify technical claims are accurate and properly explained.
```

### Customizing Workers

**For Your Domain:**
Add domain-specific instructions:
```json
{
  "system_prompt": "...\n\nFor technical topics, include code examples and API documentation references."
}
```

## Summary

The refined agent prompts create a much more rigorous research process:

1. **Lead** creates better sub-questions
2. **Workers** produce higher quality, well-sourced research
3. **Critic** actively finds issues rather than rubber-stamping
4. **Refiner** makes substantial improvements
5. **Result** is well-researched, properly sourced, comprehensive output

The key change is the **critic agent** - it now has specific criteria and is instructed to be demanding rather than lenient. This drives the quality improvement loop effectively.
