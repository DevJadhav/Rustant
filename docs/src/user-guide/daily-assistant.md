# Rustant Daily macOS Assistant Guide

## Overview

Rustant can function as a complete daily macOS assistant, managing your calendar, reminders, notes, mail, music, contacts, applications, clipboard, screenshots, file search, system diagnostics, GUI scripting, accessibility, OCR, Safari, HomeKit, and more — all through natural language.

## Prerequisites

### macOS Permissions

Grant these permissions in **System Settings > Privacy & Security**:

| Permission | Required For |
|-----------|-------------|
| **Automation** | Calendar, Reminders, Notes, Finder, Mail, Music, Contacts, Safari, App Control |
| **Accessibility** | GUI scripting, accessibility inspection |
| **Full Disk Access** | File operations, Spotlight search |
| **Notifications** | System notifications |
| **Screen Recording** | Screenshots, screen OCR |

### LLM Provider Setup

```bash
# Cloud (Anthropic Claude)
export ANTHROPIC_API_KEY="your-key-here"

# Cloud (OpenAI)
export OPENAI_API_KEY="your-key-here"

# Local (Ollama — Zero Cloud)
brew install ollama
ollama pull qwen2.5:14b
ollama serve
```

See the [Ollama Setup](../ollama-setup.md) guide for full model recommendations.

## Available macOS Tools (24)

### PIM (Calendar, Reminders, Notes, Contacts)

| Tool | Actions | Examples |
|------|---------|---------|
| `macos_calendar` | list, create, delete, search | "Show my calendar this week", "Create meeting tomorrow at 3pm" |
| `macos_reminders` | list, create, complete, search | "Pending reminders?", "Remind me to buy groceries" |
| `macos_notes` | list, create, read, search | "Create a note 'Meeting Notes'", "Search notes for 'ideas'" |
| `macos_contacts` | list, search, create, get | "Find John's phone number", "Add new contact" |

### Communication

| Tool | Actions | Examples |
|------|---------|---------|
| `macos_mail` | list, read, search, compose, send | "Show unread emails", "Send email to John" |
| `imessage_send` | send | "Send iMessage to +1234567890" |
| `imessage_read` | read | "Show recent iMessages" |
| `imessage_search` | search | "Search iMessages for 'dinner'" |

### Media & Entertainment

| Tool | Actions | Examples |
|------|---------|---------|
| `macos_music` | play, pause, next, prev, search, volume | "Play some jazz", "Turn volume to 50%" |
| `photos` | list, search, export | "Show recent photos", "Export vacation photos" |
| `voice_tool` | speak | "Say 'Hello World'" |

### System & Apps

| Tool | Actions | Examples |
|------|---------|---------|
| `macos_app_control` | open, quit, list_running, activate | "Open Safari", "What apps are running?" |
| `macos_notification` | send | "Notify me: Build complete!" |
| `macos_clipboard` | read, write | "What's on my clipboard?" |
| `macos_screenshot` | full, window, region | "Take a screenshot" |
| `macos_system_info` | all, battery, disk, memory, network, cpu | "Battery level?", "Disk space?" |
| `macos_spotlight` | search | "Find PDF files about budget" |
| `macos_finder` | reveal, open_folder, get_selection, trash | "Open Downloads folder" |
| `macos_focus_mode` | get, set | "Enable Do Not Disturb" |
| `macos_shortcuts` | list, run | "Run my 'Morning' shortcut" |

### Screen Automation

| Tool | Actions | Examples |
|------|---------|---------|
| `macos_gui_scripting` | click, type, select_menu, press_key, get_value, set_value, list_elements, wait_for | "Click the Save button in TextEdit" |
| `macos_accessibility` | get_tree, get_focused, get_attributes, find_element | "Show accessibility tree of Finder" |
| `macos_screen_analyze` | ocr_region, ocr_window | "Read text from the screen" |
| `macos_safari` | open_url, get_url, get_source, execute_js, list_tabs, close_tab | "Open google.com in Safari" |

### Smart Home

| Tool | Actions | Examples |
|------|---------|---------|
| `homekit` | list_devices, control, get_status | "Turn on living room lights", "Set thermostat to 72" |

## GUI Automation Workflow

For complex app interactions, chain tools in this order:

1. **`macos_app_control`** — Open/activate the target app
2. **`macos_accessibility`** — Inspect the UI element tree
3. **`macos_gui_scripting`** — Interact with specific elements
4. **`macos_screen_analyze`** — OCR fallback for unreadable elements

**Denied apps** (security): loginwindow, SecurityAgent, SystemUIServer, WindowServer, kernel_task, securityd, authorizationhost

## Daily Workflow Examples

### Morning Briefing

> "Good morning! Show me today's calendar, pending reminders, unread emails, and battery level."

Or use the built-in workflow: `/workflow run morning_briefing`

### Meeting Recording

> "Start recording this meeting and save notes when done."

The meeting recorder detects active meetings, records via `afrecord`, transcribes via Whisper, and saves to Notes.app "Meeting Transcripts" folder.

### Quick Task Management

> "Create a reminder to review the PR at 2pm, then check email for urgent messages."

### Smart Home Control

> "Turn off the lights and set thermostat to 68 degrees."

## Security Notes

- All AppleScript strings go through `sanitize_applescript_string()` for injection prevention
- Mail sending requires explicit approval (write risk level)
- GUI scripting is blocked for security-sensitive apps
- Credentials stored in OS keychain, not config files
- Error messages have home directory paths sanitized

## Troubleshooting

### "Permission denied" for Calendar/Reminders/Notes
Grant Automation permission in System Settings > Privacy & Security > Automation.

### "osascript not found"
Install Xcode Command Line Tools: `xcode-select --install`

### Ollama connection refused
Start the Ollama server: `ollama serve`

### Slow tool execution
Increase timeout: `[tools] default_timeout_secs = 120` in `.rustant/config.toml`
