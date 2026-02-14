# Workflows

Workflows are declarative multi-step task definitions that chain tool executions together with input/output flow, conditional gates, and error handling.

## Built-in Workflows

```bash
rustant workflow list               # List all available workflows
rustant workflow show code-review   # Show workflow details
rustant workflow run code-review --input path=src/main.rs
```

## Workflow Structure

A workflow consists of:

- **Inputs** — Named parameters with types and optional/required flags
- **Steps** — Ordered tool invocations with argument templates
- **Gates** — Conditional checks between steps (approval required, condition expressions)
- **Outputs** — Named results extracted from step outputs

## Running Workflows

From the CLI:
```bash
rustant workflow run <name> --input key1=value1 --input key2=value2
```

From the REPL (inside an interactive session):
```
/workflow run <name> key1=value1 key2=value2
```

All workflow CLI subcommands are also available as REPL slash commands:
```
/workflow list                       # List available workflows
/workflow show <name>                # Show workflow details
/workflow run <name> [key=val ...]   # Run a workflow
/workflow runs                       # List active runs
/workflow status <run_id>            # Check run status
/workflow cancel <run_id>            # Cancel a running workflow
```

## Automatic Workflow Routing

The agent can automatically detect when your task matches a built-in workflow. For example, saying "do a security scan of this repo" will trigger the agent to suggest running the `security_scan` workflow.

Supported pattern matches include:
- "security scan" / "security audit" / "vulnerability" → `security_scan`
- "code review" → `code_review`
- "refactor" → `refactor`
- "generate tests" / "write tests" → `test_generation`
- "generate docs" / "write docs" → `documentation`
- "update dependencies" → `dependency_update`
- "deploy" → `deployment`
- "pr review" / "pull request review" → `pr_review`
- "changelog" / "release notes" → `changelog`
- "email triage" → `email_triage`

## Managing Runs

```bash
rustant workflow runs                # List active runs
rustant workflow status <run_id>     # Check run status
rustant workflow resume <run_id>     # Resume a paused run
rustant workflow cancel <run_id>     # Cancel a running workflow
```

## Cron Scheduling

Schedule workflows or tasks to run on a cron schedule:

```bash
rustant cron list                                          # List cron jobs
rustant cron add daily-report "0 0 9 * * * *" "Generate daily report"
rustant cron run daily-report                              # Manual trigger
rustant cron disable daily-report
rustant cron enable daily-report
rustant cron remove daily-report
```

## Background Jobs

```bash
rustant cron jobs                    # List background jobs
rustant cron cancel-job <job_id>     # Cancel a job
```

## Configuration

```toml
[scheduler]
enabled = true
max_background_jobs = 10

[[scheduler.cron_jobs]]
name = "daily-summary"
schedule = "0 0 9 * * * *"
task = "Summarize yesterday's git commits"
enabled = true
```

Cron expressions follow the 7-field format: `second minute hour day-of-month month day-of-week year`.
