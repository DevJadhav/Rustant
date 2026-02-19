# Cognitive Extension Tools

Rustant includes 10 cognitive extension tools that augment your thinking, learning, and personal productivity. Each tool stores its state in `.rustant/<tool_name>/` and persists data across sessions using the atomic write pattern.

## Tool Summary

| Tool | Actions | Description |
|------|---------|-------------|
| `knowledge_graph` | 13 | Local graph of concepts, papers, methods, and relationships |
| `experiment_tracker` | 14 | Hypothesis tracking and experiment lifecycle management |
| `code_intelligence` | 7 | Architecture analysis, pattern detection, and tech debt |
| `content_engine` | 14 | Content creation pipeline with scheduling and adaptation |
| `skill_tracker` | 8 | Skill development with practice logging and learning paths |
| `career_intel` | 8 | Career planning with gap analysis and portfolio tracking |
| `system_monitor` | 8 | Service topology, health checks, and incident correlation |
| `life_planner` | 8 | Energy-aware scheduling, habits, and work-life balance |
| `privacy_manager` | 8 | Data boundaries, access auditing, and privacy reports |
| `self_improvement` | 8 | Behavioral pattern analysis and cognitive load tracking |

---

## knowledge_graph

Build and query a local graph of interconnected concepts. Supports 11 node types (Paper, Concept, Method, Dataset, Person, Organization, Experiment, Methodology, Result, Hypothesis, Benchmark) and 13 relationship types (Cites, Implements, Extends, Contradicts, BuildsOn, AuthoredBy, UsesDataset, RelatedTo, Reproduces, Refines, Validates, SupportsHypothesis, RefutesHypothesis).

| Action | Description |
|--------|-------------|
| `add_node` | Add a new node to the graph |
| `get_node` | Retrieve a node by ID |
| `update_node` | Update an existing node's properties |
| `remove_node` | Remove a node and its edges |
| `add_edge` | Create a relationship between two nodes |
| `remove_edge` | Remove a relationship |
| `neighbors` | Find neighboring nodes |
| `search` | Search nodes by name, type, or tags |
| `list` | List all nodes, optionally filtered by type |
| `path` | Find shortest path between two nodes |
| `stats` | Get graph statistics (node/edge counts, density) |
| `import_arxiv` | Import a paper from arXiv into the graph |
| `export_dot` | Export the graph in DOT format for visualization |

**State:** `.rustant/knowledge_graph/`

---

## experiment_tracker

Track hypotheses and experiments through their full lifecycle, from formation to evidence collection and comparison.

| Action | Description |
|--------|-------------|
| `add_hypothesis` | Create a new hypothesis with title and description |
| `update_hypothesis` | Update hypothesis status or details |
| `list_hypotheses` | List all hypotheses |
| `get_hypothesis` | Get details of a specific hypothesis |
| `add_experiment` | Define a new experiment linked to a hypothesis |
| `start_experiment` | Mark an experiment as started |
| `complete_experiment` | Mark an experiment as completed with metrics |
| `fail_experiment` | Mark an experiment as failed with notes |
| `get_experiment` | Get experiment details |
| `list_experiments` | List experiments, optionally filtered by hypothesis |
| `record_evidence` | Record evidence for/against a hypothesis |
| `compare_experiments` | Compare metrics across experiments |
| `summary` | Get a summary of all hypotheses and experiments |
| `export_markdown` | Export all data as a Markdown report |

**Key Parameters:** `id`, `title`, `description`, `hypothesis_id`, `experiment_id`, `finding`, `supports` (boolean), `confidence` (0.0-1.0), `config`, `metrics`, `notes`, `tags`.

**State:** `.rustant/experiment_tracker/`

---

## code_intelligence

Analyze codebases for architecture, patterns, technical debt, and API surfaces.

| Action | Description |
|--------|-------------|
| `analyze_architecture` | Detect project structure, modules, and dependencies |
| `detect_patterns` | Identify design patterns (singleton, factory, observer, etc.) |
| `translate_snippet` | Translate a code snippet between languages |
| `compare_implementations` | Compare two implementations side-by-side |
| `tech_debt_report` | Generate a technical debt report |
| `api_surface` | Map the public API surface of a project |
| `dependency_map` | Visualize internal module dependencies |

**State:** `.rustant/code_intelligence/`

---

## content_engine

Content creation pipeline with scheduling, calendar management, platform adaptation, and analytics.

| Action | Description |
|--------|-------------|
| `create` | Create a new content piece (blog, twitter, linkedin, etc.) |
| `update` | Update content text or metadata |
| `set_status` | Change status (idea, draft, review, scheduled, published, archived) |
| `get` | Retrieve a content piece by ID |
| `list` | List content, filtered by status or tag |
| `search` | Search content by keyword |
| `delete` | Delete a content piece |
| `schedule` | Schedule content for publication at a specific date/time |
| `calendar_add` | Add a topic to the content calendar |
| `calendar_list` | List upcoming content calendar entries |
| `calendar_remove` | Remove a calendar entry |
| `stats` | Content analytics and publication statistics |
| `adapt` | Adapt content for a different platform or audience |
| `export_markdown` | Export all content as Markdown |

**Platforms:** blog, twitter, linkedin, github, medium, newsletter.

**State:** `.rustant/content_engine/`

---

## skill_tracker

Track skill development with practice logging, assessments, and personalized learning paths.

| Action | Description |
|--------|-------------|
| `add_skill` | Add a new skill to track |
| `log_practice` | Log a practice session with duration and notes |
| `assess` | Record a skill assessment (create, update, or show) |
| `list_skills` | List all tracked skills |
| `knowledge_gaps` | Identify knowledge gaps in your skill set |
| `learning_path` | Generate a learning path for a skill |
| `progress_report` | Generate progress report (weekly, monthly, or all-time) |
| `daily_practice` | Get recommended daily practice plan |

**State:** `.rustant/skill_tracker/`

---

## career_intel

Career development tracking with goals, achievements, portfolio, gap analysis, and market intelligence.

| Action | Description |
|--------|-------------|
| `set_goal` | Set a career goal with timeline |
| `log_achievement` | Log a professional achievement (technical, leadership, etc.) |
| `add_portfolio` | Add a portfolio item (project, paper, talk, blog, etc.) |
| `gap_analysis` | Analyze gaps between current skills and career goals |
| `market_scan` | Scan market trends relevant to your career |
| `network_note` | Record a networking interaction |
| `progress_report` | Generate career progress report |
| `strategy_review` | Review and refine career strategy |

**State:** `.rustant/career_intel/`

---

## system_monitor

Monitor service topology, health, and incidents for personal infrastructure.

| Action | Description |
|--------|-------------|
| `add_service` | Add a service to the topology map |
| `topology` | View the service dependency topology |
| `health_check` | Check health of a specific service |
| `log_incident` | Log an incident for a service |
| `correlate` | Correlate incidents across services |
| `generate_runbook` | Generate a runbook for a service |
| `impact_analysis` | Analyze impact of a service failure |
| `list_services` | List all monitored services |

**Service Types:** api, database, cache, queue, frontend, worker, gateway.

**Health Statuses:** healthy, degraded, down, unknown.

**State:** `.rustant/system_monitor/`

---

## life_planner

Energy-aware scheduling, deadline management, habit tracking, and work-life balance optimization.

| Action | Description |
|--------|-------------|
| `set_energy_profile` | Define peak and low energy hours |
| `add_deadline` | Add a deadline with priority |
| `log_habit` | Log a habit completion |
| `daily_plan` | Generate an optimized daily plan based on energy levels |
| `weekly_review` | Generate weekly review summary |
| `context_switch_log` | Log context switches for productivity analysis |
| `balance_report` | Generate work-life balance report |
| `optimize_schedule` | Suggest schedule optimizations |

**State:** `.rustant/life_planner/`

---

## privacy_manager

Manage data boundaries, audit access patterns, and generate privacy reports.

| Action | Description |
|--------|-------------|
| `set_boundary` | Define a data boundary (local_only, encrypted, shareable) |
| `list_boundaries` | List all configured boundaries |
| `audit_access` | Audit data access patterns |
| `compliance_check` | Run a compliance check against boundaries |
| `export_data` | Export all data for portability |
| `delete_data` | Delete data by category |
| `encrypt_store` | Encrypt sensitive data at rest |
| `privacy_report` | Generate a comprehensive privacy report |

**State:** `.rustant/privacy_manager/`

---

## self_improvement

Analyze behavioral patterns, track cognitive load, and suggest improvements to your workflow.

| Action | Description |
|--------|-------------|
| `analyze_patterns` | Analyze interaction patterns and habits |
| `performance_report` | Generate performance metrics by task type |
| `suggest_improvements` | Get AI-suggested improvements to workflow |
| `set_preference` | Set a workflow preference |
| `get_preferences` | View current preferences |
| `cognitive_load` | Assess current cognitive load level |
| `feedback` | Provide feedback on suggestions |
| `reset_baseline` | Reset performance baselines |

**State:** `.rustant/self_improvement/`
