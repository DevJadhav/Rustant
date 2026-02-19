# CLI Reference

Rustant provides a comprehensive command-line interface via the `rustant` binary. When invoked without arguments or a subcommand, it starts the interactive REPL. When given a positional argument, it executes that as a single task.

## Global Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--model <MODEL>` | `-m` | LLM model to use (overrides config) |
| `--workspace <DIR>` | `-w` | Workspace directory (default: `.`) |
| `--config <PATH>` | `-c` | Configuration file path |
| `--approval <MODE>` | `-a` | Approval mode: `safe`, `cautious`, `paranoid`, `yolo` |
| `--verbose` | `-v` | Increase verbosity (`-v` info, `-vv` debug, `-vvv` trace) |
| `--quiet` | `-q` | Suppress non-essential output (errors only) |
| `--voice` | | Enable voice input mode (requires microphone access) |

## Modes

```bash
rustant                    # Interactive REPL
rustant "describe this project"  # Single task execution
rustant --voice            # Voice command mode
```

## Subcommands

### Core

| Command | Description |
|---------|-------------|
| `rustant setup` | Interactive provider setup wizard |
| `rustant init` | Smart project initialization (detects type, generates config) |
| `rustant resume [session]` | Resume a previous session (most recent if omitted) |
| `rustant sessions [--limit N]` | List saved sessions (default: 10) |

### Configuration

| Command | Description |
|---------|-------------|
| `rustant config init` | Create default configuration file |
| `rustant config show` | Show current configuration |

### Channels

| Command | Description |
|---------|-------------|
| `rustant channel list` | List all configured channels and status |
| `rustant channel setup [name]` | Interactive channel setup wizard |
| `rustant channel test <name>` | Test a channel connection |

#### Slack Operations

| Command | Description |
|---------|-------------|
| `rustant channel slack send <channel> <message>` | Send message to a Slack channel |
| `rustant channel slack history <channel> [-n LIMIT]` | Read recent messages (default: 10) |
| `rustant channel slack channels` | List all workspace channels |
| `rustant channel slack users` | List all workspace users |
| `rustant channel slack info <channel>` | Get channel info by ID |
| `rustant channel slack dm <user> <message>` | Send a direct message |
| `rustant channel slack thread <channel> <ts> <message>` | Reply in a thread |
| `rustant channel slack react <channel> <ts> <emoji>` | Add emoji reaction |
| `rustant channel slack files [channel]` | List shared files |
| `rustant channel slack team` | Show workspace/team info |
| `rustant channel slack groups` | List user groups |
| `rustant channel slack join <channel>` | Join a channel |

### Authentication

| Command | Description |
|---------|-------------|
| `rustant auth status` | Show auth status for all providers |
| `rustant auth login <provider> [--redirect-uri URL]` | OAuth login flow |
| `rustant auth logout <provider>` | Remove stored OAuth tokens |
| `rustant auth refresh <provider>` | Manually refresh OAuth token |

Supported providers: `openai`, `gemini`, `slack`, `discord`, `teams`, `whatsapp`.

### Workflows

| Command | Description |
|---------|-------------|
| `rustant workflow list` | List available workflow definitions |
| `rustant workflow show <name>` | Show workflow details |
| `rustant workflow run <name> [-i key=val]` | Run a workflow with optional inputs |
| `rustant workflow runs` | List active workflow runs |
| `rustant workflow status <run_id>` | Show run status |
| `rustant workflow resume <run_id>` | Resume a paused run |
| `rustant workflow cancel <run_id>` | Cancel a running workflow |

### Cron / Scheduler

| Command | Description |
|---------|-------------|
| `rustant cron list` | List all scheduled cron jobs |
| `rustant cron add <name> <schedule> <task>` | Add a new cron job |
| `rustant cron run <name>` | Manually trigger a cron job |
| `rustant cron enable <name>` | Enable a disabled job |
| `rustant cron disable <name>` | Disable a job |
| `rustant cron remove <name>` | Remove a cron job |
| `rustant cron jobs` | List background jobs |
| `rustant cron cancel-job <job_id>` | Cancel a background job |

Cron state is persisted to `.rustant/cron/state.json`.

### Voice

| Command | Description |
|---------|-------------|
| `rustant voice speak <text> [--voice NAME]` | Synthesize text to speech |
| `rustant voice roundtrip <text>` | TTS then STT roundtrip |

Voices: `alloy` (default), `echo`, `fable`, `onyx`, `nova`, `shimmer`. Requires `OPENAI_API_KEY`.

### Browser

| Command | Description |
|---------|-------------|
| `rustant browser test [url]` | Navigate to URL (default: example.com) |
| `rustant browser connect [--port PORT]` | Connect to existing Chrome (default: 9222) |
| `rustant browser launch [--port PORT] [--headless]` | Launch new Chrome instance |
| `rustant browser status` | Show connection status and open tabs |

### Canvas

| Command | Description |
|---------|-------------|
| `rustant canvas push <type> <content>` | Push content to canvas |
| `rustant canvas clear` | Clear the canvas |
| `rustant canvas snapshot` | Get canvas state snapshot |

Content types: `html`, `markdown`, `code`, `chart`, `table`, `form`, `image`, `diagram`.

### Skills

| Command | Description |
|---------|-------------|
| `rustant skill list [--dir PATH]` | List loaded skills |
| `rustant skill info <path>` | Show SKILL.md details |
| `rustant skill validate <path>` | Validate a skill file for security issues |
| `rustant skill load <path>` | Load and show parsed definition |

### Plugins

| Command | Description |
|---------|-------------|
| `rustant plugin list [--dir PATH]` | List loaded plugins |
| `rustant plugin info <name>` | Show plugin info |

### System

| Command | Description |
|---------|-------------|
| `rustant update check` | Check for available updates |
| `rustant update install` | Download and install latest version |
| `rustant ui [--port PORT]` | Launch the Tauri dashboard UI (default port: 18790) |

### Security

#### Scanning

| Command | Description |
|---------|-------------|
| `rustant scan all [--path PATH] [--format FMT]` | Run all scanners (formats: sarif, markdown, json) |
| `rustant scan sast [--path PATH] [--languages LANGS]` | Static application security testing |
| `rustant scan sca [--path PATH]` | Software composition analysis |
| `rustant scan secrets [--path PATH] [--history]` | Scan for hardcoded secrets |
| `rustant scan iac [--path PATH]` | Scan infrastructure-as-code files |
| `rustant scan container <target>` | Scan container images or Dockerfiles |
| `rustant scan supply-chain [--path PATH]` | Check supply chain security |

#### Code Review

| Command | Description |
|---------|-------------|
| `rustant review diff [base]` | Review code changes (default base: HEAD~1) |
| `rustant review path <path>` | Review a specific file or directory |
| `rustant review fix [--auto]` | Generate fix suggestions (optionally auto-apply) |

#### Quality

| Command | Description |
|---------|-------------|
| `rustant quality score [path]` | Calculate code quality score (A-F) |
| `rustant quality complexity [path]` | Analyze cyclomatic complexity |
| `rustant quality dead-code [path]` | Detect dead/unreachable code |
| `rustant quality duplicates [path]` | Find code duplication |
| `rustant quality debt [path]` | Generate technical debt report |

#### License & SBOM

| Command | Description |
|---------|-------------|
| `rustant license check [--path PATH]` | Check license compliance |
| `rustant license summary [--path PATH]` | Show license summary |
| `rustant sbom generate [--path PATH] [--format FMT] [--output FILE]` | Generate SBOM (cyclonedx, spdx, csv) |
| `rustant sbom diff <old> <new>` | Compare two SBOM versions |

#### Compliance & Risk

| Command | Description |
|---------|-------------|
| `rustant compliance report <framework> [--format FMT]` | Generate compliance report (soc2, iso27001, nist, pci-dss, owasp) |
| `rustant compliance status` | Show compliance status summary |
| `rustant audit export [--start DATE] [--end DATE] [--format FMT]` | Export audit trail (sarif, ocsf, json, csv) |
| `rustant audit verify` | Verify audit trail integrity (Merkle chain) |
| `rustant risk score [path]` | Calculate risk score |
| `rustant risk trend` | Show risk trend over time |

#### Policy & Alerts

| Command | Description |
|---------|-------------|
| `rustant policy list` | List active policies |
| `rustant policy check [--path PATH]` | Check policies against project |
| `rustant policy validate <path>` | Validate a policy file |
| `rustant alerts list [--severity LEVEL]` | List active alerts |
| `rustant alerts triage` | Run AI-powered alert triage |
| `rustant alerts acknowledge <id>` | Acknowledge an alert |
| `rustant alerts resolve <id>` | Resolve an alert |

## Verbosity Levels

| Flag | Level | Description |
|------|-------|-------------|
| (none) | `warn` | Clean output, only warnings and errors |
| `-q` | `error` | Errors only |
| `-v` | `info` | Informational messages |
| `-vv` | `debug` | Debug-level details |
| `-vvv` | `trace` | Full trace output |

## Configuration Priority

Configuration is resolved in this order (highest priority first):

1. CLI flags (`--model`, `--approval`, etc.)
2. Environment variables (`RUSTANT_*` prefix)
3. Workspace config (`.rustant/config.toml`)
4. User config (`~/.config/rustant/config.toml`)
5. Built-in defaults
