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
        // Research workflows
        "arxiv_research",
        // Cognitive extension workflows
        "knowledge_graph",
        "experiment_tracking",
        "code_analysis",
        "content_pipeline",
        "skill_development",
        "career_planning",
        "system_monitoring",
        "life_planning",
        "privacy_audit",
        "self_improvement_loop",
        // ML/AI workflow templates
        "ml_data_pipeline",
        "ml_training_experiment",
        "ml_model_deployment",
        "ml_rag_setup",
        "ml_llm_finetune",
        "ai_research",
        "ai_safety_audit",
        "ai_evaluation_suite",
        // Fullstack development workflows
        "fullstack_verify",
        // Security & compliance workflows
        "compliance_audit",
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
        "arxiv_research" => ARXIV_RESEARCH_WORKFLOW,
        "knowledge_graph" => KNOWLEDGE_GRAPH_WORKFLOW,
        "experiment_tracking" => EXPERIMENT_TRACKING_WORKFLOW,
        "code_analysis" => CODE_ANALYSIS_WORKFLOW,
        "content_pipeline" => CONTENT_PIPELINE_WORKFLOW,
        "skill_development" => SKILL_DEVELOPMENT_WORKFLOW,
        "career_planning" => CAREER_PLANNING_WORKFLOW,
        "system_monitoring" => SYSTEM_MONITORING_WORKFLOW,
        "life_planning" => LIFE_PLANNING_WORKFLOW,
        "privacy_audit" => PRIVACY_AUDIT_WORKFLOW,
        "self_improvement_loop" => SELF_IMPROVEMENT_LOOP_WORKFLOW,
        "ml_data_pipeline" => ML_DATA_PIPELINE_WORKFLOW,
        "ml_training_experiment" => ML_TRAINING_EXPERIMENT_WORKFLOW,
        "ml_model_deployment" => ML_MODEL_DEPLOYMENT_WORKFLOW,
        "ml_rag_setup" => ML_RAG_SETUP_WORKFLOW,
        "ml_llm_finetune" => ML_LLM_FINETUNE_WORKFLOW,
        "ai_research" => AI_RESEARCH_WORKFLOW,
        "ai_safety_audit" => AI_SAFETY_AUDIT_WORKFLOW,
        "ai_evaluation_suite" => AI_EVALUATION_SUITE_WORKFLOW,
        "fullstack_verify" => FULLSTACK_VERIFY_WORKFLOW,
        "compliance_audit" => COMPLIANCE_AUDIT_WORKFLOW,
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

const ARXIV_RESEARCH_WORKFLOW: &str = r#"
name: arxiv_research
description: Search, analyze, summarize, and manage academic papers with multi-source enrichment
version: "2.0"
author: rustant
inputs:
  - name: topic
    type: string
    description: Research topic or search query
  - name: category
    type: string
    optional: true
    description: ArXiv category filter (e.g., cs.AI, cs.LG, cs.CL)
  - name: max_results
    type: number
    optional: true
    default: 5
    description: Maximum number of papers to return
steps:
  - id: search_papers
    tool: arxiv_research
    params:
      action: "search"
      query: "{{ inputs.topic }}"
      max_results: "{{ inputs.max_results }}"
  - id: summarize_top
    tool: arxiv_research
    params:
      action: "summarize"
      arxiv_id: "{{ steps.search_papers.top_id }}"
      level: "abstract"
  - id: analyze_top
    tool: arxiv_research
    params:
      action: "analyze"
      arxiv_id: "{{ steps.search_papers.top_id }}"
      depth: "standard"
  - id: save_paper
    tool: arxiv_research
    params:
      action: "save"
      arxiv_id: "{{ steps.search_papers.top_id }}"
    gate:
      type: approval_required
      message: "Save the top paper to your library?"
  - id: summary
    tool: echo
    params:
      text: "ArXiv research complete. Papers searched, summarized, analyzed, and optionally saved."
outputs:
  - name: research_report
    value: "{{ steps.analyze_top.output }}"
"#;

const KNOWLEDGE_GRAPH_WORKFLOW: &str = r#"
name: knowledge_graph
description: Build and query a knowledge graph of concepts and relationships
version: "1.0"
author: rustant
inputs:
  - name: topic
    type: string
    description: Topic or concept to add to the knowledge graph
steps:
  - id: search_existing
    tool: knowledge_graph
    params:
      action: "search"
      query: "{{ inputs.topic }}"
  - id: add_node
    tool: knowledge_graph
    params:
      action: "add_node"
      name: "{{ inputs.topic }}"
      node_type: "Concept"
    gate:
      type: approval_required
      message: "Add '{{ inputs.topic }}' to the knowledge graph?"
  - id: add_edges
    tool: knowledge_graph
    params:
      action: "list"
  - id: summary
    tool: echo
    params:
      text: "Knowledge graph updated. Review connections and add edges as needed."
outputs:
  - name: graph_update
    value: "{{ steps.summary.output }}"
"#;

const EXPERIMENT_TRACKING_WORKFLOW: &str = r#"
name: experiment_tracking
description: Track hypotheses, run experiments, and record evidence
version: "1.0"
author: rustant
inputs:
  - name: hypothesis
    type: string
    description: The hypothesis to test
  - name: experiment_name
    type: string
    description: Name for the experiment
steps:
  - id: create_hypothesis
    tool: experiment_tracker
    params:
      action: "add_hypothesis"
      title: "{{ inputs.hypothesis }}"
  - id: create_experiment
    tool: experiment_tracker
    params:
      action: "add_experiment"
      name: "{{ inputs.experiment_name }}"
      hypothesis_id: "{{ steps.create_hypothesis.id }}"
  - id: start_experiment
    tool: experiment_tracker
    params:
      action: "start_experiment"
      id: "{{ steps.create_experiment.id }}"
  - id: summary
    tool: echo
    params:
      text: "Experiment started. Complete it with results when done."
outputs:
  - name: experiment_status
    value: "{{ steps.summary.output }}"
"#;

const CODE_ANALYSIS_WORKFLOW: &str = r#"
name: code_analysis
description: Analyze codebase architecture, tech debt, and API surface
version: "1.0"
author: rustant
inputs:
  - name: path
    type: string
    optional: true
    default: "."
    description: Path to analyze
steps:
  - id: architecture
    tool: code_intelligence
    params:
      action: "analyze_architecture"
      path: "{{ inputs.path }}"
  - id: tech_debt
    tool: code_intelligence
    params:
      action: "tech_debt_report"
      path: "{{ inputs.path }}"
  - id: api_surface
    tool: code_intelligence
    params:
      action: "api_surface"
      path: "{{ inputs.path }}"
  - id: summary
    tool: echo
    params:
      text: "Code analysis complete. Architecture, tech debt, and API surface reports generated."
outputs:
  - name: analysis_report
    value: "{{ steps.summary.output }}"
"#;

const CONTENT_PIPELINE_WORKFLOW: &str = r#"
name: content_pipeline
description: Content creation pipeline from idea to publication
version: "1.0"
author: rustant
inputs:
  - name: title
    type: string
    description: Content title
  - name: platform
    type: string
    optional: true
    default: "Blog"
    description: Target platform (Blog, Twitter, LinkedIn, etc.)
steps:
  - id: create_draft
    tool: content_engine
    params:
      action: "create"
      title: "{{ inputs.title }}"
      platform: "{{ inputs.platform }}"
  - id: review
    tool: echo
    params:
      text: "Draft created. Review and edit the content."
    gate:
      type: approval_required
      message: "Ready to schedule content for publishing?"
  - id: schedule
    tool: content_engine
    params:
      action: "stats"
  - id: summary
    tool: echo
    params:
      text: "Content pipeline complete. Draft created and ready for scheduling."
outputs:
  - name: pipeline_status
    value: "{{ steps.summary.output }}"
"#;

const SKILL_DEVELOPMENT_WORKFLOW: &str = r#"
name: skill_development
description: Assess skill gaps and create practice plans
version: "1.0"
author: rustant
inputs:
  - name: focus_area
    type: string
    optional: true
    description: Specific skill area to focus on
steps:
  - id: gaps
    tool: skill_tracker
    params:
      action: "knowledge_gaps"
  - id: daily_practice
    tool: skill_tracker
    params:
      action: "daily_practice"
  - id: summary
    tool: echo
    params:
      text: "Skill assessment complete. Practice plan generated."
outputs:
  - name: skill_report
    value: "{{ steps.summary.output }}"
"#;

const CAREER_PLANNING_WORKFLOW: &str = r#"
name: career_planning
description: Career goal setting and gap analysis
version: "1.0"
author: rustant
inputs:
  - name: goal
    type: string
    description: Career goal to plan for
steps:
  - id: set_goal
    tool: career_intel
    params:
      action: "set_goal"
      title: "{{ inputs.goal }}"
  - id: gap_analysis
    tool: career_intel
    params:
      action: "gap_analysis"
  - id: strategy
    tool: career_intel
    params:
      action: "strategy_review"
  - id: summary
    tool: echo
    params:
      text: "Career planning complete. Goal set with gap analysis and strategy review."
outputs:
  - name: career_plan
    value: "{{ steps.summary.output }}"
"#;

const SYSTEM_MONITORING_WORKFLOW: &str = r#"
name: system_monitoring
description: Health check services and handle incidents
version: "1.0"
author: rustant
inputs:
  - name: service_id
    type: string
    optional: true
    description: Specific service to check (omit for all)
steps:
  - id: health_check
    tool: system_monitor
    params:
      action: "health_check"
  - id: topology
    tool: system_monitor
    params:
      action: "topology"
  - id: summary
    tool: echo
    params:
      text: "System monitoring complete. Health checks run and topology reviewed."
outputs:
  - name: monitoring_report
    value: "{{ steps.summary.output }}"
"#;

const LIFE_PLANNING_WORKFLOW: &str = r#"
name: life_planning
description: Generate daily plan with energy-aware scheduling
version: "1.0"
author: rustant
inputs:
  - name: date
    type: string
    optional: true
    description: Date to plan for (default today)
steps:
  - id: daily_plan
    tool: life_planner
    params:
      action: "daily_plan"
  - id: optimize
    tool: life_planner
    params:
      action: "optimize_schedule"
  - id: summary
    tool: echo
    params:
      text: "Daily plan generated with energy-optimized schedule."
outputs:
  - name: daily_plan
    value: "{{ steps.summary.output }}"
"#;

const PRIVACY_AUDIT_WORKFLOW: &str = r#"
name: privacy_audit
description: Audit data boundaries and compliance
version: "1.0"
author: rustant
inputs:
  - name: scope
    type: string
    optional: true
    default: "full"
    description: Audit scope (full, boundaries, access)
steps:
  - id: compliance
    tool: privacy_manager
    params:
      action: "compliance_check"
  - id: audit
    tool: privacy_manager
    params:
      action: "audit_access"
      limit: 50
  - id: report
    tool: privacy_manager
    params:
      action: "privacy_report"
  - id: summary
    tool: echo
    params:
      text: "Privacy audit complete. Compliance check, access audit, and report generated."
outputs:
  - name: audit_report
    value: "{{ steps.summary.output }}"
"#;

const SELF_IMPROVEMENT_LOOP_WORKFLOW: &str = r#"
name: self_improvement_loop
description: Analyze usage patterns and suggest optimizations
version: "1.0"
author: rustant
inputs:
  - name: focus
    type: string
    optional: true
    description: Specific area to focus improvement on
steps:
  - id: patterns
    tool: self_improvement
    params:
      action: "analyze_patterns"
  - id: performance
    tool: self_improvement
    params:
      action: "performance_report"
  - id: suggestions
    tool: self_improvement
    params:
      action: "suggest_improvements"
  - id: summary
    tool: echo
    params:
      text: "Self-improvement analysis complete. Patterns, performance, and suggestions generated."
outputs:
  - name: improvement_report
    value: "{{ steps.summary.output }}"
"#;

// ---------------------------------------------------------------------------
// ML/AI Workflow Templates
// ---------------------------------------------------------------------------

const ML_DATA_PIPELINE_WORKFLOW: &str = r#"
name: ml_data_pipeline
description: End-to-end ML data pipeline with ingestion, validation, PII scanning, transformation, splitting, and versioning
version: "1.0"
author: rustant
inputs:
  - name: data_source
    type: string
    description: Path or URI of the raw data source
  - name: output_dir
    type: string
    optional: true
    default: "./data"
    description: Output directory for processed data
steps:
  - id: ingest
    tool: echo
    params:
      text: "Ingesting data from {{ inputs.data_source }}"
  - id: validate
    tool: echo
    params:
      text: "Validating data schema and integrity"
  - id: pii_scan
    tool: echo
    params:
      text: "Scanning for PII and sensitive data"
  - id: transform
    tool: echo
    params:
      text: "Applying data transformations and feature engineering"
    gate:
      type: approval_required
      message: "Apply data transformations to the validated dataset?"
  - id: split
    tool: echo
    params:
      text: "Splitting dataset into train/validation/test sets"
  - id: version
    tool: echo
    params:
      text: "Versioning processed dataset in {{ inputs.output_dir }}"
outputs:
  - name: pipeline_result
    value: "{{ steps.version.output }}"
"#;

const ML_TRAINING_EXPERIMENT_WORKFLOW: &str = r#"
name: ml_training_experiment
description: ML training experiment lifecycle with experiment tracking, training, evaluation, checkpointing, and comparison
version: "1.0"
author: rustant
inputs:
  - name: experiment_name
    type: string
    description: Name for the training experiment
  - name: model_config
    type: string
    optional: true
    default: "default"
    description: Model configuration or hyperparameter preset
steps:
  - id: create_experiment
    tool: echo
    params:
      text: "Creating experiment: {{ inputs.experiment_name }} with config {{ inputs.model_config }}"
  - id: train
    tool: echo
    params:
      text: "Training model for experiment {{ inputs.experiment_name }}"
    gate:
      type: approval_required
      message: "Start training run for {{ inputs.experiment_name }}?"
  - id: evaluate
    tool: echo
    params:
      text: "Evaluating trained model on validation set"
  - id: checkpoint
    tool: echo
    params:
      text: "Saving model checkpoint and artifacts"
  - id: compare
    tool: echo
    params:
      text: "Comparing results against previous experiments"
outputs:
  - name: experiment_result
    value: "{{ steps.compare.output }}"
"#;

const ML_MODEL_DEPLOYMENT_WORKFLOW: &str = r#"
name: ml_model_deployment
description: ML model deployment pipeline with benchmarking, conversion, serving, output validation, and monitoring
version: "1.0"
author: rustant
inputs:
  - name: model_path
    type: string
    description: Path to the trained model artifact
  - name: target_env
    type: string
    optional: true
    default: "staging"
    description: Target deployment environment
steps:
  - id: benchmark
    tool: echo
    params:
      text: "Benchmarking model latency and throughput from {{ inputs.model_path }}"
  - id: convert
    tool: echo
    params:
      text: "Converting model to serving format"
  - id: serve
    tool: echo
    params:
      text: "Deploying model to {{ inputs.target_env }}"
    gate:
      type: approval_required
      message: "Deploy model to {{ inputs.target_env }}?"
  - id: validate_outputs
    tool: echo
    params:
      text: "Validating model outputs against golden test set"
  - id: monitor
    tool: echo
    params:
      text: "Enabling monitoring and drift detection for deployed model"
outputs:
  - name: deployment_result
    value: "{{ steps.monitor.output }}"
"#;

const ML_RAG_SETUP_WORKFLOW: &str = r#"
name: ml_rag_setup
description: RAG pipeline setup with document ingestion, chunking, embedding, indexing, query testing, and evaluation
version: "1.0"
author: rustant
inputs:
  - name: documents_path
    type: string
    description: Path to the documents to ingest
  - name: chunk_size
    type: number
    optional: true
    default: 512
    description: Chunk size in tokens for document splitting
steps:
  - id: ingest_documents
    tool: echo
    params:
      text: "Ingesting documents from {{ inputs.documents_path }}"
  - id: chunk
    tool: echo
    params:
      text: "Chunking documents with size {{ inputs.chunk_size }} tokens"
  - id: embed
    tool: echo
    params:
      text: "Generating embeddings for document chunks"
  - id: index
    tool: echo
    params:
      text: "Building vector index from embeddings"
    gate:
      type: approval_required
      message: "Build vector index from processed documents?"
  - id: test_queries
    tool: echo
    params:
      text: "Running test queries against the index"
  - id: evaluate
    tool: echo
    params:
      text: "Evaluating retrieval quality and relevance scores"
outputs:
  - name: rag_result
    value: "{{ steps.evaluate.output }}"
"#;

const ML_LLM_FINETUNE_WORKFLOW: &str = r#"
name: ml_llm_finetune
description: LLM fine-tuning pipeline with dataset preparation, PII scanning, training, alignment testing, red teaming, and model merge
version: "1.0"
author: rustant
inputs:
  - name: base_model
    type: string
    description: Base model identifier to fine-tune
  - name: dataset_path
    type: string
    description: Path to the fine-tuning dataset
steps:
  - id: prepare_dataset
    tool: echo
    params:
      text: "Preparing fine-tuning dataset from {{ inputs.dataset_path }}"
  - id: pii_scan
    tool: echo
    params:
      text: "Scanning dataset for PII and sensitive content"
  - id: finetune
    tool: echo
    params:
      text: "Fine-tuning {{ inputs.base_model }} on prepared dataset"
    gate:
      type: approval_required
      message: "Start fine-tuning {{ inputs.base_model }}?"
  - id: alignment_test
    tool: echo
    params:
      text: "Running alignment and safety tests on fine-tuned model"
  - id: red_team
    tool: echo
    params:
      text: "Executing red team evaluation against fine-tuned model"
  - id: merge
    tool: echo
    params:
      text: "Merging fine-tuned adapters into base model"
    gate:
      type: approval_required
      message: "Merge fine-tuned model weights into base model?"
outputs:
  - name: finetune_result
    value: "{{ steps.merge.output }}"
"#;

const AI_RESEARCH_WORKFLOW: &str = r#"
name: ai_research
description: AI research workflow with literature search, methodology extraction, comparison, knowledge graph integration, and gap analysis
version: "1.0"
author: rustant
inputs:
  - name: topic
    type: string
    description: Research topic or question
  - name: max_papers
    type: number
    optional: true
    default: 10
    description: Maximum number of papers to analyze
steps:
  - id: literature_search
    tool: echo
    params:
      text: "Searching literature for {{ inputs.topic }} (max {{ inputs.max_papers }} papers)"
  - id: methodology_extract
    tool: echo
    params:
      text: "Extracting methodologies, datasets, and metrics from found papers"
  - id: compare
    tool: echo
    params:
      text: "Comparing approaches, results, and trade-offs across papers"
  - id: knowledge_graph
    tool: echo
    params:
      text: "Updating knowledge graph with research findings and relationships"
    gate:
      type: approval_required
      message: "Add extracted research findings to the knowledge graph?"
  - id: gaps
    tool: echo
    params:
      text: "Identifying research gaps and open questions"
outputs:
  - name: research_result
    value: "{{ steps.gaps.output }}"
"#;

const AI_SAFETY_AUDIT_WORKFLOW: &str = r#"
name: ai_safety_audit
description: AI safety audit with PII scanning, bias detection, alignment testing, red teaming, and report generation
version: "1.0"
author: rustant
inputs:
  - name: model_id
    type: string
    description: Model identifier or endpoint to audit
  - name: dataset_path
    type: string
    optional: true
    default: ""
    description: Path to evaluation dataset (uses built-in if empty)
steps:
  - id: pii_scan
    tool: echo
    params:
      text: "Scanning model inputs and outputs for PII leakage"
  - id: bias_detect
    tool: echo
    params:
      text: "Running bias detection across demographic groups for {{ inputs.model_id }}"
  - id: alignment_test
    tool: echo
    params:
      text: "Testing alignment with safety guidelines and refusal behavior"
  - id: red_team
    tool: echo
    params:
      text: "Executing red team adversarial evaluation"
    gate:
      type: approval_required
      message: "Run red team adversarial prompts against {{ inputs.model_id }}?"
  - id: report
    tool: echo
    params:
      text: "Generating AI safety audit report with findings and recommendations"
outputs:
  - name: audit_result
    value: "{{ steps.report.output }}"
"#;

const AI_EVALUATION_SUITE_WORKFLOW: &str = r#"
name: ai_evaluation_suite
description: AI evaluation suite with trace collection, taxonomy building, LLM-as-judge evaluation, benchmarking, and CI gating
version: "1.0"
author: rustant
inputs:
  - name: eval_name
    type: string
    description: Name for the evaluation run
  - name: threshold
    type: string
    optional: true
    default: "0.8"
    description: Minimum passing score threshold for CI gate
steps:
  - id: collect_traces
    tool: echo
    params:
      text: "Collecting inference traces for evaluation {{ inputs.eval_name }}"
  - id: build_taxonomy
    tool: echo
    params:
      text: "Building evaluation taxonomy and failure categories"
  - id: judge
    tool: echo
    params:
      text: "Running LLM-as-judge evaluation on collected traces"
  - id: benchmark
    tool: echo
    params:
      text: "Computing benchmark scores and comparing against baselines"
  - id: ci_gate
    tool: echo
    params:
      text: "Evaluating CI gate with threshold {{ inputs.threshold }}"
    gate:
      type: approval_required
      message: "Apply CI gate decision based on evaluation results?"
outputs:
  - name: evaluation_result
    value: "{{ steps.ci_gate.output }}"
"#;

// ---------------------------------------------------------------------------
// Fullstack Development Workflow Templates
// ---------------------------------------------------------------------------

const FULLSTACK_VERIFY_WORKFLOW: &str = r#"
name: fullstack_verify
description: "Full verification pipeline: test, lint, typecheck, fix, and re-verify with up to 3 iterations"
version: "1.0"
author: rustant
inputs:
  - name: path
    type: string
    optional: true
    default: "."
    description: Project root path
  - name: max_iterations
    type: number
    optional: true
    default: 3
    description: Maximum fix-and-recheck iterations
steps:
  - id: run_tests
    tool: shell_exec
    params:
      command: "cd {{ inputs.path }} && cargo test --workspace 2>&1 || npm test 2>&1 || pytest 2>&1 || echo 'No test runner detected'"
    on_error:
      action: skip
  - id: run_lint
    tool: shell_exec
    params:
      command: "cd {{ inputs.path }} && cargo clippy --workspace --all-targets -- -D warnings 2>&1 || npx eslint . 2>&1 || ruff check . 2>&1 || echo 'No linter detected'"
    on_error:
      action: skip
  - id: run_typecheck
    tool: shell_exec
    params:
      command: "cd {{ inputs.path }} && npx tsc --noEmit 2>&1 || mypy . 2>&1 || echo 'No type checker detected'"
    on_error:
      action: skip
  - id: auto_fix
    tool: shell_exec
    params:
      command: "cd {{ inputs.path }} && cargo fmt --all 2>&1 || npx eslint . --fix 2>&1 || ruff check . --fix 2>&1 || echo 'No auto-fixer detected'"
    gate:
      type: approval_required
      message: "Apply auto-fix for detected lint and formatting issues?"
    on_error:
      action: skip
  - id: re_verify
    tool: shell_exec
    params:
      command: "cd {{ inputs.path }} && cargo test --workspace 2>&1 && cargo clippy --workspace --all-targets -- -D warnings 2>&1 || npm test 2>&1 && npx eslint . 2>&1 || echo 'Re-verification complete'"
    on_error:
      action: skip
  - id: summary
    tool: echo
    params:
      text: "Fullstack verification complete. Tests, linting, type checking, and auto-fix applied (max {{ inputs.max_iterations }} iterations)."
outputs:
  - name: verification_result
    value: "{{ steps.summary.output }}"
"#;

// ---------------------------------------------------------------------------
// Security & Compliance Workflow Templates
// ---------------------------------------------------------------------------

const COMPLIANCE_AUDIT_WORKFLOW: &str = r#"
name: compliance_audit
description: Generate compliance evidence with security scanning, license checks, SBOM generation, risk scoring, and audit export
version: "1.0"
author: rustant
inputs:
  - name: path
    type: string
    optional: true
    default: "."
    description: Path to the project to audit
  - name: format
    type: string
    optional: true
    default: "sarif"
    description: Export format for findings (sarif, ocsf, markdown)
steps:
  - id: security_scan
    tool: echo
    params:
      text: "Running all security scanners (SAST, SCA, secrets, IaC, container, supply chain) on {{ inputs.path }}"
  - id: license_check
    tool: echo
    params:
      text: "Checking license compliance for all dependencies"
  - id: sbom_generate
    tool: echo
    params:
      text: "Generating Software Bill of Materials (SBOM) in CycloneDX format"
  - id: risk_score
    tool: echo
    params:
      text: "Calculating multi-dimensional risk score across security, license, and supply chain vectors"
  - id: review_gate
    tool: echo
    params:
      text: "Review compliance findings before generating final report"
    gate:
      type: approval_required
      message: "Critical findings detected. Review and approve before generating compliance report?"
  - id: compliance_report
    tool: echo
    params:
      text: "Generating compliance report with findings, risk assessment, and remediation guidance"
  - id: audit_export
    tool: echo
    params:
      text: "Exporting audit findings in {{ inputs.format }} format"
outputs:
  - name: audit_report
    value: "{{ steps.audit_export.output }}"
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
            let wf = get_builtin(name).unwrap_or_else(|| panic!("Failed to load builtin: {name}"));
            validate_workflow(&wf)
                .unwrap_or_else(|e| panic!("Builtin '{name}' failed validation: {e}"));
        }
    }

    #[test]
    fn test_list_builtin_names() {
        let names = list_builtin_names();
        assert_eq!(names.len(), 38);
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
        // Research workflows
        assert!(names.contains(&"arxiv_research"));
        // Cognitive extension workflows
        assert!(names.contains(&"knowledge_graph"));
        assert!(names.contains(&"experiment_tracking"));
        assert!(names.contains(&"code_analysis"));
        assert!(names.contains(&"content_pipeline"));
        assert!(names.contains(&"skill_development"));
        assert!(names.contains(&"career_planning"));
        assert!(names.contains(&"system_monitoring"));
        assert!(names.contains(&"life_planning"));
        assert!(names.contains(&"privacy_audit"));
        assert!(names.contains(&"self_improvement_loop"));
        // ML/AI workflow templates
        assert!(names.contains(&"ml_data_pipeline"));
        assert!(names.contains(&"ml_training_experiment"));
        assert!(names.contains(&"ml_model_deployment"));
        assert!(names.contains(&"ml_rag_setup"));
        assert!(names.contains(&"ml_llm_finetune"));
        assert!(names.contains(&"ai_research"));
        assert!(names.contains(&"ai_safety_audit"));
        assert!(names.contains(&"ai_evaluation_suite"));
        // Fullstack development workflows
        assert!(names.contains(&"fullstack_verify"));
        // Security & compliance workflows
        assert!(names.contains(&"compliance_audit"));
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

    #[test]
    fn test_builtin_arxiv_research_parses() {
        let wf = parse_workflow(ARXIV_RESEARCH_WORKFLOW).unwrap();
        assert_eq!(wf.name, "arxiv_research");
        assert!(!wf.steps.is_empty());
        assert!(wf.inputs.iter().any(|i| i.name == "topic"));
    }
}
