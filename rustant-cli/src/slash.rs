//! Slash command registry for REPL and TUI command discovery.
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
    /// Whether this command is only available in TUI mode.
    pub tui_only: bool,
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
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/clear",
            aliases: &[],
            description: "Clear the screen",
            usage: "/clear",
            category: CommandCategory::Session,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/session",
            aliases: &[],
            description: "Save, load, or list sessions",
            usage: "/session save|load|list [name]",
            category: CommandCategory::Session,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/resume",
            aliases: &[],
            description: "Resume a saved session (latest if no name)",
            usage: "/resume [name]",
            category: CommandCategory::Session,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/sessions",
            aliases: &[],
            description: "List, search, tag, or filter saved sessions",
            usage: "/sessions [search <q> | tag <name> <tag> | filter <tag>]",
            category: CommandCategory::Session,
            tui_only: false,
            detailed_help: Some("Manage saved sessions.\n\nSubcommands:\n  /sessions              - List recent sessions\n  /sessions search <q>   - Search sessions by name, goal, or summary\n  /sessions tag <n> <t>  - Add a tag to a session\n  /sessions filter <tag> - List sessions with a specific tag\n\nExamples:\n  /sessions search auth  - Find sessions related to auth\n  /sessions tag my-proj bugfix - Tag session 'my-proj' with 'bugfix'\n  /sessions filter refactor    - List all refactoring sessions"),
        });

        // Agent commands
        self.register(CommandInfo {
            name: "/cost",
            aliases: &[],
            description: "Show token usage and cost",
            usage: "/cost",
            category: CommandCategory::Agent,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/tools",
            aliases: &[],
            description: "List available tools",
            usage: "/tools",
            category: CommandCategory::Agent,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/status",
            aliases: &[],
            description: "Show agent status, task, and iteration count",
            usage: "/status",
            category: CommandCategory::Agent,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/compact",
            aliases: &[],
            description: "Compress conversation context to free memory",
            usage: "/compact",
            category: CommandCategory::Agent,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/context",
            aliases: &[],
            description: "Show context window usage breakdown",
            usage: "/context",
            category: CommandCategory::Agent,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/memory",
            aliases: &[],
            description: "Show memory system stats",
            usage: "/memory",
            category: CommandCategory::Agent,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/pin",
            aliases: &[],
            description: "Pin message #n (survives compression) or list pinned",
            usage: "/pin [n]",
            category: CommandCategory::Agent,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/unpin",
            aliases: &[],
            description: "Unpin message #n",
            usage: "/unpin <n>",
            category: CommandCategory::Agent,
            tui_only: false,
            detailed_help: None,
        });

        // Safety commands
        self.register(CommandInfo {
            name: "/safety",
            aliases: &[],
            description: "Show current safety mode and stats",
            usage: "/safety",
            category: CommandCategory::Safety,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/permissions",
            aliases: &[],
            description: "View or set approval mode (safe/cautious/paranoid/yolo)",
            usage: "/permissions [mode]",
            category: CommandCategory::Safety,
            tui_only: false,
            detailed_help: Some("Control how the agent asks for permission before executing actions.\n\nModes:\n  safe     - Auto-approve read-only operations, ask for writes/executes (default)\n  cautious - Auto-approve reads and reversible writes, ask for executes\n  paranoid - Ask for approval on every single action\n  yolo     - Auto-approve everything (use with caution!)\n\nExamples:\n  /permissions          - Show current mode\n  /permissions cautious - Switch to cautious mode"),
        });
        self.register(CommandInfo {
            name: "/audit",
            aliases: &[],
            description: "Show, query, export, or verify audit trail",
            usage: "/audit [show [n] | verify | export [fmt] | query <tool>]",
            category: CommandCategory::Safety,
            tui_only: false,
            detailed_help: None,
        });

        // Development commands
        self.register(CommandInfo {
            name: "/undo",
            aliases: &[],
            description: "Undo last file operation via git checkpoint",
            usage: "/undo",
            category: CommandCategory::Development,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/diff",
            aliases: &[],
            description: "Show recent file changes",
            usage: "/diff",
            category: CommandCategory::Development,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/review",
            aliases: &[],
            description: "Review all session file changes",
            usage: "/review",
            category: CommandCategory::Development,
            tui_only: false,
            detailed_help: None,
        });

        // System commands
        self.register(CommandInfo {
            name: "/help",
            aliases: &["/?"],
            description: "Show help (use /help <topic> for details)",
            usage: "/help [topic]",
            category: CommandCategory::System,
            tui_only: false,
            detailed_help: Some("Show all commands or detailed help for a specific topic.\n\nExamples:\n  /help           - Show all available commands\n  /help safety    - Show safety-related commands and explanation\n  /help session   - Show session management commands\n  /help compact   - Show help for /compact command\n\nTopics match command names (without /) or category names."),
        });
        self.register(CommandInfo {
            name: "/config",
            aliases: &[],
            description: "View or modify runtime configuration",
            usage: "/config [key] [value]",
            category: CommandCategory::System,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/doctor",
            aliases: &[],
            description: "Run diagnostic checks (LLM, tools, workspace)",
            usage: "/doctor",
            category: CommandCategory::System,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/setup",
            aliases: &[],
            description: "Re-run provider setup wizard",
            usage: "/setup",
            category: CommandCategory::System,
            tui_only: false,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/workflows",
            aliases: &[],
            description: "List available workflow templates",
            usage: "/workflows",
            category: CommandCategory::System,
            tui_only: false,
            detailed_help: None,
        });

        // Trust command
        self.register(CommandInfo {
            name: "/trust",
            aliases: &[],
            description: "Show safety trust dashboard with per-tool approval stats",
            usage: "/trust",
            category: CommandCategory::Safety,
            tui_only: false,
            detailed_help: Some("Display a trust calibration dashboard showing:\n  - Current approval mode with plain-English explanation\n  - Per-tool approval/denial statistics from the audit log\n  - Suggestions for adjusting trust based on your usage patterns\n\nThe dashboard helps you understand why you are being prompted and\nmake informed decisions about adjusting your approval mode."),
        });

        // Keys command
        self.register(CommandInfo {
            name: "/keys",
            aliases: &[],
            description: "Show keyboard shortcuts (TUI: F1 for overlay)",
            usage: "/keys",
            category: CommandCategory::System,
            tui_only: false,
            detailed_help: Some("Show all keyboard shortcuts grouped by context.\n\nIn TUI mode, press F1 for a floating overlay.\n\nGlobal:    Ctrl+C/D quit, Ctrl+L scroll to bottom\nOverlays:  Ctrl+E explanation panel, Ctrl+T task board\nApproval:  y=approve, n=deny, a=approve all, d=diff, ?=help\nVim mode:  i/a=insert, Esc=normal, /=search, q=quit"),
        });

        // TUI-only commands
        self.register(CommandInfo {
            name: "/theme",
            aliases: &[],
            description: "Switch color theme",
            usage: "/theme dark|light",
            category: CommandCategory::Ui,
            tui_only: true,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/sidebar",
            aliases: &[],
            description: "Toggle sidebar visibility",
            usage: "/sidebar",
            category: CommandCategory::Ui,
            tui_only: true,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/vim",
            aliases: &[],
            description: "Toggle vim mode",
            usage: "/vim",
            category: CommandCategory::Ui,
            tui_only: true,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/save",
            aliases: &[],
            description: "Save session (shorthand for /session save)",
            usage: "/save <name>",
            category: CommandCategory::Session,
            tui_only: true,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/load",
            aliases: &[],
            description: "Load session (shorthand for /session load)",
            usage: "/load <name>",
            category: CommandCategory::Session,
            tui_only: true,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/analytics",
            aliases: &[],
            description: "Show session analytics dashboard",
            usage: "/analytics",
            category: CommandCategory::Agent,
            tui_only: true,
            detailed_help: None,
        });
        self.register(CommandInfo {
            name: "/replay",
            aliases: &[],
            description: "Replay execution traces",
            usage: "/replay [next|prev|timeline|reset]",
            category: CommandCategory::Agent,
            tui_only: true,
            detailed_help: None,
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
                let tui_marker = if cmd.tui_only { " (TUI)" } else { "" };
                output.push_str(&format!(
                    "    {:<24} {}{}{}\n",
                    cmd.usage, cmd.description, aliases, tui_marker
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
            if cmd.tui_only {
                output.push_str("Note: This command is only available in TUI mode.\n");
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
                    let tui_marker = if cmd.tui_only { " (TUI)" } else { "" };
                    output.push_str(&format!(
                        "  {:<24} {}{}\n",
                        cmd.usage, cmd.description, tui_marker
                    ));
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
        // We have 31 commands registered (24 core + 7 TUI-only)
        assert!(
            registry.len() >= 31,
            "Expected at least 31 commands, got {}",
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
    fn test_tui_only_commands_exist() {
        let registry = CommandRegistry::with_defaults();
        for name in &[
            "/theme",
            "/sidebar",
            "/vim",
            "/save",
            "/load",
            "/analytics",
            "/replay",
        ] {
            let cmd = registry.lookup(name);
            assert!(cmd.is_some(), "Missing TUI command: {}", name);
            assert!(cmd.unwrap().tui_only, "{} should be TUI-only", name);
        }
    }

    #[test]
    fn test_core_commands_not_tui_only() {
        let registry = CommandRegistry::with_defaults();
        for name in &["/help", "/quit", "/compact", "/status", "/config"] {
            let cmd = registry.lookup(name);
            assert!(cmd.is_some(), "Missing core command: {}", name);
            assert!(!cmd.unwrap().tui_only, "{} should NOT be TUI-only", name);
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
        assert!(count >= 3, "UI category should have at least 3 commands");
    }

    #[test]
    fn test_help_text_marks_tui_commands() {
        let registry = CommandRegistry::with_defaults();
        let help = registry.help_text();
        assert!(
            help.contains("(TUI)"),
            "Help text should contain (TUI) markers for TUI-only commands"
        );
    }
}
