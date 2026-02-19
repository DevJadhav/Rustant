# Plan Mode

Plan mode provides a structured Generate-Review-Execute flow for complex tasks.

## Overview

Instead of the agent immediately executing actions, plan mode generates a structured `ExecutionPlan` with steps, dependencies, and alternatives. You review and approve before execution begins.

## Activation

```
/plan on          # Enable plan mode
/plan off         # Disable plan mode
```

## Workflow

### 1. Generate

The LLM produces a JSON `ExecutionPlan` containing:
- **Steps** — Ordered actions with tool calls, descriptions, and estimated duration
- **Dependencies** — Step ordering constraints (step B depends on step A)
- **Alternatives** — Fallback approaches if a step fails

The parser (`parse_plan_json()`) handles markdown wrapping, trailing commas, and uses fallback parsing for malformed JSON.

### 2. Review

In the REPL, you see the plan and can:

| Key | Action |
|-----|--------|
| `a` | Approve — execute the plan |
| `x` | Reject — cancel the plan |
| `e` | Edit — modify a step |
| `r` | Regenerate — ask for a new plan |
| `+` | Add — insert a new step |
| `?` | Help — show review commands |

### 3. Execute

After approval, the agent executes steps in dependency order, reporting progress. If a step fails, it checks for alternatives.

## Plan Types

- **PlanStatus** (7 variants): Draft, InReview, Approved, Executing, Paused, Completed, Failed
- **StepStatus** (5 variants): Pending, Running, Completed, Failed, Skipped
- **PlanDecision** (7 variants): Approve, Reject, Edit, Regenerate, AddStep, RemoveStep, Reorder

## When to Use Plan Mode

Plan mode is useful for:
- Multi-step refactoring tasks
- Deployment sequences
- Security audits requiring structured approach
- Any task where you want to review the approach before execution

## Configuration

Plan mode state is managed per-session and does not persist across sessions.
