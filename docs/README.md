# Bob-Bar Documentation

Welcome to the bob-bar documentation. This guide provides a deep dive into how bob-bar works internally.

## Documentation Index

1. [Architecture Overview](architecture.md) - High-level system design and component interaction
2. [Research Pipeline](research-pipeline.md) - Detailed walkthrough of the multi-agent research process
3. [Memory System](memory-system.md) - How shared memory and vector search work
4. [Agent Roles](agent-roles.md) - Detailed explanation of each agent type and their responsibilities
5. [Configuration Guide](configuration.md) - All configuration options and how to tune them
6. [Tool System](tool-system.md) - How tools are executed and managed
7. [Quality Control](quality-control.md) - The debate, critic, and refinement systems

## Quick Start

Bob-bar is a Rust/Iced desktop application that provides an AI launcher with advanced multi-agent research capabilities. It combines:

- **LLM Integration**: Communicates with Ollama for local LLM inference
- **Multi-Agent System**: Specialized agents work in parallel on different aspects of research
- **Shared Memory**: SQLite-based persistent memory with vector search for agent coordination
- **Quality Control**: Multiple layers of review, debate, and refinement
- **Tool Execution**: Agents can call external tools (web search, Wikipedia, etc.)

## Core Concepts

### Multi-Agent Research

Instead of a single LLM trying to answer everything, bob-bar:

1. **Decomposes** complex queries into focused sub-questions
2. **Assigns** each sub-question to a specialist worker agent
3. **Executes** workers in parallel with tool access
4. **Coordinates** via shared memory (discoveries, deadends, feedback)
5. **Synthesizes** findings into a comprehensive document
6. **Reviews** with debate and critic agents for quality

### Iterative Refinement

Every major stage has review and refinement:

- **Planning**: Plan critic reviews research plan before execution
- **Research**: Supervisor monitors workers and provides feedback
- **Synthesis**: Debate agents (advocate/skeptic/synthesizer) review findings
- **Documentation**: Document critic reviews drafts, writer revises
- **Final Review**: Multiple iterations until quality standards met

### Memory-Driven Coordination

Agents don't just work in isolation:

- **Discoveries**: Facts found by workers are stored and shared
- **Deadends**: Failed searches are recorded to avoid duplication
- **Insights**: Patterns observed across research are captured
- **Feedback**: Supervisor guidance is stored for workers to see
- **Vector Search**: Agents can search memory semantically

## Architecture Philosophy

Bob-bar is designed around these principles:

1. **Specialization**: Each agent has a narrow, well-defined role
2. **Verification**: Facts must be sourced and verifiable
3. **Transparency**: All citations preserved with URLs for fact-checking
4. **Iteration**: Quality improves through multiple review cycles
5. **Collaboration**: Shared memory enables agent coordination
6. **Configurability**: Iteration counts, worker limits, and behavior are tunable

## System Requirements

- **Rust**: 1.70+ for building
- **Ollama**: Running locally with models installed
- **SQLite**: With vec0 extension for vector search
- **Linux**: Tested on Linux (may work on other platforms)

## Next Steps

- Read [Architecture Overview](architecture.md) for the big picture
- Explore [Research Pipeline](research-pipeline.md) to understand the flow
- Check [Agent Roles](agent-roles.md) to understand each agent's job
- Review [Configuration Guide](configuration.md) to tune behavior
