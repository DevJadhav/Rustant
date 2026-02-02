# Skills

Skills are declarative tool definitions written in SKILL.md files. They allow extending Rustant's capabilities without writing Rust code.

## SKILL.md Format

A skill file is a Markdown document with YAML frontmatter:

```text
---
name: my-skill
version: "1.0.0"
description: A brief description of what this skill does
author: Your Name
requires:
  - type: tool
    name: shell_exec
  - type: secret
    name: MY_API_KEY
config:
  setting_name: default_value
---

### tool_name

Description of what this tool does.

Parameters:
- `param1` (string): Description of param1
- `param2` (number): Description of param2

Body:
shell_exec with arguments based on {{param1}}
```

## Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique skill identifier |
| `version` | No | Semver version (default: "0.1.0") |
| `description` | No | Human-readable description |
| `author` | No | Skill author |
| `requires` | No | Dependencies (tools and secrets) |
| `config` | No | Key-value configuration defaults |

## Tool Definitions

Each `### heading` in the markdown body defines a tool. The section contains:

- A description paragraph
- Optional `Parameters:` list
- Optional `Body:` section with implementation details

## Security Validation

Skills are automatically scanned for security risks:

```bash
rustant skill validate path/to/SKILL.md
```

The validator checks for:

- **Dangerous patterns** — `shell_exec`, `sudo`, `rm -rf`, `chmod 777`, `eval`, `curl | bash`, etc.
- **Missing dependencies** — Required tools or secrets not available
- **Risk levels** — Low, Medium, High, Critical

| Risk Level | Criteria |
|------------|----------|
| Low | Read-only tools, no secrets |
| Medium | Write tools or secrets required |
| High | Shell execution or dangerous patterns |
| Critical | Multiple dangerous patterns |

## CLI

```bash
rustant skill list                      # List skills in default directory
rustant skill list --dir ./my-skills    # List skills in a specific directory
rustant skill info path/to/SKILL.md     # Show skill details
rustant skill validate path/to/SKILL.md # Security validation
rustant skill load path/to/SKILL.md     # Parse and display as JSON
```

## Configuration

```toml
[skills]
enabled = true
skills_dir = "~/.config/rustant/skills"
auto_load = true
```

## Example Skill

```text
---
name: github-pr-review
version: "1.0.0"
description: Review GitHub pull requests
author: Dev
requires:
  - type: tool
    name: shell_exec
  - type: secret
    name: GITHUB_TOKEN
---

### review_pr

Reviews a GitHub pull request and provides feedback.

Parameters:
- `repo` (string): Repository in owner/repo format
- `pr_number` (number): Pull request number

Body:
Fetch PR diff via GitHub API and analyze code changes.
```
