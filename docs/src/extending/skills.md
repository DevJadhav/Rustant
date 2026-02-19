# Skills

Skills are declarative tool definitions written in SKILL.md files. They allow extending Rustant's capabilities without writing Rust code.

## SKILL.md Format

A skill file is a Markdown document with YAML frontmatter:

```text
---
name: my-skill
version: "1.0.0"
description: A brief description
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
- `param1` (string): Description
- `param2` (number): Description

Body:
shell_exec with arguments based on {{param1}}
```

## Security Validation

```bash
rustant skill validate path/to/SKILL.md
```

Checks for dangerous patterns (`sudo`, `rm -rf`, `eval`, `curl | bash`), missing dependencies, and assigns risk levels (Low/Medium/High/Critical).

## CLI

```bash
rustant skill list                      # List skills
rustant skill list --dir ./my-skills    # List from directory
rustant skill info path/to/SKILL.md     # Show details
rustant skill validate path/to/SKILL.md # Security check
rustant skill load path/to/SKILL.md     # Parse as JSON
```

## Configuration

```toml
[skills]
enabled = true
skills_dir = "~/.config/rustant/skills"
auto_load = true
```
