//! CLI subcommand handlers.

use crate::Commands;
use crate::ConfigAction;
use std::path::Path;

/// Handle a CLI subcommand.
pub async fn handle_command(command: Commands, workspace: &Path) -> anyhow::Result<()> {
    match command {
        Commands::Config { action } => handle_config(action, workspace).await,
    }
}

async fn handle_config(action: ConfigAction, workspace: &Path) -> anyhow::Result<()> {
    match action {
        ConfigAction::Init => {
            let config_dir = workspace.join(".rustant");
            std::fs::create_dir_all(&config_dir)?;

            let config_path = config_dir.join("config.toml");
            if config_path.exists() {
                println!(
                    "Configuration file already exists at: {}",
                    config_path.display()
                );
                return Ok(());
            }

            let default_config = rustant_core::AgentConfig::default();
            let toml_str = toml::to_string_pretty(&default_config)?;
            std::fs::write(&config_path, &toml_str)?;
            println!(
                "Created default configuration at: {}",
                config_path.display()
            );
            Ok(())
        }
        ConfigAction::Show => {
            let config = rustant_core::config::load_config(Some(workspace), None)
                .map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))?;
            let toml_str = toml::to_string_pretty(&config)?;
            println!("{}", toml_str);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_config_init_creates_file() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();

        let command = Commands::Config {
            action: ConfigAction::Init,
        };
        handle_command(command, workspace).await.unwrap();

        let config_path = workspace.join(".rustant").join("config.toml");
        assert!(config_path.exists());

        // Verify it's valid TOML
        let content = std::fs::read_to_string(&config_path).unwrap();
        let parsed: rustant_core::AgentConfig = toml::from_str(&content).unwrap();
        assert_eq!(parsed.llm.model, "gpt-4o");
        assert_eq!(
            parsed.safety.approval_mode,
            rustant_core::ApprovalMode::Safe
        );
    }

    #[tokio::test]
    async fn test_config_init_idempotent() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();

        // First init
        let command = Commands::Config {
            action: ConfigAction::Init,
        };
        handle_command(command, workspace).await.unwrap();

        let config_path = workspace.join(".rustant").join("config.toml");
        let content_first = std::fs::read_to_string(&config_path).unwrap();

        // Second init should not overwrite
        let command = Commands::Config {
            action: ConfigAction::Init,
        };
        handle_command(command, workspace).await.unwrap();

        let content_second = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(content_first, content_second);
    }

    #[tokio::test]
    async fn test_config_show_defaults() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();

        // Show should work even without a config file (uses defaults)
        let command = Commands::Config {
            action: ConfigAction::Show,
        };
        let result = handle_command(command, workspace).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_config_show_after_init() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path();

        // Init first
        let init_cmd = Commands::Config {
            action: ConfigAction::Init,
        };
        handle_command(init_cmd, workspace).await.unwrap();

        // Show should work with the config file present
        let show_cmd = Commands::Config {
            action: ConfigAction::Show,
        };
        let result = handle_command(show_cmd, workspace).await;
        assert!(result.is_ok());
    }
}
