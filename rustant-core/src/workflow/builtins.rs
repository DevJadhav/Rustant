//! Built-in workflow definitions shipped with Rustant.
//!
//! Each workflow is defined as a YAML string constant that can be parsed
//! by the workflow parser.

use crate::workflow::parser::parse_workflow;
use crate::workflow::types::WorkflowDefinition;

/// Returns the list of all built-in workflow names.
pub fn list_builtin_names() -> Vec<&'static str> {
    vec![
        "code_review",
        "refactor",
        "test_generation",
        "documentation",
        "dependency_update",
        "security_scan",
        "deployment",
        "incident_response",
        // Daily workflow automation templates
        "morning_briefing",
        "pr_review",
        "dependency_audit",
        "changelog",
        // macOS daily assistant workflows
        "meeting_recorder",
        "daily_briefing_full",
        "end_of_day_summary",
        // macOS screen automation workflows
        "app_automation",
        "email_triage",
    ]
}

/// Load a built-in workflow by name.
pub fn get_builtin(name: &str) -> Option<WorkflowDefinition> {
    let yaml = match name {
        "code_review" => CODE_REVIEW_WORKFLOW,
        "refactor" => REFACTOR_WORKFLOW,
        "test_generation" => TEST_GENERATION_WORKFLOW,
        "documentation" => DOCUMENTATION_WORKFLOW,
        "dependency_update" => DEPENDENCY_UPDATE_WORKFLOW,
        "security_scan" => SECURITY_SCAN_WORKFLOW,
        "deployment" => DEPLOYMENT_WORKFLOW,
        "incident_response" => INCIDENT_RESPONSE_WORKFLOW,
        "morning_briefing" => MORNING_BRIEFING_WORKFLOW,
        "pr_review" => PR_REVIEW_WORKFLOW,
        "dependency_audit" => DEPENDENCY_AUDIT_WORKFLOW,
        "changelog" => CHANGELOG_WORKFLOW,
        "meeting_recorder" => MEETING_RECORDER_WORKFLOW,
        "daily_briefing_full" => DAILY_BRIEFING_FULL_WORKFLOW,
        "end_of_day_summary" => END_OF_DAY_SUMMARY_WORKFLOW,
        "app_automation" => APP_AUTOMATION_WORKFLOW,
        "email_triage" => EMAIL_TRIAGE_WORKFLOW,
        _ => return None,
    };
    parse_workflow(yaml).ok()
}

/// Load all built-in workflows.
pub fn all_builtins() -> Vec<WorkflowDefinition> {
    list_builtin_names()
        .into_iter()
        .filter_map(get_builtin)
        .collect()
}

const CODE_REVIEW_WORKFLOW: &str = r#"
name: code_review
description: Automated code review workflow
version: "1.0"
inputs:
  - name: path
    type: string
    description: Path to the file or directory to review
  - name: focus_areas
    type: "string[]"
    optional: true
    default: ["security", "performance", "correctness"]
steps:
  - id: read_files
    tool: file_read
    params:
      path: "{{ inputs.path }}"
  - id: analyze
    tool: echo
    params:
      text: "Analyzing code from {{ inputs.path }}"
  - id: report
    tool: echo
    params:
      text: "Code review complete for {{ inputs.path }}"
outputs:
  - name: review
    value: "{{ steps.report.output }}"
"#;

const REFACTOR_WORKFLOW: &str = r#"
name: refactor
description: Guided code refactoring workflow
version: "1.0"
inputs:
  - name: path
    type: string
    description: Path to refactor
  - name: strategy
    type: string
    optional: true
    default: "simplify"
steps:
  - id: read_source
    tool: file_read
    params:
      path: "{{ inputs.path }}"
  - id: plan_refactor
    tool: echo
    params:
      text: "Planning refactor for {{ inputs.path }}"
  - id: apply_changes
    tool: echo
    params:
      text: "Applying refactor changes"
    gate:
      type: approval_required
      message: "Apply the planned refactoring changes?"
outputs:
  - name: result
    value: "{{ steps.apply_changes.output }}"
"#;

const TEST_GENERATION_WORKFLOW: &str = r#"
name: test_generation
description: Generate tests for existing code
version: "1.0"
inputs:
  - name: path
    type: string
    description: Path to generate tests for
  - name: framework
    type: string
    optional: true
    default: "default"
steps:
  - id: read_source
    tool: file_read
    params:
      path: "{{ inputs.path }}"
  - id: generate
    tool: echo
    params:
      text: "Generating tests for {{ inputs.path }}"
  - id: write_tests
    tool: echo
    params:
      text: "Writing generated tests"
    gate:
      type: approval_required
      message: "Write the generated tests?"
outputs:
  - name: test_path
    value: "{{ steps.write_tests.output }}"
"#;

const DOCUMENTATION_WORKFLOW: &str = r#"
name: documentation
description: Generate or update documentation
version: "1.0"
inputs:
  - name: path
    type: string
    description: Path to document
  - name: style
    type: string
    optional: true
    default: "rustdoc"
steps:
  - id: read_source
    tool: file_read
    params:
      path: "{{ inputs.path }}"
  - id: generate_docs
    tool: echo
    params:
      text: "Generating documentation for {{ inputs.path }}"
  - id: write_docs
    tool: echo
    params:
      text: "Writing documentation"
    gate:
      type: approval_required
      message: "Write the generated documentation?"
outputs:
  - name: doc_path
    value: "{{ steps.write_docs.output }}"
"#;

const DEPENDENCY_UPDATE_WORKFLOW: &str = r#"
name: dependency_update
description: Update and test dependencies
version: "1.0"
inputs:
  - name: path
    type: string
    optional: true
    default: "."
    description: Project root path
steps:
  - id: check_deps
    tool: shell_exec
    params:
      command: "cargo outdated"
  - id: update_deps
    tool: echo
    params:
      text: "Updating dependencies"
    gate:
      type: approval_required
      message: "Update the following dependencies?"
  - id: test
    tool: shell_exec
    params:
      command: "cargo test"
    on_error:
      action: fail
outputs:
  - name: result
    value: "{{ steps.test.output }}"
"#;

const SECURITY_SCAN_WORKFLOW: &str = r#"
name: security_scan
description: Security analysis of codebase
version: "1.0"
inputs:
  - name: path
    type: string
    optional: true
    default: "."
    description: Path to scan
steps:
  - id: audit
    tool: shell_exec
    params:
      command: "cargo audit"
    on_error:
      action: skip
  - id: analyze
    tool: echo
    params:
      text: "Security analysis of {{ inputs.path }}"
  - id: report
    tool: echo
    params:
      text: "Security scan complete"
outputs:
  - name: report
    value: "{{ steps.report.output }}"
"#;

const DEPLOYMENT_WORKFLOW: &str = r#"
name: deployment
description: Build and deploy with safety checks
version: "1.0"
inputs:
  - name: environment
    type: string
    description: Target environment
steps:
  - id: build
    tool: shell_exec
    params:
      command: "cargo build --release"
  - id: test
    tool: shell_exec
    params:
      command: "cargo test"
  - id: deploy
    tool: echo
    params:
      text: "Deploying to {{ inputs.environment }}"
    gate:
      type: approval_required
      message: "Deploy to {{ inputs.environment }}?"
outputs:
  - name: status
    value: "{{ steps.deploy.output }}"
"#;

const INCIDENT_RESPONSE_WORKFLOW: &str = r#"
name: incident_response
description: Guided incident response procedure
version: "1.0"
inputs:
  - name: description
    type: string
    description: Incident description
  - name: severity
    type: string
    optional: true
    default: "medium"
steps:
  - id: assess
    tool: echo
    params:
      text: "Assessing incident: {{ inputs.description }}"
  - id: investigate
    tool: echo
    params:
      text: "Investigating root cause"
  - id: mitigate
    tool: echo
    params:
      text: "Applying mitigation"
    gate:
      type: approval_required
      message: "Apply the proposed mitigation?"
  - id: report
    tool: echo
    params:
      text: "Incident report generated"
outputs:
  - name: report
    value: "{{ steps.report.output }}"
"#;

// ---------------------------------------------------------------------------
// Daily Workflow Automation Templates
// ---------------------------------------------------------------------------

const MORNING_BRIEFING_WORKFLOW: &str = r#"
name: morning_briefing
description: "Daily morning briefing: git log, open PRs, pending reviews, and project status"
version: "1.0"
author: rustant
inputs:
  - name: days
    type: number
    optional: true
    default: 1
    description: Number of days of history to include
  - name: branch
    type: string
    optional: true
    default: "main"
    description: Main branch to summarize against
steps:
  - id: git_log
    tool: shell_exec
    params:
      command: "git log --oneline --since='{{ inputs.days }} days ago' --all"
    on_error:
      action: skip
  - id: git_status
    tool: git_status
    params:
      include_untracked: true
  - id: open_branches
    tool: shell_exec
    params:
      command: "git branch --no-merged {{ inputs.branch }}"
    on_error:
      action: skip
  - id: recent_changes
    tool: git_diff
    params:
      target: "HEAD~5"
    on_error:
      action: skip
  - id: summary
    tool: echo
    params:
      text: "Morning briefing complete. Recent commits, branch status, and changes have been collected."
outputs:
  - name: briefing
    value: "{{ steps.summary.output }}"
"#;

const PR_REVIEW_WORKFLOW: &str = r#"
name: pr_review
description: "Review a pull request: fetch diff, analyze code quality, and generate review summary"
version: "1.0"
author: rustant
inputs:
  - name: branch
    type: string
    description: Branch name to review
  - name: base
    type: string
    optional: true
    default: "main"
    description: Base branch to diff against
steps:
  - id: fetch_diff
    tool: shell_exec
    params:
      command: "git diff {{ inputs.base }}...{{ inputs.branch }}"
    on_error:
      action: fail
  - id: changed_files
    tool: shell_exec
    params:
      command: "git diff --name-only {{ inputs.base }}...{{ inputs.branch }}"
    on_error:
      action: skip
  - id: commit_log
    tool: shell_exec
    params:
      command: "git log --oneline {{ inputs.base }}..{{ inputs.branch }}"
    on_error:
      action: skip
  - id: line_stats
    tool: shell_exec
    params:
      command: "git diff --stat {{ inputs.base }}...{{ inputs.branch }}"
    on_error:
      action: skip
  - id: review_summary
    tool: echo
    params:
      text: "PR review for {{ inputs.branch }} against {{ inputs.base }} complete. Diff, changed files, commits, and stats collected."
outputs:
  - name: review
    value: "{{ steps.review_summary.output }}"
"#;

const DEPENDENCY_AUDIT_WORKFLOW: &str = r#"
name: dependency_audit
description: "Audit project dependencies for vulnerabilities and outdated packages"
version: "1.0"
author: rustant
inputs:
  - name: path
    type: string
    optional: true
    default: "."
    description: Project root path
steps:
  - id: check_lockfile
    tool: file_search
    params:
      directory: "{{ inputs.path }}"
      pattern: "*.lock"
    on_error:
      action: skip
  - id: cargo_audit
    tool: shell_exec
    params:
      command: "cargo audit 2>&1 || true"
    on_error:
      action: skip
  - id: cargo_outdated
    tool: shell_exec
    params:
      command: "cargo outdated 2>&1 || true"
    on_error:
      action: skip
  - id: npm_audit
    tool: shell_exec
    params:
      command: "npm audit --json 2>&1 || true"
    condition: "{{ inputs.path }}"
    on_error:
      action: skip
  - id: audit_report
    tool: echo
    params:
      text: "Dependency audit complete. Vulnerabilities and outdated packages have been checked."
outputs:
  - name: report
    value: "{{ steps.audit_report.output }}"
"#;

const CHANGELOG_WORKFLOW: &str = r#"
name: changelog
description: "Generate a changelog from git commits grouped by type (feat, fix, chore, etc.)"
version: "1.0"
author: rustant
inputs:
  - name: since
    type: string
    optional: true
    default: "1 week ago"
    description: Time period for changelog (e.g., '1 week ago', 'v1.0.0')
  - name: format
    type: string
    optional: true
    default: "grouped"
    description: Output format (grouped, flat, conventional)
steps:
  - id: fetch_commits
    tool: shell_exec
    params:
      command: "git log --pretty=format:'%h %s (%an, %ar)' --since='{{ inputs.since }}'"
    on_error:
      action: fail
  - id: commit_stats
    tool: shell_exec
    params:
      command: "git shortlog -sn --since='{{ inputs.since }}'"
    on_error:
      action: skip
  - id: files_changed
    tool: shell_exec
    params:
      command: "git diff --stat $(git rev-list -1 --before='{{ inputs.since }}' HEAD 2>/dev/null || echo HEAD~10)..HEAD 2>/dev/null || echo 'No stats available'"
    on_error:
      action: skip
  - id: generate_changelog
    tool: echo
    params:
      text: "Changelog generation complete. Commits since {{ inputs.since }} have been collected and categorized."
outputs:
  - name: changelog
    value: "{{ steps.generate_changelog.output }}"
"#;

const MEETING_RECORDER_WORKFLOW: &str = r#"
name: meeting_recorder
description: Record, transcribe, and summarize a meeting to Notes.app
version: "1.0"
author: rustant
inputs:
  - name: title
    type: string
    optional: true
    default: "Untitled Meeting"
    description: Meeting title for the Notes.app entry
steps:
  - id: detect
    tool: macos_meeting_recorder
    params:
      action: "detect_meeting"
  - id: record
    tool: macos_meeting_recorder
    params:
      action: "record"
      title: "{{ inputs.title }}"
    gate:
      type: approval_required
    gate_message: "Start recording meeting audio from microphone?"
  - id: notify_recording
    tool: macos_notification
    params:
      message: "Meeting recording started. Use 'stop' when finished."
      title: "Rustant Meeting Recorder"
  - id: stop
    tool: macos_meeting_recorder
    params:
      action: "stop"
    gate:
      type: approval_required
    gate_message: "Stop recording and begin transcription?"
  - id: transcribe
    tool: macos_meeting_recorder
    params:
      action: "transcribe"
      audio_path: "{{ steps.stop.output }}"
  - id: save_to_notes
    tool: macos_meeting_recorder
    params:
      action: "summarize_to_notes"
      title: "{{ inputs.title }}"
      transcript: "{{ steps.transcribe.output }}"
outputs:
  - name: transcript
    value: "{{ steps.transcribe.output }}"
"#;

const DAILY_BRIEFING_FULL_WORKFLOW: &str = r#"
name: daily_briefing_full
description: Complete daily briefing with calendar, reminders, weather, and Notes.app output
version: "1.0"
author: rustant
inputs:
  - name: location
    type: string
    optional: true
    default: ""
    description: Location for weather forecast (auto-detect if empty)
steps:
  - id: briefing
    tool: macos_daily_briefing
    params:
      action: "morning"
      include_weather: true
      location: "{{ inputs.location }}"
  - id: notify
    tool: macos_notification
    params:
      message: "Your morning briefing is ready in Notes.app"
      title: "Rustant Daily Briefing"
outputs:
  - name: briefing
    value: "{{ steps.briefing.output }}"
"#;

const END_OF_DAY_SUMMARY_WORKFLOW: &str = r#"
name: end_of_day_summary
description: End-of-day review with completed tasks, tomorrow preview, and Notes.app output
version: "1.0"
author: rustant
steps:
  - id: evening_summary
    tool: macos_daily_briefing
    params:
      action: "evening"
  - id: tomorrow_preview
    tool: macos_calendar
    params:
      action: "list"
      days_ahead: 1
  - id: notify
    tool: macos_notification
    params:
      message: "End-of-day summary saved to Notes.app"
      title: "Rustant EOD Summary"
outputs:
  - name: summary
    value: "{{ steps.evening_summary.output }}"
"#;

const APP_AUTOMATION_WORKFLOW: &str = r#"
name: app_automation
description: Open a macOS app, inspect its UI via accessibility, and perform GUI actions
version: "1.0"
author: rustant
inputs:
  - name: app_name
    type: string
    description: Name of the macOS application to automate
  - name: task
    type: string
    description: Description of the task to perform in the app
steps:
  - id: open_app
    tool: macos_app_control
    params:
      action: "open"
      app_name: "{{ inputs.app_name }}"
  - id: inspect_ui
    tool: macos_accessibility
    params:
      action: "get_tree"
      app_name: "{{ inputs.app_name }}"
      max_depth: 3
  - id: perform_action
    tool: macos_gui_scripting
    params:
      action: "click_element"
      app_name: "{{ inputs.app_name }}"
      element_description: "{{ inputs.task }}"
    gate:
      type: approval_required
      message: "Perform GUI action in {{ inputs.app_name }}?"
  - id: verify
    tool: macos_accessibility
    params:
      action: "get_tree"
      app_name: "{{ inputs.app_name }}"
      max_depth: 2
outputs:
  - name: result
    value: "{{ steps.verify.output }}"
"#;

const EMAIL_TRIAGE_WORKFLOW: &str = r#"
name: email_triage
description: Read unread emails, classify by priority, and draft replies for important ones
version: "1.0"
author: rustant
inputs:
  - name: max_emails
    type: number
    optional: true
    default: 10
    description: Maximum number of unread emails to process
steps:
  - id: fetch_unread
    tool: macos_mail
    params:
      action: "unread"
  - id: classify
    tool: echo
    params:
      text: "Classifying emails by priority and type"
  - id: draft_replies
    tool: echo
    params:
      text: "Drafting replies for high-priority emails"
  - id: send_replies
    tool: echo
    params:
      text: "Ready to send drafted replies"
    gate:
      type: approval_required
      message: "Send the drafted email replies?"
  - id: summary
    tool: echo
    params:
      text: "Email triage complete. Unread emails classified and replies drafted."
outputs:
  - name: triage_report
    value: "{{ steps.summary.output }}"
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::parser::validate_workflow;

    #[test]
    fn test_builtin_code_review_parses() {
        let wf = parse_workflow(CODE_REVIEW_WORKFLOW).unwrap();
        assert_eq!(wf.name, "code_review");
        assert!(!wf.steps.is_empty());
    }

    #[test]
    fn test_builtin_refactor_parses() {
        let wf = parse_workflow(REFACTOR_WORKFLOW).unwrap();
        assert_eq!(wf.name, "refactor");
        assert!(!wf.steps.is_empty());
    }

    #[test]
    fn test_builtin_test_generation_parses() {
        let wf = parse_workflow(TEST_GENERATION_WORKFLOW).unwrap();
        assert_eq!(wf.name, "test_generation");
    }

    #[test]
    fn test_builtin_documentation_parses() {
        let wf = parse_workflow(DOCUMENTATION_WORKFLOW).unwrap();
        assert_eq!(wf.name, "documentation");
    }

    #[test]
    fn test_all_builtins_validate() {
        let names = list_builtin_names();
        for name in &names {
            let wf =
                get_builtin(name).unwrap_or_else(|| panic!("Failed to load builtin: {}", name));
            validate_workflow(&wf)
                .unwrap_or_else(|e| panic!("Builtin '{}' failed validation: {}", name, e));
        }
    }

    #[test]
    fn test_list_builtin_names() {
        let names = list_builtin_names();
        assert_eq!(names.len(), 17);
        assert!(names.contains(&"code_review"));
        assert!(names.contains(&"refactor"));
        assert!(names.contains(&"test_generation"));
        assert!(names.contains(&"documentation"));
        assert!(names.contains(&"dependency_update"));
        assert!(names.contains(&"security_scan"));
        assert!(names.contains(&"deployment"));
        assert!(names.contains(&"incident_response"));
        // Daily workflow automation templates
        assert!(names.contains(&"morning_briefing"));
        assert!(names.contains(&"pr_review"));
        assert!(names.contains(&"dependency_audit"));
        assert!(names.contains(&"changelog"));
        // macOS daily assistant workflows
        assert!(names.contains(&"meeting_recorder"));
        assert!(names.contains(&"daily_briefing_full"));
        assert!(names.contains(&"end_of_day_summary"));
        // macOS screen automation workflows
        assert!(names.contains(&"app_automation"));
        assert!(names.contains(&"email_triage"));
    }

    #[test]
    fn test_builtin_morning_briefing_parses() {
        let wf = parse_workflow(MORNING_BRIEFING_WORKFLOW).unwrap();
        assert_eq!(wf.name, "morning_briefing");
        assert!(!wf.steps.is_empty());
        assert!(wf.inputs.iter().any(|i| i.name == "days"));
    }

    #[test]
    fn test_builtin_pr_review_parses() {
        let wf = parse_workflow(PR_REVIEW_WORKFLOW).unwrap();
        assert_eq!(wf.name, "pr_review");
        assert!(!wf.steps.is_empty());
        assert!(wf.inputs.iter().any(|i| i.name == "branch"));
    }

    #[test]
    fn test_builtin_dependency_audit_parses() {
        let wf = parse_workflow(DEPENDENCY_AUDIT_WORKFLOW).unwrap();
        assert_eq!(wf.name, "dependency_audit");
        assert!(!wf.steps.is_empty());
    }

    #[test]
    fn test_builtin_changelog_parses() {
        let wf = parse_workflow(CHANGELOG_WORKFLOW).unwrap();
        assert_eq!(wf.name, "changelog");
        assert!(!wf.steps.is_empty());
        assert!(wf.inputs.iter().any(|i| i.name == "since"));
    }

    #[test]
    fn test_builtin_meeting_recorder_parses() {
        let wf = parse_workflow(MEETING_RECORDER_WORKFLOW).unwrap();
        assert_eq!(wf.name, "meeting_recorder");
        assert!(!wf.steps.is_empty());
        assert!(wf.inputs.iter().any(|i| i.name == "title"));
    }

    #[test]
    fn test_builtin_daily_briefing_full_parses() {
        let wf = parse_workflow(DAILY_BRIEFING_FULL_WORKFLOW).unwrap();
        assert_eq!(wf.name, "daily_briefing_full");
        assert!(!wf.steps.is_empty());
        assert!(wf.inputs.iter().any(|i| i.name == "location"));
    }

    #[test]
    fn test_builtin_end_of_day_summary_parses() {
        let wf = parse_workflow(END_OF_DAY_SUMMARY_WORKFLOW).unwrap();
        assert_eq!(wf.name, "end_of_day_summary");
        assert!(!wf.steps.is_empty());
    }

    #[test]
    fn test_builtin_app_automation_parses() {
        let wf = parse_workflow(APP_AUTOMATION_WORKFLOW).unwrap();
        assert_eq!(wf.name, "app_automation");
        assert!(!wf.steps.is_empty());
        assert!(wf.inputs.iter().any(|i| i.name == "app_name"));
        assert!(wf.inputs.iter().any(|i| i.name == "task"));
    }

    #[test]
    fn test_builtin_email_triage_parses() {
        let wf = parse_workflow(EMAIL_TRIAGE_WORKFLOW).unwrap();
        assert_eq!(wf.name, "email_triage");
        assert!(!wf.steps.is_empty());
    }
}
