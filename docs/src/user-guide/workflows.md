# Workflows & Cron

Workflows are declarative multi-step task definitions that chain tool executions with input/output flow, conditional gates, and error handling.

## Built-in Workflows (38)

Rustant includes 38 built-in workflow templates across categories:

### Core Development
| Template | Description |
|----------|-------------|
| `code_review` | Automated code review with quality analysis |
| `refactor` | Guided refactoring with safety checks |
| `test_generation` | Generate tests for existing code |
| `documentation` | Auto-generate documentation |
| `dependency_update` | Update and verify dependencies |
| `pr_review` | Pull request review workflow |
| `changelog` | Generate changelog entries |
| `fullstack_verify` | Automated test/lint/build verification |

### Security & Compliance
| Template | Description |
|----------|-------------|
| `security_scan` | Full security scan (SAST, SCA, secrets) |
| `compliance_audit` | Compliance framework evaluation |
| `code_review_ai` | AI-assisted code review with security focus |

### SRE & Operations
| Template | Description |
|----------|-------------|
| `incident_response` | SRE incident response steps |
| `sre_deployment` | Deployment with risk assessment |
| `alert_triage` | Alert investigation and triage |
| `sre_health_review` | Infrastructure health review |
| `deployment` | Standard deployment workflow |

### Daily Productivity
| Template | Description |
|----------|-------------|
| `morning_briefing` | Morning summary and planning |
| `daily_briefing_full` | Comprehensive daily briefing |
| `end_of_day_summary` | End of day review |
| `email_triage` | Email classification and response |
| `meeting_recorder` | Meeting recording and transcription |

### Cognitive Extension
| Template | Description |
|----------|-------------|
| `knowledge_graph` | Build concept maps |
| `experiment_tracking` | Track hypotheses and experiments |
| `code_analysis` | Architecture and pattern analysis |
| `content_pipeline` | Content strategy and drafting |
| `skill_development` | Skill assessment and learning plans |
| `career_planning` | Career strategy and portfolio |
| `system_monitoring` | Service health monitoring |
| `life_planning` | Energy-aware scheduling |
| `privacy_audit` | Data boundary management |
| `self_improvement_loop` | Performance analysis and feedback |

### macOS Automation
| Template | Description |
|----------|-------------|
| `app_automation` | macOS app interaction workflows |
| `dependency_audit` | Dependency security audit |
| `arxiv_research` | Academic paper research pipeline |

### ML/AI Engineering
| Template | Description |
|----------|-------------|
| `ml_training` | End-to-end model training |
| `rag_pipeline` | RAG pipeline setup and evaluation |
| `model_evaluation` | Model benchmarking and comparison |
| `data_pipeline` | Data processing workflow |
| `llm_finetune` | LLM fine-tuning pipeline |
| `research_paper` | Research methodology and analysis |
| `model_deployment` | Model serving and monitoring |
| `safety_audit` | AI safety evaluation |

## Running Workflows

```bash
# CLI
rustant workflow list
rustant workflow show code-review
rustant workflow run code-review --input path=src/main.rs

# REPL
/workflow list
/workflow show <name>
/workflow run <name> key=value
/workflow runs
/workflow status <run_id>
/workflow cancel <run_id>
```

## Automatic Workflow Routing

The agent automatically detects when tasks match built-in workflows:
- "security scan" → `security_scan`
- "code review" → `code_review`
- "generate tests" → `test_generation`
- "deploy" → `deployment`
- "update dependencies" → `dependency_update`
- "knowledge graph" → `knowledge_graph`
- "email triage" → `email_triage`
- And many more pattern matches...

## Cron Scheduling

```bash
rustant cron list                                          # List cron jobs
rustant cron add daily-report "0 0 9 * * * *" "Generate daily report"
rustant cron run daily-report                              # Manual trigger
rustant cron disable daily-report
rustant cron enable daily-report
rustant cron remove daily-report
rustant cron jobs                                          # List background jobs
```

Cron state persists to `.rustant/cron/state.json` with atomic write pattern.

Cron expressions use 7-field format: `second minute hour day-of-month month day-of-week year`.

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
