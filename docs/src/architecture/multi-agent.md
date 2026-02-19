# Multi-Agent

Rustant supports multi-agent orchestration with parent-child relationships and inter-agent communication.

## Components

### Agent Spawning

Parent agents can spawn child agents with:
- Isolated workspaces
- Resource limits (max tokens, max iterations, timeout)
- Specific tool subsets
- Dedicated memory contexts

### MessageBus

In-process message bus for inter-agent communication:
- Pub/sub pattern with topic-based routing
- Async channels for non-blocking delivery
- Message serialization for cross-agent boundaries

### AgentRouter

Routes messages between agents based on:
- Agent capabilities and registered tools
- Current workload and resource availability
- Topic-based subscriptions

### AgentOrchestrator

Manages agent lifecycle:
- Spawn and track child agents
- Monitor health and resource usage
- Handle agent failures with restart policies
- Coordinate parallel task execution

### ResourceLimits

Per-agent isolation:
- Token budget limits
- Iteration caps
- Execution timeout
- Sandboxed workspace directories
