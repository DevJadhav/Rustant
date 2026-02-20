//! macOS Shortcut generation for Siri integration.
//!
//! Generates Apple Shortcuts that connect Siri voice commands
//! to the Rustant daemon.

use std::path::PathBuf;

/// Generates macOS Shortcuts for Siri integration.
pub struct ShortcutGenerator {
    /// Path to the rustant binary.
    rustant_bin: PathBuf,
    /// Base directory for Rustant state.
    base_dir: PathBuf,
}

/// A generated shortcut definition.
#[derive(Debug, Clone)]
pub struct ShortcutDef {
    /// Human-readable name of the shortcut.
    pub name: String,
    /// Siri trigger phrase.
    pub trigger: String,
    /// Shell command to execute.
    pub command: String,
    /// Description of what the shortcut does.
    pub description: String,
}

impl ShortcutGenerator {
    /// Create a new shortcut generator.
    pub fn new(rustant_bin: PathBuf, base_dir: PathBuf) -> Self {
        Self {
            rustant_bin,
            base_dir,
        }
    }

    /// Generate all preset Siri shortcuts.
    pub fn generate_all(&self) -> Vec<ShortcutDef> {
        let bin = self.rustant_bin.display();
        let base = self.base_dir.display();

        vec![
            ShortcutDef {
                name: "Activate Rustant".into(),
                trigger: "Hey Siri, activate Rustant".into(),
                command: format!("{bin} daemon start --siri-mode"),
                description: "Starts the Rustant daemon and enables Siri routing.".into(),
            },
            ShortcutDef {
                name: "Deactivate Rustant".into(),
                trigger: "Hey Siri, deactivate Rustant".into(),
                command: format!("{bin} daemon stop --siri-mode"),
                description: "Disables Siri routing and optionally stops the daemon.".into(),
            },
            ShortcutDef {
                name: "Rustant Task".into(),
                trigger: "Hey Siri, ask Rustant".into(),
                command: format!(
                    "if [ -f {base}/siri_active ]; then {bin} siri send \"$INPUT\"; fi"
                ),
                description: "Routes voice input to Rustant if activated.".into(),
            },
            ShortcutDef {
                name: "Rustant Calendar".into(),
                trigger: "Hey Siri, check my calendar with Rustant".into(),
                command: format!(
                    "if [ -f {base}/siri_active ]; then {bin} siri send \"check my calendar\"; fi"
                ),
                description: "Check calendar via Rustant.".into(),
            },
            ShortcutDef {
                name: "Rustant Briefing".into(),
                trigger: "Hey Siri, Rustant briefing".into(),
                command: format!(
                    "if [ -f {base}/siri_active ]; then {bin} siri send \"what's my briefing\"; fi"
                ),
                description: "Get your morning/evening briefing.".into(),
            },
            ShortcutDef {
                name: "Rustant Security Scan".into(),
                trigger: "Hey Siri, Rustant security scan".into(),
                command: format!(
                    "if [ -f {base}/siri_active ]; then {bin} siri send \"run a security scan\"; fi"
                ),
                description: "Start a security scan via Rustant.".into(),
            },
            ShortcutDef {
                name: "Rustant Research".into(),
                trigger: "Hey Siri, Rustant research".into(),
                command: format!(
                    "if [ -f {base}/siri_active ]; then {bin} siri send \"research $INPUT\"; fi"
                ),
                description: "Start deep research on a topic.".into(),
            },
            ShortcutDef {
                name: "Rustant Status".into(),
                trigger: "Hey Siri, Rustant status".into(),
                command: format!("{bin} daemon status"),
                description: "Check Rustant daemon status.".into(),
            },
        ]
    }

    /// Format shortcuts as a human-readable installation guide.
    pub fn format_installation_guide(&self) -> String {
        let shortcuts = self.generate_all();
        let mut guide = String::from("# Siri Shortcuts Installation Guide\n\n");
        guide.push_str("Create the following shortcuts in the Shortcuts app:\n\n");

        for (i, shortcut) in shortcuts.iter().enumerate() {
            guide.push_str(&format!("## {}. {}\n\n", i + 1, shortcut.name));
            guide.push_str(&format!("**Trigger:** \"{}\"\n", shortcut.trigger));
            guide.push_str(&format!(
                "**Action:** Run Shell Script:\n```\n{}\n```\n",
                shortcut.command
            ));
            guide.push_str(&format!("**Description:** {}\n\n", shortcut.description));
        }

        guide.push_str("---\n\n");
        guide.push_str("Each shortcut should:\n");
        guide.push_str("1. Add a \"Run Shell Script\" action\n");
        guide.push_str("2. Paste the command above\n");
        guide.push_str(
            "3. For shortcuts with $INPUT, add \"Ask for Input\" before the shell action\n",
        );

        guide
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_all() {
        let generator = ShortcutGenerator::new(
            PathBuf::from("/usr/local/bin/rustant"),
            PathBuf::from("/Users/test/.rustant"),
        );
        let shortcuts = generator.generate_all();
        assert!(shortcuts.len() >= 8);

        // Check activation shortcut
        assert_eq!(shortcuts[0].name, "Activate Rustant");
        assert!(shortcuts[0].command.contains("daemon start"));
    }

    #[test]
    fn test_installation_guide() {
        let generator = ShortcutGenerator::new(
            PathBuf::from("/usr/local/bin/rustant"),
            PathBuf::from("/Users/test/.rustant"),
        );
        let guide = generator.format_installation_guide();
        assert!(guide.contains("Activate Rustant"));
        assert!(guide.contains("Run Shell Script"));
    }
}
