//! Skill parser — reads SKILL.md files with YAML frontmatter.
//!
//! A SKILL.md file has the format:
//! ```text
//! ---
//! name: my-skill
//! version: 1.0.0
//! description: A skill that does X
//! requires:
//!   - type: tool
//!     name: shell_exec
//!   - type: secret
//!     name: API_KEY
//! ---
//!
//! ## Tools
//!
//! ### tool_name
//!
//! Description of the tool.
//!
//! **Parameters:**
//! ```json
//! {"type": "object", "properties": {"input": {"type": "string"}}}
//! ```
//!
//! **Body:**
//! ```text
//! Execute: shell_exec with input
//! ```

use super::types::{SkillDefinition, SkillRequirement, SkillToolDef};

/// Error when parsing a SKILL.md file.
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("No YAML frontmatter found (expected --- delimiters)")]
    NoFrontmatter,
    #[error("Invalid YAML frontmatter: {0}")]
    InvalidYaml(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
}

/// Parsed YAML frontmatter from a SKILL.md file.
#[derive(Debug, serde::Deserialize)]
struct SkillFrontmatter {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    author: Option<String>,
    #[serde(default)]
    requires: Vec<RequirementYaml>,
}

#[derive(Debug, serde::Deserialize)]
struct RequirementYaml {
    #[serde(rename = "type")]
    req_type: String,
    name: String,
}

/// Parse a SKILL.md file content into a SkillDefinition.
pub fn parse_skill_md(content: &str) -> Result<SkillDefinition, ParseError> {
    let (frontmatter_str, body) = extract_frontmatter(content)?;

    let fm: SkillFrontmatter = serde_yaml::from_str(&frontmatter_str)
        .map_err(|e| ParseError::InvalidYaml(e.to_string()))?;

    let name = fm.name.ok_or(ParseError::MissingField("name".into()))?;
    let version = fm.version.unwrap_or_else(|| "0.1.0".into());
    let description = fm.description.unwrap_or_else(|| "No description".into());

    let requires: Vec<SkillRequirement> = fm
        .requires
        .into_iter()
        .map(|r| SkillRequirement {
            req_type: r.req_type,
            name: r.name,
        })
        .collect();

    let tools = parse_tools_section(&body);

    Ok(SkillDefinition {
        name,
        version,
        description,
        author: fm.author,
        requires,
        tools,
        config: Default::default(),
        risk_level: super::types::SkillRiskLevel::Low,
        source_path: None,
    })
}

/// Extract YAML frontmatter from content between --- delimiters.
fn extract_frontmatter(content: &str) -> Result<(String, String), ParseError> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(ParseError::NoFrontmatter);
    }

    let after_first = &trimmed[3..];
    let end_pos = after_first.find("\n---").ok_or(ParseError::NoFrontmatter)?;

    let frontmatter = after_first[..end_pos].trim().to_string();
    let body = after_first[end_pos + 4..].to_string();

    Ok((frontmatter, body))
}

/// Parse the ## Tools section from the markdown body.
fn parse_tools_section(body: &str) -> Vec<SkillToolDef> {
    let mut tools = Vec::new();
    let mut current_tool: Option<(String, String)> = None;
    let mut in_params_block = false;
    let mut in_body_block = false;
    let mut params_json = String::new();
    let mut body_text = String::new();

    for line in body.lines() {
        // Detect ### tool_name headers
        if let Some(stripped) = line.strip_prefix("### ") {
            // Save previous tool if any
            if let Some((name, description)) = current_tool.take() {
                let params = if params_json.is_empty() {
                    serde_json::json!({"type": "object"})
                } else {
                    serde_json::from_str(&params_json)
                        .unwrap_or(serde_json::json!({"type": "object"}))
                };
                tools.push(SkillToolDef {
                    name,
                    description,
                    parameters: params,
                    body: body_text.trim().to_string(),
                });
                params_json.clear();
                body_text.clear();
            }
            let tool_name = stripped.trim().to_string();
            current_tool = Some((tool_name, String::new()));
            continue;
        }

        // If we're inside a tool definition, collect description, params, body
        if let Some((_, ref mut description)) = current_tool {
            if line.starts_with("**Parameters:**") {
                continue;
            }
            if line.starts_with("**Body:**") {
                continue;
            }
            if line.starts_with("```json") {
                in_params_block = true;
                continue;
            }
            if line.starts_with("```") && !line.starts_with("```json") {
                if in_params_block {
                    in_params_block = false;
                    continue;
                }
                if in_body_block {
                    in_body_block = false;
                    continue;
                }
                // Start body block
                in_body_block = true;
                continue;
            }
            if in_params_block {
                params_json.push_str(line);
                params_json.push('\n');
                continue;
            }
            if in_body_block {
                body_text.push_str(line);
                body_text.push('\n');
                continue;
            }
            // Regular line — add to description if description is empty
            let trimmed = line.trim();
            if !trimmed.is_empty() && description.is_empty() {
                *description = trimmed.to_string();
            }
        }
    }

    // Save last tool
    if let Some((name, description)) = current_tool.take() {
        let params = if params_json.is_empty() {
            serde_json::json!({"type": "object"})
        } else {
            serde_json::from_str(&params_json).unwrap_or(serde_json::json!({"type": "object"}))
        };
        tools.push(SkillToolDef {
            name,
            description,
            parameters: params,
            body: body_text.trim().to_string(),
        });
    }

    tools
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SKILL: &str = r#"---
name: github
version: 1.0.0
description: GitHub integration skill
author: Test Author
requires:
  - type: tool
    name: shell_exec
  - type: secret
    name: GITHUB_TOKEN
---

## Tools

### github_pr_list

List open pull requests for a repository.

**Parameters:**
```json
{"type": "object", "properties": {"repo": {"type": "string"}}, "required": ["repo"]}
```

**Body:**
```
shell_exec: gh pr list --repo {{repo}}
```

### github_issue_create

Create a new GitHub issue.

**Parameters:**
```json
{"type": "object", "properties": {"repo": {"type": "string"}, "title": {"type": "string"}}}
```

**Body:**
```
shell_exec: gh issue create --repo {{repo}} --title "{{title}}"
```
"#;

    #[test]
    fn test_parse_valid_skill() {
        let skill = parse_skill_md(VALID_SKILL).unwrap();
        assert_eq!(skill.name, "github");
        assert_eq!(skill.version, "1.0.0");
        assert_eq!(skill.description, "GitHub integration skill");
        assert_eq!(skill.author, Some("Test Author".into()));
        assert_eq!(skill.requires.len(), 2);
        assert_eq!(skill.requires[0].req_type, "tool");
        assert_eq!(skill.requires[0].name, "shell_exec");
        assert_eq!(skill.requires[1].req_type, "secret");
        assert_eq!(skill.requires[1].name, "GITHUB_TOKEN");
        assert_eq!(skill.tools.len(), 2);
        assert_eq!(skill.tools[0].name, "github_pr_list");
        assert_eq!(skill.tools[1].name, "github_issue_create");
        assert!(skill.tools[0].body.contains("gh pr list"));
    }

    #[test]
    fn test_parse_missing_name() {
        let content = "---\nversion: 1.0.0\n---\n";
        let result = parse_skill_md(content);
        assert!(matches!(result, Err(ParseError::MissingField(_))));
    }

    #[test]
    fn test_parse_no_frontmatter() {
        let content = "# Just markdown\nNo frontmatter here.";
        let result = parse_skill_md(content);
        assert!(matches!(result, Err(ParseError::NoFrontmatter)));
    }

    #[test]
    fn test_parse_no_tools_section() {
        let content = "---\nname: empty-skill\n---\n\nJust some text.";
        let skill = parse_skill_md(content).unwrap();
        assert_eq!(skill.name, "empty-skill");
        assert!(skill.tools.is_empty());
    }

    #[test]
    fn test_parse_minimal_skill() {
        let content = "---\nname: minimal\n---\n";
        let skill = parse_skill_md(content).unwrap();
        assert_eq!(skill.name, "minimal");
        assert_eq!(skill.version, "0.1.0");
        assert_eq!(skill.description, "No description");
    }

    #[test]
    fn test_extract_frontmatter() {
        let (fm, body) = extract_frontmatter("---\nname: test\n---\nbody here").unwrap();
        assert_eq!(fm, "name: test");
        assert!(body.contains("body here"));
    }

    #[test]
    fn test_tool_parameters_parsed() {
        let skill = parse_skill_md(VALID_SKILL).unwrap();
        let tool = &skill.tools[0];
        assert!(tool.parameters.is_object());
        assert!(tool.parameters["properties"]["repo"].is_object());
    }
}
