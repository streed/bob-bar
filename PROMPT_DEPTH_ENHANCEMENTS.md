# Agent Prompt Depth Enhancements - October 2025

## Problem

Research outputs were sparse and lacked sufficient depth. Workers were providing brief, well-cited answers but missing comprehensive coverage.

## Root Cause

Prompts emphasized **citation accuracy** but didn't explicitly demand **content depth**. This led to:
- Single-fact responses when multiple facts were needed
- Brief summaries instead of comprehensive analysis
- Sparse documents that were technically correct but inadequate
- Workers stopping after minimal coverage

## Solution

Enhanced all agent prompts to explicitly demand comprehensive depth while maintaining citation rigor.

---

## Changes Made

### 1. All Worker Agents (5 workers)

**Added CRITICAL notices** emphasizing thoroughness:
```
CRITICAL: Your response should be THOROUGH and DETAILED. Sparse or brief answers are unacceptable.
Aim for 300-500+ words of substantive content with multiple facts and citations.
```

**Added "Content Depth Requirements" section** to every worker:
- Provide MULTIPLE facts per aspect (not just one)
- Include specific examples, case studies, statistics
- Cover historical context when relevant
- Explain mechanisms, processes, relationships in detail
- Include comparative information when useful
- Add relevant details that enhance understanding
- Use MULTIPLE tool calls to gather comprehensive information
- Aim for 300-500+ words of substantive, well-sourced content

**Updated Quality Standards** to emphasize depth:
- **Web Research Specialist**: "Every claim verifiable + response thorough enough for a research report"
- **Technical Documentation Analyst**: "Response comprehensive enough that a developer can implement the feature using only your answer + citations. Depth matters as much as accuracy."
- **Data & Metrics Specialist**: "Comprehensive quantitative analysis with multiple metrics, comparisons, and context."
- **Comparative Analysis Specialist**: "Thorough multi-dimensional comparison with quantitative and qualitative analysis."
- **Current Events Researcher**: "Comprehensive timeline with full context and implications. Every claim traceable with complete narrative."

### 2. Research Document Writer

**Added CRITICAL notice**:
```
CRITICAL: The document should be COMPREHENSIVE and DETAILED. Sparse documents are unacceptable.
Expand on worker research while preserving all citations. Target 1000-2000+ words for substantial topics.
```

**Added "Content Expansion Requirements" section**:
- EXPAND on research findings - don't just summarize
- Add explanatory text around citations to provide context
- Include examples and elaboration while citing sources
- Organize information into coherent narrative paragraphs
- Connect related facts to build comprehensive understanding
- Aim for publication-quality depth and readability

**Updated Quality Standard**:
```
Document should be comprehensive enough to serve as a Wikipedia article or research report.
Every claim verifiable + sufficient depth to satisfy expert readers.
```

### 3. Document Quality Critic

**Renamed evaluation section**:
- Changed "Completeness (CRITICAL)" to "Completeness & Depth (CRITICAL)"

**Added depth checks**:
- Original query fully answered with COMPREHENSIVE coverage?
- Document provides substantial depth (1000+ words for complex topics)?
- Multiple facts provided per key aspect (not just single facts)?

**Added sparse content as rejection criterion**:
```
IMPROVEMENTS NEEDED if ANY true:
- Document too sparse/brief (lacks depth)
```

**Updated Issue Prioritization**:
```
1. Critical: Unsourced major claims, factual errors, missing core content, insufficient depth/detail
```

---

## Files Modified

- **~/.config/bob-bar/agents.json** - Enhanced all worker, writer, and critic prompts
- **agents.example.json** - Synchronized with user config

---

## Expected Impact

### Worker Behavior Changes

✅ **More Tool Calls**: Workers will use multiple tool calls to gather comprehensive information
✅ **Deeper Research**: 300-500+ word responses instead of brief 50-100 word answers
✅ **Multiple Facts**: Several facts per aspect instead of single facts
✅ **Contextual Information**: Historical context, examples, mechanisms explained
✅ **Comparative Data**: Including comparison information when useful

### Writer Behavior Changes

✅ **Content Expansion**: Expanding worker findings into narrative paragraphs
✅ **Explanatory Text**: Adding context around citations
✅ **Comprehensive Documents**: 1000-2000+ word documents for substantial topics
✅ **Publication Quality**: Wikipedia-article or research-report level depth

### Critic Behavior Changes

✅ **Depth Checking**: Rejecting sparse documents even if well-cited
✅ **Word Count Awareness**: Flagging documents under 1000 words for complex topics
✅ **Multiple Facts Requirement**: Ensuring comprehensive coverage of each aspect

---

## Testing Recommendations

1. **Baseline Test**: Run same query before/after prompt changes
2. **Word Count**: Measure average document length increase
3. **Tool Usage**: Count tool calls per worker (should increase)
4. **Rejection Rate**: Monitor if critic rejects more sparse outputs
5. **Quality Assessment**: Compare depth and comprehensiveness of final documents

---

## Potential Issues to Monitor

⚠️ **Increased Latency**: More tool calls and longer content generation
⚠️ **Token Usage**: Longer prompts and responses increase token consumption
⚠️ **Over-Expansion**: Risk of workers being too verbose without adding substance
⚠️ **Hallucination Risk**: Longer responses might encourage unsourced elaboration

### Mitigation:
- CRITICAL notices emphasize "substantive" content (not just long)
- Citation requirements remain strict (every fact must cite sources)
- Debate process still catches unsourced claims
- Quality standards still prioritize accuracy alongside depth

---

## Backward Compatibility

✅ **Config Format**: No changes to JSON structure
✅ **Code Changes**: No Rust code modifications needed
✅ **Tool Definitions**: No changes to tools.json
✅ **Rollback**: Simply replace agents.json with previous version if needed

---

**Implemented**: October 3, 2025
**Status**: ✅ Complete - Prompts enhanced for comprehensive depth
