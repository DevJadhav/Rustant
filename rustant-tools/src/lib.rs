//! # Rustant Tools
//!
//! Built-in tool implementations for the Rustant agent.
//! Provides file operations, search, git integration, and shell execution.

#[cfg(target_os = "macos")]
pub mod accessibility;
pub mod arxiv;
pub mod arxiv_api;
pub mod browser;
pub mod canvas;
pub mod career_intel;
pub mod checkpoint;
pub mod code_intelligence;
pub mod codebase_search;
pub mod compress;
pub mod content_engine;
pub mod database;
pub mod dev_server;
pub mod experiment_tracker;
pub mod paper_sources;

#[cfg(target_os = "macos")]
pub mod contacts;
#[cfg(target_os = "macos")]
pub mod daily_briefing;
pub mod file;
pub mod file_organizer;
pub mod finance;
pub mod flashcards;
pub mod git;
#[cfg(target_os = "macos")]
pub mod gui_scripting;
#[cfg(target_os = "macos")]
pub mod homekit;
pub mod http_api;
pub mod imessage;
pub mod inbox;
pub mod knowledge_graph;
pub mod life_planner;
pub mod lint;
pub mod lsp;
#[macro_use]
pub mod macros;
#[cfg(target_os = "macos")]
pub mod macos;
#[cfg(target_os = "macos")]
pub mod meeting;
pub mod pdf_generate;
#[cfg(target_os = "macos")]
pub mod photos;
pub mod pomodoro;
pub mod privacy_manager;
pub mod registry;
pub mod relationships;
#[cfg(target_os = "macos")]
pub mod safari;
pub mod sandbox;
pub mod scaffold;
#[cfg(target_os = "macos")]
pub mod screen_analyze;
pub mod self_improvement;
pub mod shell;
#[cfg(target_os = "macos")]
pub mod siri;
pub mod skill_tracker;
pub mod slack;
pub mod smart_edit;
pub mod system_monitor;
pub mod template;
pub mod templates;
pub mod test_runner;
pub mod travel;
pub mod utils;
#[cfg(target_os = "macos")]
pub mod voice_tool;
pub mod web;

use registry::{Tool, ToolRegistry};
use rustant_core::types::ProgressUpdate;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Register all built-in tools with the given workspace path.
pub fn register_builtin_tools(registry: &mut ToolRegistry, workspace: PathBuf) {
    register_builtin_tools_with_progress(registry, workspace, None);
}

/// Register all built-in tools, optionally with a progress channel for streaming output.
pub fn register_builtin_tools_with_progress(
    registry: &mut ToolRegistry,
    workspace: PathBuf,
    progress_tx: Option<mpsc::UnboundedSender<ProgressUpdate>>,
) {
    let shell_tool: Arc<dyn Tool> = if let Some(tx) = progress_tx {
        Arc::new(shell::ShellExecTool::with_progress(workspace.clone(), tx))
    } else {
        Arc::new(shell::ShellExecTool::new(workspace.clone()))
    };

    #[allow(unused_mut)]
    let mut tools: Vec<Arc<dyn Tool>> = vec![
        Arc::new(file::FileReadTool::new(workspace.clone())),
        Arc::new(file::FileListTool::new(workspace.clone())),
        Arc::new(file::FileSearchTool::new(workspace.clone())),
        Arc::new(file::FileWriteTool::new(workspace.clone())),
        Arc::new(file::FilePatchTool::new(workspace.clone())),
        Arc::new(git::GitStatusTool::new(workspace.clone())),
        Arc::new(git::GitDiffTool::new(workspace.clone())),
        Arc::new(git::GitCommitTool::new(workspace.clone())),
        shell_tool,
        Arc::new(utils::EchoTool),
        Arc::new(utils::DateTimeTool),
        Arc::new(utils::CalculatorTool),
        // Web tools — search, fetch, and document reading
        Arc::new(web::WebSearchTool::new()),
        Arc::new(web::WebFetchTool::new()),
        Arc::new(web::DocumentReadTool::new(workspace.clone())),
        // Smart editing with fuzzy matching and auto-checkpoint
        Arc::new(smart_edit::SmartEditTool::new(workspace.clone())),
        // Codebase search with auto-indexing
        Arc::new(codebase_search::CodebaseSearchTool::new(workspace.clone())),
        // Cross-platform utility tools
        Arc::new(file_organizer::FileOrganizerTool::new(workspace.clone())),
        Arc::new(compress::CompressTool::new(workspace.clone())),
        Arc::new(http_api::HttpApiTool::new()),
        Arc::new(template::TemplateTool::new(workspace.clone())),
        // PDF generation
        Arc::new(pdf_generate::PdfGenerateTool::new(workspace.clone())),
        // Personal productivity tools
        Arc::new(pomodoro::PomodoroTool::new(workspace.clone())),
        Arc::new(inbox::InboxTool::new(workspace.clone())),
        Arc::new(relationships::RelationshipsTool::new(workspace.clone())),
        // Life planner — energy-aware scheduling, deadlines, habits
        Arc::new(life_planner::LifePlannerTool::new(workspace.clone())),
        // Advanced personal tools
        Arc::new(finance::FinanceTool::new(workspace.clone())),
        Arc::new(flashcards::FlashcardsTool::new(workspace.clone())),
        Arc::new(travel::TravelTool::new(workspace.clone())),
        // Career intelligence
        Arc::new(career_intel::CareerIntelTool::new(workspace.clone())),
        // Research tools
        Arc::new(arxiv::ArxivResearchTool::new(workspace.clone())),
        // Cognitive extension tools
        Arc::new(knowledge_graph::KnowledgeGraphTool::new(workspace.clone())),
        Arc::new(experiment_tracker::ExperimentTrackerTool::new(
            workspace.clone(),
        )),
        Arc::new(code_intelligence::CodeIntelligenceTool::new(
            workspace.clone(),
        )),
        Arc::new(content_engine::ContentEngineTool::new(workspace.clone())),
        Arc::new(skill_tracker::SkillTrackerTool::new(workspace.clone())),
        Arc::new(system_monitor::SystemMonitorTool::new(workspace.clone())),
        Arc::new(privacy_manager::PrivacyManagerTool::new(workspace.clone())),
        Arc::new(self_improvement::SelfImprovementTool::new(
            workspace.clone(),
        )),
        // Slack tool — cross-platform, uses Slack Bot Token API
        Arc::new(slack::SlackTool::new(workspace.clone())),
        // Fullstack development tools
        Arc::new(scaffold::ScaffoldTool::new(workspace.clone())),
        Arc::new(dev_server::DevServerTool::new(workspace.clone())),
        Arc::new(database::DatabaseTool::new(workspace.clone())),
        Arc::new(test_runner::TestRunnerTool::new(workspace.clone())),
        Arc::new(lint::LintTool::new(workspace.clone())),
    ];

    // iMessage tools — macOS only
    #[cfg(target_os = "macos")]
    {
        tools.push(Arc::new(imessage::IMessageContactsTool));
        tools.push(Arc::new(imessage::IMessageSendTool));
        tools.push(Arc::new(imessage::IMessageReadTool));
    }

    // macOS native tools — Calendar, Reminders, Notes, App Control, etc.
    #[cfg(target_os = "macos")]
    {
        tools.push(Arc::new(macos::MacosCalendarTool));
        tools.push(Arc::new(macos::MacosRemindersTool));
        tools.push(Arc::new(macos::MacosNotesTool));
        tools.push(Arc::new(macos::MacosAppControlTool));
        tools.push(Arc::new(macos::MacosNotificationTool));
        tools.push(Arc::new(macos::MacosClipboardTool));
        tools.push(Arc::new(macos::MacosScreenshotTool));
        tools.push(Arc::new(macos::MacosSystemInfoTool));
        tools.push(Arc::new(macos::MacosSpotlightTool));
        tools.push(Arc::new(macos::MacosFinderTool));
        tools.push(Arc::new(macos::MacosFocusModeTool));
        tools.push(Arc::new(macos::MacosMailTool));
        tools.push(Arc::new(macos::MacosMusicTool));
        tools.push(Arc::new(macos::MacosShortcutsTool));
        tools.push(Arc::new(meeting::MacosMeetingRecorderTool));
        tools.push(Arc::new(daily_briefing::MacosDailyBriefingTool));
        tools.push(Arc::new(gui_scripting::MacosGuiScriptingTool));
        tools.push(Arc::new(accessibility::MacosAccessibilityTool));
        tools.push(Arc::new(screen_analyze::MacosScreenAnalyzeTool));
        tools.push(Arc::new(contacts::MacosContactsTool));
        tools.push(Arc::new(safari::MacosSafariTool));
        tools.push(Arc::new(voice_tool::MacosSayTool::new()));
        tools.push(Arc::new(photos::MacosPhotosTool::new()));
        tools.push(Arc::new(homekit::HomeKitTool::new()));
        tools.push(Arc::new(siri::SiriIntegrationTool));
    }

    for tool in tools {
        if let Err(e) = registry.register(tool) {
            tracing::warn!("Failed to register tool: {}", e);
        }
    }
}

/// Register all LSP tools backed by a shared [`lsp::LspManager`].
///
/// The LSP tools provide code intelligence capabilities (hover, definition,
/// references, diagnostics, completions, rename, format) by connecting to
/// language servers installed on the system.
pub fn register_lsp_tools(registry: &mut ToolRegistry, workspace: PathBuf) {
    let manager = Arc::new(lsp::LspManager::new(workspace));
    let lsp_tools = lsp::create_lsp_tools(manager);

    for tool in lsp_tools {
        if let Err(e) = registry.register(tool) {
            tracing::warn!("Failed to register LSP tool: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_register_all_builtin_tools() {
        let dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new();
        register_builtin_tools(&mut registry, dir.path().to_path_buf());

        // 45 base + 3 iMessage + 24 macOS native = 72 on macOS
        #[cfg(target_os = "macos")]
        assert_eq!(registry.len(), 73);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(registry.len(), 45);

        // Verify all expected tools are registered
        let names = registry.list_names();
        assert!(names.contains(&"file_read".to_string()));
        assert!(names.contains(&"file_list".to_string()));
        assert!(names.contains(&"file_search".to_string()));
        assert!(names.contains(&"file_write".to_string()));
        assert!(names.contains(&"file_patch".to_string()));
        assert!(names.contains(&"git_status".to_string()));
        assert!(names.contains(&"git_diff".to_string()));
        assert!(names.contains(&"git_commit".to_string()));
        assert!(names.contains(&"shell_exec".to_string()));
        assert!(names.contains(&"echo".to_string()));
        assert!(names.contains(&"datetime".to_string()));
        assert!(names.contains(&"calculator".to_string()));

        // iMessage tools on macOS
        #[cfg(target_os = "macos")]
        {
            assert!(names.contains(&"imessage_contacts".to_string()));
            assert!(names.contains(&"imessage_send".to_string()));
            assert!(names.contains(&"imessage_read".to_string()));
        }

        // macOS native tools
        #[cfg(target_os = "macos")]
        {
            assert!(names.contains(&"macos_calendar".to_string()));
            assert!(names.contains(&"macos_reminders".to_string()));
            assert!(names.contains(&"macos_notes".to_string()));
            assert!(names.contains(&"macos_app_control".to_string()));
            assert!(names.contains(&"macos_notification".to_string()));
            assert!(names.contains(&"macos_clipboard".to_string()));
            assert!(names.contains(&"macos_screenshot".to_string()));
            assert!(names.contains(&"macos_system_info".to_string()));
            assert!(names.contains(&"macos_spotlight".to_string()));
            assert!(names.contains(&"macos_finder".to_string()));
            assert!(names.contains(&"macos_focus_mode".to_string()));
            assert!(names.contains(&"macos_mail".to_string()));
            assert!(names.contains(&"macos_music".to_string()));
            assert!(names.contains(&"macos_shortcuts".to_string()));
            assert!(names.contains(&"macos_meeting_recorder".to_string()));
            assert!(names.contains(&"macos_daily_briefing".to_string()));
            assert!(names.contains(&"macos_gui_scripting".to_string()));
            assert!(names.contains(&"macos_accessibility".to_string()));
            assert!(names.contains(&"macos_screen_analyze".to_string()));
            assert!(names.contains(&"macos_contacts".to_string()));
            assert!(names.contains(&"macos_safari".to_string()));
            assert!(names.contains(&"macos_say".to_string()));
            assert!(names.contains(&"macos_photos".to_string()));
            assert!(names.contains(&"homekit".to_string()));
        }

        // Research tools
        assert!(names.contains(&"arxiv_research".to_string()));

        // Cognitive extension tools
        assert!(names.contains(&"knowledge_graph".to_string()));
        assert!(names.contains(&"experiment_tracker".to_string()));
        assert!(names.contains(&"code_intelligence".to_string()));
        assert!(names.contains(&"content_engine".to_string()));
        assert!(names.contains(&"skill_tracker".to_string()));
        assert!(names.contains(&"career_intel".to_string()));
        assert!(names.contains(&"system_monitor".to_string()));
        assert!(names.contains(&"life_planner".to_string()));
        assert!(names.contains(&"privacy_manager".to_string()));
        assert!(names.contains(&"self_improvement".to_string()));

        // Slack tool
        assert!(names.contains(&"slack".to_string()));

        // Cross-platform productivity tools
        assert!(names.contains(&"pomodoro".to_string()));
        assert!(names.contains(&"inbox".to_string()));
        assert!(names.contains(&"relationships".to_string()));
        assert!(names.contains(&"finance".to_string()));
        assert!(names.contains(&"flashcards".to_string()));
        assert!(names.contains(&"travel".to_string()));
    }

    #[test]
    fn test_tool_definitions_are_valid_json() {
        let dir = TempDir::new().unwrap();
        let mut registry = ToolRegistry::new();
        register_builtin_tools(&mut registry, dir.path().to_path_buf());

        let definitions = registry.list_definitions();
        for def in &definitions {
            assert!(!def.name.is_empty(), "Tool name should not be empty");
            assert!(
                !def.description.is_empty(),
                "Tool description should not be empty"
            );
            // Parameters should be a valid JSON object
            assert!(
                def.parameters.is_object(),
                "Parameters should be a JSON object for tool '{}'",
                def.name
            );
        }
    }
}
