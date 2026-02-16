# Rustant Daily macOS Assistant Guide

## Overview

Rustant can function as a complete daily macOS assistant, managing your calendar, reminders, notes, applications, clipboard, screenshots, file search, and system diagnostics — all through natural language.

## Prerequisites

### macOS Permissions

Grant these permissions in **System Settings > Privacy & Security**:

| Permission | Required For |
|-----------|-------------|
| **Automation** | Calendar, Reminders, Notes, Finder, App Control |
| **Full Disk Access** | File operations, Spotlight search |
| **Notifications** | System notifications |
| **Screen Recording** | Screenshots |

### LLM Provider Setup

#### Cloud (Recommended: Claude Opus 4.6)

```bash
export ANTHROPIC_API_KEY="your-key-here"
```

#### Cloud (Alternative: OpenAI GPT-4o)

```bash
export OPENAI_API_KEY="your-key-here"
```

#### Local (Ollama — Zero Cloud)

```bash
brew install ollama
ollama pull qwen2.5:14b    # Best balance: 9GB, 16GB RAM
ollama serve                # Start the local server
```

See `docs/ollama-setup.md` for full model recommendations.

### Configuration

Copy the appropriate template:

```bash
# Cloud provider
cp docs/daily-assistant-config.toml .rustant/config.toml

# Local Ollama
cp docs/daily-assistant-ollama-config.toml .rustant/config.toml
```

## Available macOS Tools (10)

### 1. Calendar (`macos_calendar`)

Manage macOS Calendar.app events.

**Actions:** `list`, `create`, `delete`, `search`

**Examples:**
- "Show me my calendar for this week"
- "Create a meeting with John tomorrow at 3pm"
- "Delete the dentist appointment"
- "Search for meetings about Q4 review"

### 2. Reminders (`macos_reminders`)

Manage macOS Reminders.app.

**Actions:** `list`, `create`, `complete`, `search`

**Examples:**
- "What are my pending reminders?"
- "Remind me to buy groceries tomorrow"
- "Mark 'send report' as complete"
- "Search reminders for 'dentist'"

### 3. Notes (`macos_notes`)

Manage Apple Notes.app.

**Actions:** `list`, `create`, `read`, `search`

**Examples:**
- "Show my recent notes"
- "Create a note called 'Meeting Notes' with today's discussion"
- "Read the note titled 'Shopping List'"
- "Search notes for 'project ideas'"

### 4. App Control (`macos_app_control`)

Launch, quit, and manage macOS applications.

**Actions:** `open`, `quit`, `list_running`, `activate`

**Examples:**
- "Open Safari"
- "Quit Slack"
- "What apps are currently running?"
- "Bring VS Code to the front"

### 5. Notifications (`macos_notification`)

Send macOS system notifications.

**Examples:**
- "Send me a notification: 'Build complete!'"
- "Notify me with title 'Reminder' and message 'Team call in 5 minutes'"

### 6. Clipboard (`macos_clipboard`)

Read from and write to the system clipboard.

**Actions:** `read`, `write`

**Examples:**
- "What's on my clipboard?"
- "Copy this text to clipboard: Hello World"

### 7. Screenshot (`macos_screenshot`)

Capture screenshots.

**Modes:** `full`, `window`, `region`

**Examples:**
- "Take a screenshot"
- "Screenshot the current window to ~/Desktop/screen.png"

### 8. System Info (`macos_system_info`)

Get system diagnostics.

**Types:** `all`, `battery`, `disk`, `memory`, `network`, `uptime`, `cpu`, `version`

**Examples:**
- "What's my battery level?"
- "How much disk space do I have?"
- "Show me system info"
- "What's my IP address?"

### 9. Spotlight (`macos_spotlight`)

Search files using macOS Spotlight.

**Examples:**
- "Find PDF files about budget"
- "Search for images on my Desktop"
- "Find applications named 'Xcode'"

### 10. Finder (`macos_finder`)

Interact with Finder.

**Actions:** `reveal`, `open_folder`, `get_selection`, `trash`

**Examples:**
- "Reveal this file in Finder"
- "Open my Downloads folder"
- "What files do I have selected in Finder?"
- "Move old-report.pdf to trash"

## Daily Workflow Examples

### Morning Briefing

> "Good morning! Show me today's calendar events, pending reminders, and my battery level."

### Quick Task Management

> "Create a reminder to review the PR at 2pm, then check my email for any urgent messages."

### File Management

> "Find all PDF files on my Desktop from this week, then open the Downloads folder."

### Development Session

> "Open VS Code and Terminal, then show me the git status of this project."

## Security Notes

- Credentials are stored in environment variables, never in config files
- The safety guardian prompts before destructive operations (trash, quit apps)
- All file operations create git checkpoints for reversibility
- Prompt injection detection is enabled by default
- Denied paths protect sensitive files (.env, .ssh, credentials)

## Channel Auto-Reply (CDC)

Rustant can monitor your messaging channels and automatically reply to incoming messages:

```
/cdc on                    # Start background polling
/cdc interval slack 30     # Poll Slack every 30 seconds
/cdc style                 # See learned communication profiles
```

The agent learns each sender's communication style (formality, emoji usage, greeting patterns) and adapts its responses over time.

## ArXiv Paper Implementation

Implement academic papers as working code with full environment isolation:

```
> implement paper 1706.03762 in python
```

This creates a complete project scaffold with:
- Virtual environment (venv for Python, cargo for Rust)
- Test files first (TDD approach)
- Implementation stubs with paper section references
- Dependency management
- README with paper citation

## Troubleshooting

### "Permission denied" for Calendar/Reminders/Notes

Grant Automation permission in System Settings > Privacy & Security > Automation.

### "osascript not found"

Ensure Xcode Command Line Tools are installed: `xcode-select --install`

### Ollama connection refused

Start the Ollama server: `ollama serve`

### Slow tool execution

Increase the tool timeout in `.rustant/config.toml`:
```toml
[tools]
default_timeout_secs = 120
```
