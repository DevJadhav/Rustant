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

```bash
rustant workflow run <name> --input key1=value1 --input key2=value2
```

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
