# Slash Commands Reference

Rustant provides 117 slash commands in the interactive REPL. Type `/help` to see all commands or `/help <topic>` for detailed help on a specific command.

## Session

| Command | Aliases | Description |
|---------|---------|-------------|
| `/quit` | `/exit`, `/q` | Exit Rustant |
| `/clear` | | Clear the screen |
| `/session save\|load\|list [name]` | | Save, load, or list sessions |
| `/resume [name]` | | Resume a saved session (latest if no name) |
| `/sessions [search\|tag\|filter]` | | List, search, tag, or filter saved sessions |

## Agent

| Command | Aliases | Description |
|---------|---------|-------------|
| `/cost` | | Show token usage and cost |
| `/tools` | | List available tools |
| `/status` | | Show agent status, task, and iteration count |
| `/compact` | | Compress conversation context to free memory |
| `/context` | | Show context window usage breakdown |
| `/memory` | | Show memory system stats |
| `/pin [n]` | | Pin message #n (survives compression) or list pinned |
| `/unpin <n>` | | Unpin message #n |
| `/digest [history]` | | Show or generate channel digest |
| `/replies [approve\|reject\|edit <id>]` | | Manage pending auto-reply drafts |
| `/reminders [dismiss\|complete <id>]` | | Manage follow-up reminders |
| `/intelligence [on\|off\|status]` | `/intel` | Channel intelligence status and control |
| `/meeting [detect\|record\|stop\|status]` | `/meet` | Meeting recording and transcription |
| `/briefing [morning\|evening]` | `/brief` | Generate daily briefing |
| `/why [index]` | | Show why the agent made recent decisions |
| `/voice speak <text>` | | Synthesize text to speech |
| `/canvas push\|clear\|snapshot` | | Canvas operations |
| `/council [question\|status\|detect]` | | Multi-model LLM council deliberation |
| `/plan [on\|off\|show]` | | Toggle plan mode or manage plans |
| `/persona [status\|list\|set\|auto\|stats]` | | Manage adaptive expert personas |
| `/think [on\|off\|budget <N>]` | | Toggle extended thinking mode |
| `/vision <path> [prompt]` | `/img` | Send image to LLM for analysis |
| `/ground [on\|off]` | | Toggle Gemini grounding with Google Search |
| `/structured [off\|<schema>]` | `/json` | Set JSON schema for structured output |
| `/team [create\|list\|run\|status]` | | Manage coordinated agent teams |
| `/batch [submit\|status\|results\|cancel]` | | Submit and manage batch LLM operations |
| `/index [status\|rebuild\|stats]` | | Manage the semantic code index |
| `/improve [patterns\|performance\|preferences]` | `/meta` | Self-improvement: usage patterns and performance |
| `/arxiv search\|fetch\|trending\|library` | `/paper` | Search and manage arXiv research papers |
| `/knowledge search\|add\|import\|stats` | `/kg`, `/graph` | Manage knowledge graph |
| `/experiment add\|list\|start\|complete` | `/exp`, `/hypothesis` | Track hypotheses and experiments |
| `/codeintel architecture\|debt\|patterns` | `/ci`, `/analyze` | Analyze codebase architecture |
| `/content create\|list\|calendar\|adapt` | `/write`, `/publish` | Content creation pipeline |
| `/skills add\|gaps\|practice\|progress` | `/learn` | Track skill development |
| `/career goals\|achieve\|portfolio\|gaps` | `/portfolio` | Career strategy and portfolio |
| `/monitor add\|topology\|check\|incident` | `/sysmon`, `/health` | Service monitoring and health checks |
| `/planner deadline\|habits\|daily\|review` | `/plan-life`, `/deadlines` | Life planning with energy-aware scheduling |
| `/deepresearch start\|status\|resume\|sessions\|report` | `/dr` | Deep multi-phase research with source synthesis |
| `/decisions [n]` | `/agentexplain` | Show recent agent decisions with reasoning |

## Safety

| Command | Aliases | Description |
|---------|---------|-------------|
| `/safety` | | Show current safety mode and stats |
| `/permissions [mode]` | | View or set approval mode (safe/cautious/paranoid/yolo) |
| `/trust` | | Show safety trust dashboard with per-tool approval stats |
| `/audit [show\|verify\|export\|query]` | | Show, query, export, or verify audit trail |
| `/privacy boundaries\|audit\|compliance\|export` | `/priv` | Privacy management and data boundaries |
| `/dataflow [recent\|filter\|stats\|persist]` | | Track and inspect data flow through the agent |
| `/consent status\|grant\|revoke\|list` | | Manage user consent for data usage scopes |

## Development

| Command | Aliases | Description |
|---------|---------|-------------|
| `/undo` | | Undo last file operation via git checkpoint |
| `/diff` | | Show recent file changes |
| `/review` | | Review all session file changes |
| `/init <template> <name>` | | Initialize project from template |
| `/preview [start\|stop\|restart\|status]` | | Launch or manage dev server |
| `/db [migrate\|rollback\|seed\|query\|schema\|status]` | | Database operations |
| `/test [run_all\|run_file\|run_test\|run_changed\|coverage]` | | Run tests |
| `/lint [check\|fix\|typecheck\|format\|format_check]` | | Lint and type check |
| `/deps [list\|add\|remove\|update\|audit]` | | Manage dependencies |
| `/verify [all\|test\|lint\|typecheck]` | | Run full verification pipeline |
| `/repomap [build\|show\|summary]` | | Show repository symbol map |
| `/symbols <query>` | | Search symbols across the codebase |
| `/refs <symbol>` | | Find references to a symbol |

## System

| Command | Aliases | Description |
|---------|---------|-------------|
| `/help [topic]` | `/?` | Show help (use /help <topic> for details) |
| `/keys` | | Show keyboard shortcuts |
| `/config [key] [value]` | | View or modify runtime configuration |
| `/doctor` | | Run diagnostic checks (LLM, tools, workspace) |
| `/setup` | | Re-run provider setup wizard |
| `/workflows` | | List available workflow templates |
| `/channel list\|setup\|test` | `/ch` | Manage messaging channels |
| `/workflow list\|show\|run` | `/wf` | Manage workflows |
| `/browser test\|launch\|connect\|status` | | Browser automation operations |
| `/auth status\|login\|logout` | | Manage OAuth authentication |
| `/skill list\|info\|validate` | | Manage skills (SKILL.md files) |
| `/plugin list\|info` | | Manage plugins |
| `/update check\|install` | | Check for or install updates |
| `/schedule [list\|add\|remove\|enable\|disable]` | `/sched`, `/cron` | Manage scheduled tasks and cron jobs |
| `/cache [status\|clear]` | | Show prompt cache state and savings |
| `/capabilities` | `/caps` | Show current provider capabilities |
| `/hooks [list\|add\|remove\|enable\|disable]` | | Manage agent lifecycle hooks |
| `/cdc [status\|on\|off\|interval\|enable\|disable\|cursors\|style]` | | Change Data Capture channel polling |
| `/voicecmd [on\|off\|status]` | `/vc` | Toggle voice command mode |
| `/record [start\|stop\|status]` | `/rec` | Toggle meeting recording |
| `/siri setup\|activate\|deactivate\|shortcuts\|status\|test` | | Siri voice integration (macOS) |
| `/daemon start\|stop\|status\|install\|uninstall` | | Manage background daemon process |

## UI

| Command | Aliases | Description |
|---------|---------|-------------|
| `/verbose` | `/v` | Toggle verbose output (show/hide tool execution details) |

## Security

| Command | Aliases | Description |
|---------|---------|-------------|
| `/scan [--sast\|--sca\|--secrets\|--all]` | | Run security scan |
| `/autofix [suggest\|apply] [id]` | | Generate and apply code fixes for findings |
| `/quality [path]` | | Code quality metrics and scoring |
| `/debt [path]` | | Technical debt report |
| `/complexity [path]` | | Complexity analysis of codebase |
| `/findings [--severity\|--tool\|--status]` | | List and filter security findings |
| `/license [check\|report]` | | License compliance check |
| `/sbom [cyclonedx\|csv\|diff]` | | Generate Software Bill of Materials |
| `/risk [path]` | | Calculate risk score for the project |
| `/compliance [status\|report\|check <framework>]` | | Compliance status and reporting |
| `/alerts [list\|ack\|silence\|correlate\|rules]` | | List and manage security alerts |
| `/triage [list\|prioritize\|assign\|dismiss]` | | Alert triage and prioritization |

## AI Engineer / ML

| Command | Aliases | Description |
|---------|---------|-------------|
| `/ml [status\|pipelines\|experiments]` | | ML operations overview and status |
| `/data [ingest\|validate\|split\|version\|status]` | | Data pipeline management |
| `/features [define\|compute\|serve\|list\|status]` | | Feature store management |
| `/train [start\|status\|compare\|sweep\|stop]` | | Model training runs |
| `/finetune [start\|merge\|eval\|status\|list]` | | LLM fine-tuning |
| `/quantize [run\|compare\|status]` | | Model quantization |
| `/rag [ingest\|query\|collection\|evaluate\|status]` | | RAG pipeline management |
| `/eval [run\|judge\|analyze\|report\|list]` | | Model evaluation and benchmarks |
| `/inference [serve\|stop\|status\|endpoints]` | | Model serving and inference |
| `/models [list\|download\|benchmark\|info]` | | Model zoo management |
| `/mlresearch [review\|compare\|repro\|survey]` | | ML research tools |
| `/research [review\|compare\|repro\|search]` | | Research workflows |
| `/explain [decision\|attention\|features\|shap]` | `/interpret` | Model interpretability |
| `/aisafety [check\|pii\|bias\|alignment\|report]` | | AI safety checks |
| `/redteam [run\|report\|strategies\|status]` | | Adversarial red teaming |
| `/lineage [data\|model\|graph\|trace]` | | Data/model lineage tracking |
| `/benchmark [run\|compare\|create\|list]` | | ML model benchmarks |

## Tips

- Type `/help <command>` (without the slash in the topic) for detailed help on any command.
- Commands support tab completion in the REPL.
- If you mistype a command, Rustant suggests the closest match using Levenshtein distance.
- Most commands that match CLI subcommands work identically (e.g., `/channel list` = `rustant channel list`).
