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
        assert_eq!(names.len(), 12);
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
}
