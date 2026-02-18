//! Slash command registry for REPL command discovery.
//!
//! Provides structured metadata for all `/command` slash commands,
//! enabling categorized help, alias resolution, and tab completion.

/// Categories for grouping commands in `/help` output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    Session,
    Agent,
    Safety,
    Development,
    System,
    Ui,
}

impl CommandCategory {
    pub fn label(&self) -> &'static str {
        match self {
            CommandCategory::Session => "Session",
            CommandCategory::Agent => "Agent",
            CommandCategory::Safety => "Safety",
            CommandCategory::Development => "Development",
            CommandCategory::System => "System",
            CommandCategory::Ui => "UI",
        }
    }

    pub fn all() -> &'static [CommandCategory] {
        &[
            CommandCategory::Session,
            CommandCategory::Agent,
            CommandCategory::Safety,
            CommandCategory::Development,
            CommandCategory::System,
            CommandCategory::Ui,
        ]
    }
}

impl std::fmt::Display for CommandCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Metadata describing a slash command.
#[derive(Debug, Clone)]
pub struct CommandInfo {
    /// Primary name including the slash, e.g., "/compact".
    pub name: &'static str,
    /// Alternative aliases, e.g., &["/exit", "/q"] for /quit.
    pub aliases: &'static [&'static str],
    /// One-line description shown in /help.
    pub description: &'static str,
    /// Usage pattern, e.g., "/config [key] [value]".
    pub usage: &'static str,
    /// Category for grouping in /help.
    pub category: CommandCategory,
    /// Detailed help text shown by `/help <command>`. Includes examples and explanation.
    pub detailed_help: Option<&'static str>,
}

/// Registry holding all slash commands with their metadata.
pub struct CommandRegistry {
    commands: Vec<CommandInfo>,
}

#[allow(dead_code)]
impl CommandRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            commands: Vec::new(),
        }
    }

    /// Create a registry pre-populated with all default commands.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register_defaults();
        registry
    }

    /// Register a single command.
    pub fn register(&mut self, info: CommandInfo) {
        self.commands.push(info);
    }

    /// Register all built-in commands.
    pub fn register_defaults(&mut self) {
        // Session commands
        self.register(CommandInfo {
            name: "/quit",
            aliases: &["/exit", "/q"],
            description: "Exit Rustant",
            usage: "/quit",
            category: CommandCategory::Session,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/clear",
            aliases: &[],
            description: "Clear the screen",
            usage: "/clear",
            category: CommandCategory::Session,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/session",
            aliases: &[],
            description: "Save, load, or list sessions",
            usage: "/session save|load|list [name]",
            category: CommandCategory::Session,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/resume",
            aliases: &[],
            description: "Resume a saved session (latest if no name)",
            usage: "/resume [name]",
            category: CommandCategory::Session,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/sessions",
            aliases: &[],
            description: "List, search, tag, or filter saved sessions",
            usage: "/sessions [search <q> | tag <name> <tag> | filter <tag>]",
            category: CommandCategory::Session,

            detailed_help: Some("Manage saved sessions.\n\nSubcommands:\n  /sessions              - List recent sessions\n  /sessions search <q>   - Search sessions by name, goal, or summary\n  /sessions tag <n> <t>  - Add a tag to a session\n  /sessions filter <tag> - List sessions with a specific tag\n\nExamples:\n  /sessions search auth  - Find sessions related to auth\n  /sessions tag my-proj bugfix - Tag session 'my-proj' with 'bugfix'\n  /sessions filter refactor    - List all refactoring sessions"),
        });

        // Agent commands
        self.register(CommandInfo {
            name: "/cost",
            aliases: &[],
            description: "Show token usage and cost",
            usage: "/cost",
            category: CommandCategory::Agent,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/tools",
            aliases: &[],
            description: "List available tools",
            usage: "/tools",
            category: CommandCategory::Agent,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/status",
            aliases: &[],
            description: "Show agent status, task, and iteration count",
            usage: "/status",
            category: CommandCategory::Agent,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/compact",
            aliases: &[],
            description: "Compress conversation context to free memory",
            usage: "/compact",
            category: CommandCategory::Agent,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/context",
            aliases: &[],
            description: "Show context window usage breakdown",
            usage: "/context",
            category: CommandCategory::Agent,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/memory",
            aliases: &[],
            description: "Show memory system stats",
            usage: "/memory",
            category: CommandCategory::Agent,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/pin",
            aliases: &[],
            description: "Pin message #n (survives compression) or list pinned",
            usage: "/pin [n]",
            category: CommandCategory::Agent,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/unpin",
            aliases: &[],
            description: "Unpin message #n",
            usage: "/unpin <n>",
            category: CommandCategory::Agent,

            detailed_help: None,
        });

        // Safety commands
        self.register(CommandInfo {
            name: "/safety",
            aliases: &[],
            description: "Show current safety mode and stats",
            usage: "/safety",
            category: CommandCategory::Safety,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/permissions",
            aliases: &[],
            description: "View or set approval mode (safe/cautious/paranoid/yolo)",
            usage: "/permissions [mode]",
            category: CommandCategory::Safety,

            detailed_help: Some("Control how the agent asks for permission before executing actions.\n\nModes:\n  safe     - Auto-approve read-only operations, ask for writes/executes (default)\n  cautious - Auto-approve reads and reversible writes, ask for executes\n  paranoid - Ask for approval on every single action\n  yolo     - Auto-approve everything (use with caution!)\n\nExamples:\n  /permissions          - Show current mode\n  /permissions cautious - Switch to cautious mode"),
        });
        self.register(CommandInfo {
            name: "/audit",
            aliases: &[],
            description: "Show, query, export, or verify audit trail",
            usage: "/audit [show [n] | verify | export [fmt] | query <tool>]",
            category: CommandCategory::Safety,

            detailed_help: None,
        });

        // Development commands
        self.register(CommandInfo {
            name: "/undo",
            aliases: &[],
            description: "Undo last file operation via git checkpoint",
            usage: "/undo",
            category: CommandCategory::Development,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/diff",
            aliases: &[],
            description: "Show recent file changes",
            usage: "/diff",
            category: CommandCategory::Development,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/review",
            aliases: &[],
            description: "Review all session file changes",
            usage: "/review",
            category: CommandCategory::Development,

            detailed_help: None,
        });

        // System commands
        self.register(CommandInfo {
            name: "/help",
            aliases: &["/?"],
            description: "Show help (use /help <topic> for details)",
            usage: "/help [topic]",
            category: CommandCategory::System,

            detailed_help: Some("Show all commands or detailed help for a specific topic.\n\nExamples:\n  /help           - Show all available commands\n  /help safety    - Show safety-related commands and explanation\n  /help session   - Show session management commands\n  /help compact   - Show help for /compact command\n\nTopics match command names (without /) or category names."),
        });
        self.register(CommandInfo {
            name: "/config",
            aliases: &[],
            description: "View or modify runtime configuration",
            usage: "/config [key] [value]",
            category: CommandCategory::System,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/doctor",
            aliases: &[],
            description: "Run diagnostic checks (LLM, tools, workspace)",
            usage: "/doctor",
            category: CommandCategory::System,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/setup",
            aliases: &[],
            description: "Re-run provider setup wizard",
            usage: "/setup",
            category: CommandCategory::System,

            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/workflows",
            aliases: &[],
            description: "List available workflow templates",
            usage: "/workflows",
            category: CommandCategory::System,

            detailed_help: None,
        });

        // Trust command
        self.register(CommandInfo {
            name: "/trust",
            aliases: &[],
            description: "Show safety trust dashboard with per-tool approval stats",
            usage: "/trust",
            category: CommandCategory::Safety,

            detailed_help: Some("Display a trust calibration dashboard showing:\n  - Current approval mode with plain-English explanation\n  - Per-tool approval/denial statistics from the audit log\n  - Suggestions for adjusting trust based on your usage patterns\n\nThe dashboard helps you understand why you are being prompted and\nmake informed decisions about adjusting your approval mode."),
        });

        // Keys command
        self.register(CommandInfo {
            name: "/keys",
            aliases: &[],
            description: "Show keyboard shortcuts (TUI: F1 for overlay)",
            usage: "/keys",
            category: CommandCategory::System,

            detailed_help: Some("Show all keyboard shortcuts grouped by context.\n\nIn TUI mode, press F1 for a floating overlay.\n\nGlobal:    Ctrl+C/D quit, Ctrl+L scroll to bottom\nOverlays:  Ctrl+E explanation panel, Ctrl+T task board\nApproval:  y=approve, n=deny, a=approve all, d=diff, ?=help\nVim mode:  i/a=insert, Esc=normal, /=search, q=quit"),
        });

        // UI commands
        self.register(CommandInfo {
            name: "/verbose",
            aliases: &["/v"],
            description: "Toggle verbose output (show/hide tool execution details)",
            usage: "/verbose",
            category: CommandCategory::Ui,

            detailed_help: Some(
                "Toggle verbose mode on or off.\n\n\
                 When verbose is OFF (default):\n  \
                 - Tool execution messages are hidden\n  \
                 - Status changes are hidden\n  \
                 - Token usage updates are hidden\n  \
                 - Decision explanations are hidden\n  \
                 - Only assistant responses, approvals, and budget warnings are shown\n\n\
                 When verbose is ON:\n  \
                 - All tool execution details are shown\n  \
                 - Token usage is updated in real-time\n  \
                 - Decision explanations are displayed\n\n\
                 The final task summary line (iterations, tokens, cost) is always shown.",
            ),
        });

        // Channel Intelligence commands
        self.register(CommandInfo {
            name: "/digest",
            aliases: &[],
            description: "Show or generate channel digest",
            usage: "/digest [history]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Show the latest channel digest or generate one on demand.\n\n\
                 Usage:\n  /digest          — Show the latest digest\n  \
                 /digest history   — List past digests\n\n\
                 Digests summarize channel messages over a configured time period,\n\
                 highlighting action items and high-priority messages.",
            ),
        });
        self.register(CommandInfo {
            name: "/replies",
            aliases: &[],
            description: "Manage pending auto-reply drafts",
            usage: "/replies [approve|reject|edit <id>]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "View and manage pending auto-reply drafts.\n\n\
                 Usage:\n  /replies               — List all pending replies\n  \
                 /replies approve <id>   — Approve and send a reply\n  \
                 /replies reject <id>    — Reject and discard a reply\n  \
                 /replies edit <id>      — Edit a reply before sending\n\n\
                 Auto-replies are gated by the SafetyGuardian approval system.",
            ),
        });
        self.register(CommandInfo {
            name: "/reminders",
            aliases: &[],
            description: "Manage follow-up reminders",
            usage: "/reminders [dismiss|complete <id>]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "View and manage scheduled follow-up reminders.\n\n\
                 Usage:\n  /reminders               — List active reminders\n  \
                 /reminders dismiss <id>   — Dismiss a reminder\n  \
                 /reminders complete <id>  — Mark as completed\n\n\
                 Reminders are auto-created for messages classified as needing follow-up.\n\
                 ICS calendar files are exported to .rustant/reminders/.",
            ),
        });
        self.register(CommandInfo {
            name: "/intelligence",
            aliases: &["/intel"],
            description: "Channel intelligence status and control",
            usage: "/intelligence [on|off|status]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "View or control the channel intelligence layer.\n\n\
                 Usage:\n  /intelligence          — Show intelligence status\n  \
                 /intelligence off      — Temporarily disable\n  \
                 /intelligence on       — Re-enable\n  \
                 /intelligence status   — Detailed stats\n\n\
                 Shows messages processed, auto-replies sent, digests generated,\n\
                 reminders scheduled, and per-channel configuration.",
            ),
        });

        // macOS daily assistant commands
        self.register(CommandInfo {
            name: "/meeting",
            aliases: &["/meet"],
            description: "Meeting recording and transcription",
            usage: "/meeting [detect|record|stop|status]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Record, transcribe, and summarize meetings.\n\n\
                 Usage:\n  /meeting detect  — Check for active meeting apps\n  \
                 /meeting record  — Start recording with TTS announcement, silence detection,\n\
                 \x20                   auto-transcription, and save to Notes.app\n  \
                 /meeting stop    — Stop recording (auto-transcribes and saves)\n  \
                 /meeting status  — Show recording status and silence monitor state\n\n\
                 The record command announces 'Recording has started' via TTS, monitors for\n\
                 silence (auto-stops after 60s of silence), transcribes via Whisper API,\n\
                 and saves the transcript to Notes.app 'Meeting Transcripts' folder.",
            ),
        });
        self.register(CommandInfo {
            name: "/briefing",
            aliases: &["/brief"],
            description: "Generate daily briefing (calendar, reminders, weather)",
            usage: "/briefing [morning|evening]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Generate a daily briefing combining calendar, reminders, and weather.\n\n\
                 Usage:\n  /briefing          — Morning briefing (default)\n  \
                 /briefing morning  — Today's schedule, reminders, weather\n  \
                 /briefing evening  — End-of-day summary with tomorrow preview\n\n\
                 Briefings are saved to Notes.app in the 'Daily Briefings' folder.",
            ),
        });

        // ── Transparency ──
        self.register(CommandInfo {
            name: "/why",
            aliases: &[],
            description: "Show why the agent made recent decisions",
            usage: "/why [index]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Show the reasoning behind recent agent decisions.\n\n\
                 Usage:\n  /why         -- Show most recent decision\n  \
                 /why 0       -- Show first recorded decision\n  \
                 /why N       -- Show decision at index N\n\n\
                 Each explanation includes: decision type, confidence score,\n\
                 reasoning chain, alternatives considered, and influence factors.",
            ),
        });

        // ── CLI Subcommand Parity ──
        self.register(CommandInfo {
            name: "/channel",
            aliases: &["/ch"],
            description: "Manage messaging channels",
            usage: "/channel list|setup [name]|test <name>",
            category: CommandCategory::System,

            detailed_help: Some(
                "Manage messaging channel integrations.\n\n\
                 Usage:\n  /channel list          — List all configured channels\n  \
                 /channel setup [name]  — Interactive channel setup wizard\n  \
                 /channel test <name>   — Test a channel connection\n\n\
                 Equivalent to: rustant channel <subcommand>",
            ),
        });
        self.register(CommandInfo {
            name: "/workflow",
            aliases: &["/wf"],
            description: "Manage workflows (list, show, run)",
            usage: "/workflow list|show|run <name> [key=val]",
            category: CommandCategory::System,

            detailed_help: Some(
                "Manage and run workflow templates.\n\n\
                 Usage:\n  /workflow list              — List available workflows\n  \
                 /workflow show <name>       — Show workflow details\n  \
                 /workflow run <name> [k=v]  — Run a workflow with optional inputs\n  \
                 /workflow runs              — List active runs\n  \
                 /workflow status <id>       — Show run status\n  \
                 /workflow cancel <id>       — Cancel a running workflow\n\n\
                 Equivalent to: rustant workflow <subcommand>",
            ),
        });
        self.register(CommandInfo {
            name: "/voice",
            aliases: &[],
            description: "Synthesize text to speech",
            usage: "/voice speak <text> [-v voice]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Synthesize text to speech via OpenAI TTS.\n\n\
                 Usage:\n  /voice speak <text>         — Speak text with default voice\n  \
                 /voice speak <text> -v nova  — Use a specific voice\n\n\
                 Requires OPENAI_API_KEY. Equivalent to: rustant voice speak <text>",
            ),
        });
        self.register(CommandInfo {
            name: "/browser",
            aliases: &[],
            description: "Browser automation operations",
            usage: "/browser test|launch|connect|status",
            category: CommandCategory::System,

            detailed_help: Some(
                "Control browser automation.\n\n\
                 Usage:\n  /browser test [url]     — Test by navigating to a URL\n  \
                 /browser launch [port]  — Launch Chrome with remote debugging\n  \
                 /browser connect [port] — Connect to existing Chrome instance\n  \
                 /browser status         — Show connection status\n\n\
                 Equivalent to: rustant browser <subcommand>",
            ),
        });
        self.register(CommandInfo {
            name: "/auth",
            aliases: &[],
            description: "Manage OAuth authentication",
            usage: "/auth status|login|logout <provider>",
            category: CommandCategory::System,

            detailed_help: Some(
                "Manage OAuth authentication for LLM providers and channels.\n\n\
                 Usage:\n  /auth status            — Show auth status for all providers\n  \
                 /auth login <provider>  — Start OAuth login flow\n  \
                 /auth logout <provider> — Remove stored tokens\n\n\
                 Equivalent to: rustant auth <subcommand>",
            ),
        });
        self.register(CommandInfo {
            name: "/canvas",
            aliases: &[],
            description: "Canvas operations (push, clear, snapshot)",
            usage: "/canvas push <type> <content>|clear|snapshot",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Push content to or manage the canvas.\n\n\
                 Usage:\n  /canvas push <type> <content> — Push content to canvas\n  \
                 /canvas clear                  — Clear the canvas\n  \
                 /canvas snapshot               — Show canvas snapshot\n\n\
                 Types: html, markdown, code, chart, table, form, image, diagram\n\
                 Equivalent to: rustant canvas <subcommand>",
            ),
        });
        self.register(CommandInfo {
            name: "/skill",
            aliases: &[],
            description: "Manage skills (SKILL.md files)",
            usage: "/skill list|info|validate <path>",
            category: CommandCategory::System,

            detailed_help: Some(
                "Manage SKILL.md skill definitions.\n\n\
                 Usage:\n  /skill list              — List loaded skills\n  \
                 /skill info <path>       — Show skill details\n  \
                 /skill validate <path>   — Validate a skill file\n\n\
                 Equivalent to: rustant skill <subcommand>",
            ),
        });
        self.register(CommandInfo {
            name: "/plugin",
            aliases: &[],
            description: "Manage plugins",
            usage: "/plugin list|info <name>",
            category: CommandCategory::System,

            detailed_help: Some(
                "Manage native plugins.\n\n\
                 Usage:\n  /plugin list        — List discovered plugins\n  \
                 /plugin info <name> — Show plugin info\n\n\
                 Equivalent to: rustant plugin <subcommand>",
            ),
        });
        self.register(CommandInfo {
            name: "/update",
            aliases: &[],
            description: "Check for or install updates",
            usage: "/update check|install",
            category: CommandCategory::System,

            detailed_help: Some(
                "Check for and install Rustant updates.\n\n\
                 Usage:\n  /update check    — Check for available updates\n  \
                 /update install  — Download and install the latest version\n\n\
                 Equivalent to: rustant update <subcommand>",
            ),
        });

        // ── Scheduler ──
        self.register(CommandInfo {
            name: "/schedule",
            aliases: &["/sched", "/cron"],
            description: "Manage scheduled tasks and cron jobs",
            usage: "/schedule [list|add|remove|enable|disable]",
            category: CommandCategory::System,

            detailed_help: Some(
                "Manage background scheduled tasks (cron jobs).\n\n\
                 Usage:\n  /schedule list              — List all scheduled jobs\n  \
                 /schedule add <name> <cron> <task>  — Add a new scheduled job\n  \
                 /schedule remove <name>     — Remove a scheduled job\n  \
                 /schedule enable <name>     — Enable a disabled job\n  \
                 /schedule disable <name>    — Disable a job\n\n\
                 Example cron expressions:\n  0 0 8 * * MON-FRI *  — 8 AM weekdays\n  \
                 0 0 17 * * * *           — 5 PM daily\n  \
                 0 30 9 * * SAT *         — 9:30 AM Saturdays\n\n\
                 Configure in .rustant/config.toml under [scheduler].",
            ),
        });

        // ── LLM Council ──
        self.register(CommandInfo {
            name: "/council",
            aliases: &[],
            description: "Multi-model LLM council deliberation",
            usage: "/council [question|status|detect]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Run multi-model deliberation for planning tasks.\n\n\
                 Usage:\n  /council <question>    — Run council deliberation\n  \
                 /council status        — Show council config and members\n  \
                 /council detect        — Auto-detect available providers\n\n\
                 Requires 2+ LLM providers (API keys or Ollama models).\n\
                 Configure in .rustant/config.toml under [council].\n\n\
                 Note: Questions are sent to ALL configured providers.\n\
                 Be mindful of data privacy when using multiple cloud providers.",
            ),
        });

        // ── Plan Mode ──
        self.register(CommandInfo {
            name: "/plan",
            aliases: &[],
            description: "Toggle plan mode or manage plans",
            usage: "/plan [on|off|show]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Enable plan mode to generate a step-by-step plan before execution.\n\n\
                 When plan mode is on, every task you enter will:\n\
                 1. Generate a structured multi-step plan\n\
                 2. Show the plan for your review\n\
                 3. Let you approve, edit, or reject before execution\n\
                 4. Execute step by step with progress tracking\n\n\
                 Usage:\n  /plan on    — Enable plan mode\n  \
                 /plan off   — Disable plan mode (default)\n  \
                 /plan show  — Show current plan\n  \
                 /plan       — Show plan mode status\n\n\
                 During review:\n  [a]pprove — Execute the plan\n  \
                 [e] <n> <desc> — Edit step description\n  \
                 [r] <n> — Remove a step\n  \
                 [+] <n> <desc> — Add a step\n  \
                 [?] <question> — Ask about the plan\n  \
                 [x] — Cancel the plan\n\n\
                 Configure in .rustant/config.toml under [plan].",
            ),
        });

        // ── ArXiv Research ──
        self.register(CommandInfo {
            name: "/arxiv",
            aliases: &["/paper", "/research"],
            description: "Search and manage arXiv research papers",
            usage:
                "/arxiv search <query> | fetch <id> | trending [category] | library | analyze <id>",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Search, fetch, and manage academic papers with multi-source enrichment.\n\n\
                 Usage:\n  /arxiv search <query>         — Search papers by topic\n  \
                 /arxiv fetch <id>              — Get full paper details\n  \
                 /arxiv analyze <id>            — Structured analysis\n  \
                 /arxiv trending [category]     — Recent trending papers\n  \
                 /arxiv library                 — List saved papers\n  \
                 /arxiv save <id>               — Save to library\n  \
                 /arxiv bibtex                  — Export as BibTeX\n  \
                 /arxiv paper_to_code <id>      — Generate code scaffold\n  \
                 /arxiv paper_to_notebook <id>  — Generate Jupyter notebook\n  \
                 /arxiv semantic_search <q>     — Keyword search over library\n  \
                 /arxiv summarize <id>          — Multi-level summary\n  \
                 /arxiv citation_graph <id>     — Citation network analysis\n  \
                 /arxiv blueprint <id>          — Implementation blueprint\n  \
                 /arxiv reindex                 — Rebuild search index\n\n\
                 Examples:\n  /arxiv search transformer fine-tuning\n  \
                 /arxiv fetch 1706.03762\n  \
                 /arxiv summarize 1706.03762\n  \
                 /arxiv citation_graph 1706.03762\n\n\
                 Papers stored in .rustant/arxiv/library.json.\n\
                 Uses arXiv, Semantic Scholar, and OpenAlex APIs.",
            ),
        });

        // Cognitive extension tools
        self.register(CommandInfo {
            name: "/knowledge",
            aliases: &["/kg", "/graph"],
            description: "Manage knowledge graph of concepts and relationships",
            usage: "/knowledge search <query> | add <name> | import <arxiv_id> | stats",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Build and query a local knowledge graph of concepts, papers, methods, and people.\n\n\
                 Usage:\n  /knowledge add <name>         — Add a concept node\n  \
                 /knowledge search <query>      — Search nodes by name/tag\n  \
                 /knowledge import <arxiv_id>   — Import paper from arxiv library\n  \
                 /knowledge neighbors <id>      — Find connected nodes\n  \
                 /knowledge path <from> <to>    — Shortest path between nodes\n  \
                 /knowledge stats               — Graph statistics\n  \
                 /knowledge export              — Export as Graphviz DOT\n\n\
                 Data stored in .rustant/knowledge/graph.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/experiment",
            aliases: &["/exp", "/hypothesis"],
            description: "Track hypotheses, experiments, and results",
            usage: "/experiment add <title> | list | start <id> | complete <id>",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Track scientific hypotheses and experiments with evidence recording.\n\n\
                 Usage:\n  /experiment add <title>       — Add a new hypothesis\n  \
                 /experiment list               — List hypotheses and experiments\n  \
                 /experiment start <id>         — Start an experiment\n  \
                 /experiment complete <id>      — Complete with metrics\n  \
                 /experiment evidence <hyp_id>  — Record evidence\n  \
                 /experiment compare <ids>      — Compare experiments\n  \
                 /experiment summary            — Evidence summary\n\n\
                 Data stored in .rustant/experiments/tracker.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/codeintel",
            aliases: &["/ci", "/analyze"],
            description: "Analyze codebase architecture, patterns, and tech debt",
            usage: "/codeintel architecture | debt | patterns | translate | api | deps",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Cross-language codebase analysis and intelligence.\n\n\
                 Usage:\n  /codeintel architecture       — Analyze project structure\n  \
                 /codeintel debt                — Scan for tech debt (TODO/FIXME/complexity)\n  \
                 /codeintel patterns            — Detect design patterns\n  \
                 /codeintel translate           — Translate code between languages\n  \
                 /codeintel api                 — Extract public API surface\n  \
                 /codeintel deps               — Map dependencies\n  \
                 /codeintel compare <a> <b>    — Compare two files\n\n\
                 Read-only analysis cached in .rustant/code_intel/cache.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/content",
            aliases: &["/write", "/publish"],
            description: "Content creation and publishing pipeline",
            usage: "/content create <title> | list | calendar | adapt <id> | stats",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Multi-platform content pipeline with lifecycle tracking.\n\n\
                 Usage:\n  /content create <title>       — Create new content piece\n  \
                 /content list                  — List content by status\n  \
                 /content calendar              — Show content calendar\n  \
                 /content adapt <id>            — Adapt content for another platform\n  \
                 /content schedule <id> <date>  — Schedule for publishing\n  \
                 /content stats                 — Content statistics\n\n\
                 Platforms: Blog, Twitter, LinkedIn, GitHub, Medium, Newsletter.\n\
                 Data stored in .rustant/content/library.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/skills",
            aliases: &["/learn"],
            description: "Track skill development and learning paths",
            usage: "/skills add <name> | gaps | practice | progress | daily",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Track skill progression, knowledge gaps, and practice sessions.\n\n\
                 Usage:\n  /skills add <name>            — Add a skill to track\n  \
                 /skills gaps                   — Show knowledge gaps\n  \
                 /skills practice <id> <mins>   — Log practice session\n  \
                 /skills progress               — Progress report\n  \
                 /skills daily                  — Daily practice suggestions\n  \
                 /skills path create <name>     — Create learning path\n\n\
                 Data stored in .rustant/skills/tracker.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/career",
            aliases: &["/portfolio"],
            description: "Career strategy, achievements, and portfolio management",
            usage: "/career goals | achieve <title> | portfolio | gaps | network",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Career goal tracking, achievement logging, and portfolio management.\n\n\
                 Usage:\n  /career goals                 — List career goals\n  \
                 /career achieve <title>        — Log an achievement\n  \
                 /career portfolio              — View portfolio items\n  \
                 /career gaps                   — Gap analysis\n  \
                 /career network <person>       — Add networking note\n  \
                 /career strategy               — Strategy review\n\n\
                 Data stored in .rustant/career/intel.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/monitor",
            aliases: &["/sysmon", "/health"],
            description: "Service monitoring, health checks, and incident tracking",
            usage: "/monitor add <name> | topology | check | incident | impact <id>",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Service topology management and health monitoring.\n\n\
                 Usage:\n  /monitor add <name> <url>     — Register a service\n  \
                 /monitor topology              — Show service dependency graph\n  \
                 /monitor check [id]            — Run health checks\n  \
                 /monitor incident <title>      — Log an incident\n  \
                 /monitor impact <id>           — Cascade impact analysis\n  \
                 /monitor services              — List all services\n\n\
                 Data stored in .rustant/monitoring/topology.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/planner",
            aliases: &["/plan-life", "/deadlines"],
            description: "Life planning with energy-aware scheduling and habits",
            usage: "/planner deadline <title> | habits | daily | review | energy",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Energy-aware scheduling, deadline tracking, and habit management.\n\n\
                 Usage:\n  /planner deadline <title>     — Add a deadline\n  \
                 /planner habits                — View habit streaks\n  \
                 /planner daily                 — Generate daily plan\n  \
                 /planner review                — Weekly review\n  \
                 /planner energy                — Set energy profile\n  \
                 /planner balance               — Work-life balance report\n\n\
                 Data stored in .rustant/life/planner.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/privacy",
            aliases: &["/priv"],
            description: "Privacy management, data boundaries, and compliance",
            usage: "/privacy boundaries | audit | compliance | export | delete <domain>",
            category: CommandCategory::Safety,

            detailed_help: Some(
                "Data boundary management, access auditing, and privacy controls.\n\n\
                 Usage:\n  /privacy boundaries           — List data boundaries\n  \
                 /privacy audit [limit]         — Show access log\n  \
                 /privacy compliance            — Run compliance check\n  \
                 /privacy export                — Export all data as JSON\n  \
                 /privacy delete <domain>       — Delete data for a domain\n  \
                 /privacy report                — Full privacy report\n\n\
                 WARNING: /privacy delete is destructive and requires confirmation.\n\
                 Data stored in .rustant/privacy/config.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/cdc",
            aliases: &[],
            description: "Change Data Capture: background channel polling and auto-reply",
            usage: "/cdc [status|on|off|interval <channel> <secs>|enable <channel>|disable <channel>|cursors|style]",
            category: CommandCategory::System,

            detailed_help: Some(
                "Manage background channel polling.\n\n\
                 Subcommands:\n  status   — Show polling state and intervals\n  \
                 on/off   — Enable/disable global CDC\n  \
                 interval — Set per-channel polling interval\n  \
                 enable   — Enable a specific channel\n  \
                 disable  — Disable a specific channel\n  \
                 cursors  — Show cursor positions\n  \
                 style    — Show learned communication style profiles",
            ),
        });
        self.register(CommandInfo {
            name: "/improve",
            aliases: &["/meta"],
            description: "Self-improvement: usage patterns, performance, and preferences",
            usage: "/improve patterns | performance | preferences | feedback | load",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Analyze usage patterns and optimize agent performance.\n\n\
                 Usage:\n  /improve patterns             — Analyze tool usage patterns\n  \
                 /improve performance           — Performance metrics report\n  \
                 /improve suggestions           — Get improvement suggestions\n  \
                 /improve preferences           — View/set preferences\n  \
                 /improve feedback <1-5>        — Record task satisfaction\n  \
                 /improve load                  — Cognitive load estimate\n\n\
                 Data stored in .rustant/meta/improvement.json.",
            ),
        });
        self.register(CommandInfo {
            name: "/voicecmd",
            aliases: &["/vc"],
            description: "Toggle voice command mode (listen→transcribe→respond)",
            usage: "/voicecmd [on|off|status]",
            category: CommandCategory::System,

            detailed_help: Some(
                "Start or stop background voice command listening.\n\n\
                 Usage:\n  /voicecmd on      — Start voice command session\n  \
                 /voicecmd off     — Stop voice command session\n  \
                 /voicecmd status  — Show voice session status\n  \
                 /voicecmd         — Toggle (start if off, stop if on)\n\n\
                 Requires OPENAI_API_KEY for Whisper STT.\n\
                 TUI shortcut: Ctrl+V",
            ),
        });
        self.register(CommandInfo {
            name: "/record",
            aliases: &["/rec"],
            description: "Toggle meeting recording with auto-transcription",
            usage: "/record [start|stop|status] [title]",
            category: CommandCategory::System,

            detailed_help: Some(
                "Start or stop meeting recording with transcription.\n\n\
                 Usage:\n  /record start [title]  — Start recording (macOS only)\n  \
                 /record stop           — Stop recording, transcribe, show results\n  \
                 /record status         — Show recording status\n  \
                 /record                — Toggle (start if off, stop if on)\n\n\
                 Requires OPENAI_API_KEY for Whisper transcription.\n\
                 Audio recorded via afrecord, silence auto-stop supported.\n\
                 TUI shortcut: Ctrl+M",
            ),
        });

        // ── Prompt Caching ──
        self.register(CommandInfo {
            name: "/cache",
            aliases: &[],
            description: "Show prompt cache state, hit rate, and savings",
            usage: "/cache [status|clear]",
            category: CommandCategory::System,

            detailed_help: Some(
                "Display and manage provider-level prompt caching.\n\n\
                 Usage:\n  /cache          — Show cache state, TTL, hit rate, savings\n  \
                 /cache status   — Same as /cache\n  \
                 /cache clear    — Reset cache metrics\n\n\
                 Prompt caching reduces latency and cost by reusing cached prefixes.\n\
                 Supported providers: Anthropic (cache_control), OpenAI (automatic), Gemini (CachedContent).",
            ),
        });

        // ── Personas ──
        self.register(CommandInfo {
            name: "/persona",
            aliases: &[],
            description: "Manage adaptive expert personas",
            usage: "/persona [status|list|set <name>|auto|stats]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "View and control the adaptive persona system.\n\n\
                 Usage:\n  /persona          — Show active persona and selection rationale\n  \
                 /persona status   — Same as /persona\n  \
                 /persona list     — List all available personas with metrics\n  \
                 /persona set <n>  — Manually set persona (architect, security, mlops, general)\n  \
                 /persona auto     — Re-enable auto-detection from task classification\n  \
                 /persona stats    — Per-persona success rates and task distribution\n\n\
                 Personas adjust system prompts, tool preferences, and safety thresholds.",
            ),
        });

        // ── Extended Thinking ──
        self.register(CommandInfo {
            name: "/think",
            aliases: &[],
            description: "Toggle extended thinking mode or set budget",
            usage: "/think [on|off|budget <N>]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Control extended thinking (chain-of-thought reasoning) for LLM responses.\n\n\
                 Usage:\n  /think          — Show thinking mode status\n  \
                 /think on       — Enable extended thinking\n  \
                 /think off      — Disable extended thinking\n  \
                 /think budget <N> — Set thinking token budget (e.g., 4096)\n\n\
                 When enabled, the LLM shows its reasoning process before answering.\n\
                 Requires a provider that supports thinking (Anthropic, Gemini).\n\
                 Auto-enabled for destructive operations in safety modes.",
            ),
        });

        // ── Vision ──
        self.register(CommandInfo {
            name: "/vision",
            aliases: &["/img"],
            description: "Send an image to the LLM for analysis",
            usage: "/vision <path> [prompt]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Analyze images using the LLM's vision capabilities.\n\n\
                 Usage:\n  /vision /path/to/image.png         — Describe the image\n  \
                 /vision /path/to/screenshot.png What do you see? — Custom prompt\n\n\
                 Supported formats: PNG, JPEG, GIF, WebP\n\
                 Max file size: 20MB\n\
                 Requires a provider with vision support (Anthropic, OpenAI, Gemini).",
            ),
        });

        // ── Grounding ──
        self.register(CommandInfo {
            name: "/ground",
            aliases: &[],
            description: "Toggle Gemini grounding with Google Search",
            usage: "/ground [on|off]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Enable or disable Gemini grounding with Google Search.\n\n\
                 Usage:\n  /ground          — Show grounding status\n  \
                 /ground on       — Enable Google Search grounding\n  \
                 /ground off      — Disable grounding\n\n\
                 When enabled, Gemini uses Google Search to ground responses\n\
                 with real-time web data. Sources are displayed with responses.\n\
                 Only available with Gemini provider.",
            ),
        });

        // ── Structured Output ──
        self.register(CommandInfo {
            name: "/structured",
            aliases: &["/json"],
            description: "Set JSON schema for structured output mode",
            usage: "/structured [off|<schema_json>]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Control structured output mode with JSON schema enforcement.\n\n\
                 Usage:\n  /structured off                    — Disable structured output\n  \
                 /structured {\"type\":\"object\",...}  — Set JSON schema\n\n\
                 When a schema is set, the LLM will return responses conforming\n\
                 to the specified JSON schema. Supported by OpenAI and Gemini.\n\
                 Use /structured off to return to normal text output.",
            ),
        });

        // ── Provider Capabilities ──
        self.register(CommandInfo {
            name: "/capabilities",
            aliases: &["/caps"],
            description: "Show current provider capabilities",
            usage: "/capabilities",
            category: CommandCategory::System,

            detailed_help: Some(
                "Display the capabilities of the current LLM provider.\n\n\
                 Shows:\n  - Vision (image analysis)\n  \
                 - Extended thinking (chain-of-thought)\n  \
                 - Structured output (JSON schema)\n  \
                 - Citations (source references)\n  \
                 - Code execution (sandbox)\n  \
                 - Grounding (web search)\n  \
                 - Prompt caching support\n  \
                 - Context window size and pricing\n\n\
                 Useful for understanding what features are available\n\
                 with your current provider configuration.",
            ),
        });

        // ── Hooks ──
        self.register(CommandInfo {
            name: "/hooks",
            aliases: &[],
            description: "Manage agent lifecycle hooks",
            usage: "/hooks [list|add|remove|enable|disable]",
            category: CommandCategory::System,

            detailed_help: Some(
                "Manage event-driven hooks that execute on agent lifecycle events.\n\n\
                 Usage:\n  /hooks list                        — List all registered hooks\n  \
                 /hooks add <event> <command>        — Add a new hook\n  \
                 /hooks remove <event>               — Remove hooks for an event\n  \
                 /hooks enable <event>               — Enable hooks for an event\n  \
                 /hooks disable <event>              — Disable hooks for an event\n\n\
                 Events: session_start, session_end, task_start, task_complete,\n\
                 pre_tool_use, post_tool_use, safety_denial, error_occurred, etc.\n\n\
                 Configure in .rustant/config.toml under [hooks].",
            ),
        });

        // ── Agent Teams ──
        self.register(CommandInfo {
            name: "/team",
            aliases: &[],
            description: "Manage coordinated agent teams",
            usage: "/team [create|list|run|status] <name>",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Create and manage coordinated agent teams.\n\n\
                 Usage:\n  /team create <name> --members <a>,<b>  — Create a team\n  \
                 /team list                               — List teams\n  \
                 /team run <name> <task>                  — Run a team task\n  \
                 /team status <name>                      — Show team status\n  \
                 /team remove <name>                      — Remove a team\n\n\
                 Coordination strategies: sequential, parallel, review_chain,\n\
                 plan_execute_verify. Each member can use different providers.",
            ),
        });

        // ── Batch Operations ──
        self.register(CommandInfo {
            name: "/batch",
            aliases: &[],
            description: "Submit and manage batch LLM operations",
            usage: "/batch [submit|status|results|cancel] [args]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "Submit bulk operations for batch processing (50% cost savings).\n\n\
                 Usage:\n  /batch submit <tasks...>  — Submit tasks for batch processing\n  \
                 /batch status             — Check batch status\n  \
                 /batch results            — Retrieve batch results\n  \
                 /batch cancel             — Cancel pending batch\n\n\
                 Batch processing runs asynchronously at lower cost.\n\
                 Currently supported with Anthropic (Message Batches API).",
            ),
        });

        // ── Code Index ──
        self.register(CommandInfo {
            name: "/index",
            aliases: &[],
            description: "Manage the semantic code index",
            usage: "/index [status|rebuild|stats]",
            category: CommandCategory::Agent,

            detailed_help: Some(
                "View and manage the project's semantic code index.\n\n\
                 Usage:\n  /index          — Show indexing status and statistics\n  \
                 /index status   — Same as /index\n  \
                 /index rebuild  — Force a full re-index of the workspace\n  \
                 /index stats    — Show stale files, coverage, vector count\n\n\
                 The index enables semantic code search via the codebase_search tool.\n\
                 Incremental re-indexing detects changed files automatically.",
            ),
        });
    }

    /// Look up a command by name or alias.
    pub fn lookup(&self, input: &str) -> Option<&CommandInfo> {
        self.commands
            .iter()
            .find(|cmd| cmd.name == input || cmd.aliases.contains(&input))
    }

    /// Generate categorized help text.
    pub fn help_text(&self) -> String {
        let mut output = String::from("\nAvailable commands:\n");

        for category in CommandCategory::all() {
            let cmds: Vec<&CommandInfo> = self
                .commands
                .iter()
                .filter(|c| c.category == *category)
                .collect();

            if cmds.is_empty() {
                continue;
            }

            output.push_str(&format!("\n  {}:\n", category.label()));

            for cmd in cmds {
                let aliases = if cmd.aliases.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", cmd.aliases.join(", "))
                };
                output.push_str(&format!(
                    "    {:<24} {}{}\n",
                    cmd.usage, cmd.description, aliases
                ));
            }
        }

        output.push_str("\nInput:\n  Type your task or question and press Enter.\n");
        output
    }

    /// Return command name completions matching a prefix.
    pub fn completions(&self, prefix: &str) -> Vec<&str> {
        let mut results = Vec::new();
        for cmd in &self.commands {
            if cmd.name.starts_with(prefix) {
                results.push(cmd.name);
            }
            for alias in cmd.aliases {
                if alias.starts_with(prefix) {
                    results.push(alias);
                }
            }
        }
        results.sort();
        results
    }

    /// Return all registered commands.
    pub fn all(&self) -> &[CommandInfo] {
        &self.commands
    }

    /// Return the number of registered commands.
    pub fn len(&self) -> usize {
        self.commands.len()
    }

    /// Check if the registry is empty.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Get detailed help for a specific topic (command name or category).
    ///
    /// Looks up by command name (with or without `/` prefix), alias, or category label.
    /// Returns formatted help text or None if the topic is not found.
    pub fn help_for(&self, topic: &str) -> Option<String> {
        let topic_lower = topic.to_lowercase();
        let topic_with_slash = if topic_lower.starts_with('/') {
            topic_lower.clone()
        } else {
            format!("/{}", topic_lower)
        };

        // Try exact command lookup
        if let Some(cmd) = self.lookup(&topic_with_slash) {
            let mut output = format!("{} - {}\n", cmd.name, cmd.description);
            output.push_str(&format!("Usage: {}\n", cmd.usage));
            if !cmd.aliases.is_empty() {
                output.push_str(&format!("Aliases: {}\n", cmd.aliases.join(", ")));
            }
            if let Some(detailed) = cmd.detailed_help {
                output.push('\n');
                output.push_str(detailed);
                output.push('\n');
            }
            return Some(output);
        }

        // Try category match
        for cat in CommandCategory::all() {
            if cat.label().to_lowercase() == topic_lower {
                let cmds: Vec<&CommandInfo> = self
                    .commands
                    .iter()
                    .filter(|c| c.category == *cat)
                    .collect();
                if cmds.is_empty() {
                    return None;
                }
                let mut output = format!("{} Commands:\n\n", cat.label());
                for cmd in cmds {
                    output.push_str(&format!("  {:<24} {}\n", cmd.usage, cmd.description));
                }
                return Some(output);
            }
        }

        None
    }

    /// Suggest the closest command for an unknown input using edit distance.
    pub fn suggest(&self, input: &str) -> Option<&str> {
        let mut best: Option<(&str, usize)> = None;

        for cmd in &self.commands {
            let dist = edit_distance(input, cmd.name);
            if dist <= 3 && (best.is_none() || dist < best.unwrap().1) {
                best = Some((cmd.name, dist));
            }
            for alias in cmd.aliases {
                let dist = edit_distance(input, alias);
                if dist <= 3 && (best.is_none() || dist < best.unwrap().1) {
                    best = Some((alias, dist));
                }
            }
        }

        best.map(|(name, _)| name)
    }
}

/// Simple Levenshtein edit distance for command suggestions.
fn edit_distance(a: &str, b: &str) -> usize {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    let a_len = a_bytes.len();
    let b_len = b_bytes.len();

    let mut prev = (0..=b_len).collect::<Vec<_>>();
    let mut curr = vec![0; b_len + 1];

    for i in 1..=a_len {
        curr[0] = i;
        for j in 1..=b_len {
            let cost = if a_bytes[i - 1] == b_bytes[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[b_len]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_registry_new_is_empty() {
        let registry = CommandRegistry::new();
        assert_eq!(registry.len(), 0);
        assert!(registry.is_empty());
    }

    #[test]
    fn test_register_defaults_populates() {
        let registry = CommandRegistry::with_defaults();
        // We have 53+ commands registered (45 original + 8 new: think/vision/ground/structured/capabilities/hooks/team/batch)
        assert!(
            registry.len() >= 53,
            "Expected at least 53 commands, got {}",
            registry.len()
        );
    }

    #[test]
    fn test_lookup_by_name() {
        let registry = CommandRegistry::with_defaults();
        let cmd = registry.lookup("/help");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "/help");
    }

    #[test]
    fn test_lookup_by_alias() {
        let registry = CommandRegistry::with_defaults();
        // /q is an alias for /quit
        let cmd = registry.lookup("/q");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "/quit");
    }

    #[test]
    fn test_lookup_alias_exit() {
        let registry = CommandRegistry::with_defaults();
        let cmd = registry.lookup("/exit");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "/quit");
    }

    #[test]
    fn test_lookup_alias_question_mark() {
        let registry = CommandRegistry::with_defaults();
        let cmd = registry.lookup("/?");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "/help");
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        let registry = CommandRegistry::with_defaults();
        assert!(registry.lookup("/nonexistent").is_none());
    }

    #[test]
    fn test_help_text_contains_all_categories() {
        let registry = CommandRegistry::with_defaults();
        let help = registry.help_text();
        for cat in CommandCategory::all() {
            assert!(
                help.contains(cat.label()),
                "Help text missing category: {}",
                cat.label()
            );
        }
    }

    #[test]
    fn test_help_text_contains_all_commands() {
        let registry = CommandRegistry::with_defaults();
        let help = registry.help_text();
        for cmd in registry.all() {
            assert!(
                help.contains(cmd.usage),
                "Help text missing command usage: {}",
                cmd.usage
            );
        }
    }

    #[test]
    fn test_completions_prefix_co() {
        let registry = CommandRegistry::with_defaults();
        let completions = registry.completions("/co");
        assert!(
            completions.contains(&"/compact"),
            "Missing /compact in completions: {:?}",
            completions
        );
        assert!(
            completions.contains(&"/config"),
            "Missing /config in completions: {:?}",
            completions
        );
        assert!(
            completions.contains(&"/cost"),
            "Missing /cost in completions: {:?}",
            completions
        );
        assert!(
            completions.contains(&"/context"),
            "Missing /context in completions: {:?}",
            completions
        );
        assert!(
            !completions.contains(&"/help"),
            "/help should not match /co prefix"
        );
    }

    #[test]
    fn test_completions_slash_only() {
        let registry = CommandRegistry::with_defaults();
        let completions = registry.completions("/");
        // All commands and aliases should match "/"
        let total_names: usize = registry.all().iter().map(|c| 1 + c.aliases.len()).sum();
        assert_eq!(
            completions.len(),
            total_names,
            "All {} names/aliases should match '/'",
            total_names
        );
    }

    #[test]
    fn test_no_duplicate_names_or_aliases() {
        let registry = CommandRegistry::with_defaults();
        let mut seen = HashSet::new();
        for cmd in registry.all() {
            assert!(
                seen.insert(cmd.name),
                "Duplicate command name: {}",
                cmd.name
            );
            for alias in cmd.aliases {
                assert!(
                    seen.insert(alias),
                    "Duplicate alias: {} (for {})",
                    alias,
                    cmd.name
                );
            }
        }
    }

    #[test]
    fn test_suggest_close_match() {
        let registry = CommandRegistry::with_defaults();
        // "/hep" is close to "/help"
        let suggestion = registry.suggest("/hep");
        assert_eq!(suggestion, Some("/help"));
    }

    #[test]
    fn test_suggest_no_match() {
        let registry = CommandRegistry::with_defaults();
        // "/xyzabc" is too far from any command
        let suggestion = registry.suggest("/xyzabc");
        assert!(suggestion.is_none());
    }

    #[test]
    fn test_edit_distance_identical() {
        assert_eq!(edit_distance("hello", "hello"), 0);
    }

    #[test]
    fn test_edit_distance_one_char() {
        assert_eq!(edit_distance("/help", "/hep"), 1);
    }

    #[test]
    fn test_edit_distance_different() {
        assert_eq!(edit_distance("abc", "xyz"), 3);
    }

    #[test]
    fn test_category_display() {
        assert_eq!(format!("{}", CommandCategory::Session), "Session");
        assert_eq!(format!("{}", CommandCategory::Agent), "Agent");
        assert_eq!(format!("{}", CommandCategory::Safety), "Safety");
        assert_eq!(format!("{}", CommandCategory::Development), "Development");
        assert_eq!(format!("{}", CommandCategory::System), "System");
        assert_eq!(format!("{}", CommandCategory::Ui), "UI");
    }

    #[test]
    fn test_all_categories_have_commands() {
        let registry = CommandRegistry::with_defaults();
        for cat in CommandCategory::all() {
            let count = registry.all().iter().filter(|c| c.category == *cat).count();
            assert!(count > 0, "Category {} has no commands", cat.label());
        }
    }

    #[test]
    fn test_core_commands_present() {
        let registry = CommandRegistry::with_defaults();
        for name in &["/help", "/quit", "/compact", "/status", "/config"] {
            let cmd = registry.lookup(name);
            assert!(cmd.is_some(), "Missing core command: {}", name);
        }
    }

    #[test]
    fn test_ui_category_has_commands() {
        let registry = CommandRegistry::with_defaults();
        let count = registry
            .all()
            .iter()
            .filter(|c| c.category == CommandCategory::Ui)
            .count();
        assert!(count >= 1, "UI category should have at least 1 command");
    }

    #[test]
    fn test_help_text_has_no_tui_markers() {
        let registry = CommandRegistry::with_defaults();
        let help = registry.help_text();
        assert!(
            !help.contains("(TUI)"),
            "Help text should not contain (TUI) markers since TUI was removed"
        );
    }
}
