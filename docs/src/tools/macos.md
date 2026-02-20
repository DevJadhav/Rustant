# macOS Native Tools

Rustant includes 25 macOS-native tools that integrate deeply with Apple system services via AppleScript. These tools are only available when running on macOS (`#[cfg(target_os = "macos")]`).

All macOS tools use `sanitize_applescript_string()` for injection prevention, escaping backslashes, quotes, newlines, carriage returns, tabs, and null bytes.

## Tool Overview

| Tool | Actions | Category |
|------|---------|----------|
| `macos_calendar` | 4 | PIM |
| `macos_reminders` | 4 | PIM |
| `macos_notes` | 4 | PIM |
| `macos_app_control` | 4 | System |
| `macos_notification` | - | System |
| `macos_clipboard` | 2 | System |
| `macos_screenshot` | 3 | System |
| `macos_system_info` | 8 | System |
| `macos_spotlight` | - | Search |
| `macos_finder` | 4 | Files |
| `macos_focus_mode` | 4 | System |
| `macos_mail` | 5 | Communication |
| `macos_music` | 7 | Media |
| `macos_shortcuts` | 3 | Automation |
| `macos_meeting_recorder` | 7 | Productivity |
| `macos_daily_briefing` | 3 | Productivity |
| `macos_gui_scripting` | 8 | Screen Automation |
| `macos_accessibility` | 4 | Screen Automation |
| `macos_screen_analyze` | 2 | Screen Automation |
| `macos_contacts` | 4 | PIM |
| `macos_safari` | 6 | Browser |
| `homekit` | 3 | Smart Home |
| `macos_say` | 2 | Media |
| `macos_photos` | 3 | Media |
| `siri_integration` | 5 | Integration |

---

## PIM (Personal Information Management)

### macos_calendar

Manage Calendar.app events.

| Action | Description |
|--------|-------------|
| `list` | List upcoming calendar events |
| `create` | Create a new calendar event |
| `delete` | Delete an event by title or ID |
| `search` | Search events by keyword |

**Example:**
```json
{
  "action": "create",
  "title": "Team standup",
  "start_date": "2026-02-20 09:00",
  "end_date": "2026-02-20 09:30",
  "calendar": "Work"
}
```

### macos_reminders

Manage Reminders.app tasks.

| Action | Description |
|--------|-------------|
| `list` | List reminders from a list |
| `create` | Create a new reminder |
| `complete` | Mark a reminder as complete |
| `search` | Search reminders by text |

### macos_notes

Manage Notes.app content.

| Action | Description |
|--------|-------------|
| `list` | List notes in a folder |
| `create` | Create a new note |
| `read` | Read the contents of a note |
| `search` | Search notes by keyword |

### macos_contacts

Manage Contacts.app entries.

| Action | Description |
|--------|-------------|
| `search` | Search contacts by name or field |
| `get_details` | Get detailed info for a contact |
| `create` | Create a new contact |
| `list_groups` | List contact groups |

---

## System Tools

### macos_app_control

Control running applications.

| Action | Description |
|--------|-------------|
| `open` | Open/launch an application |
| `quit` | Quit an application |
| `list_running` | List all running applications |
| `activate` | Bring an application to the foreground |

### macos_notification

Display a macOS notification. Takes `title` and `message` parameters.

### macos_clipboard

Read from and write to the system clipboard.

| Action | Description |
|--------|-------------|
| `read` | Read current clipboard contents |
| `write` | Write text to the clipboard |

### macos_screenshot

Capture screenshots.

| Action | Description |
|--------|-------------|
| `full` | Capture full screen |
| `window` | Capture a specific window |
| `region` | Capture a screen region |

### macos_system_info

Query system information.

| Info Type | Description |
|-----------|-------------|
| `all` | Complete system overview |
| `battery` | Battery level and charging status |
| `disk` | Disk usage and free space |
| `memory` | RAM usage statistics |
| `network` | Network interfaces and IP addresses |
| `uptime` | System uptime |
| `cpu` | CPU information and load |
| `version` | macOS version details |

### macos_focus_mode

Manage macOS Focus (Do Not Disturb) modes.

| Action | Description |
|--------|-------------|
| `status` | Check current Focus mode status |
| `enable` | Enable Focus mode |
| `disable` | Disable Focus mode |
| `toggle` | Toggle Focus mode on/off |

---

## Search and Files

### macos_spotlight

Search the filesystem using Spotlight. Takes a `query` parameter and returns matching files with metadata.

### macos_finder

Interact with Finder.

| Action | Description |
|--------|-------------|
| `reveal` | Reveal a file or folder in Finder |
| `open_folder` | Open a folder in a new Finder window |
| `get_selection` | Get currently selected items in Finder |
| `trash` | Move a file to the Trash |

---

## Communication

### macos_mail

Interact with Mail.app. The `send` action requires safety approval.

| Action | Description |
|--------|-------------|
| `list_unread` | List unread emails |
| `read` | Read a specific email message |
| `search` | Search emails by keyword |
| `compose` | Draft an email (does not send) |
| `send` | Send a composed email (requires approval) |

---

## Media

### macos_music

Control Music.app (Apple Music / iTunes).

| Action | Description |
|--------|-------------|
| `play` | Start playback |
| `pause` | Pause playback |
| `next` | Skip to next track |
| `previous` | Go to previous track |
| `now_playing` | Show current track info |
| `search_play` | Search and play a song |
| `set_volume` | Set playback volume |

### macos_say

Text-to-speech using the macOS `say` command.

| Action | Description |
|--------|-------------|
| `speak` | Speak the given text |
| `list_voices` | List available system voices |

**Example:**
```json
{
  "action": "speak",
  "text": "Build completed successfully",
  "voice": "Samantha"
}
```

### macos_photos

Interact with Photos.app.

| Action | Description |
|--------|-------------|
| `search` | Search photos by keyword |
| `list_albums` | List photo albums |
| `recent` | Get recent photos |

---

## Automation

### macos_shortcuts

Run macOS Shortcuts (macOS 12+).

| Action | Description |
|--------|-------------|
| `list_shortcuts` | List all available Shortcuts |
| `run_shortcut` | Run a Shortcut by name |
| `run_with_input` | Run a Shortcut with input data |

---

## Productivity

### macos_meeting_recorder

Detect, record, and transcribe meetings. Uses `afrecord` for audio capture and Whisper for transcription. State is persisted to `.rustant/meeting-recording.json`.

| Action | Description |
|--------|-------------|
| `detect_meeting` | Detect if a meeting is in progress |
| `record` | Start recording audio |
| `record_and_transcribe` | Record and automatically transcribe |
| `stop` | Stop the current recording |
| `transcribe` | Transcribe a recorded audio file |
| `summarize_to_notes` | Summarize transcript and save to Notes.app |
| `status` | Check recording status |

Features VAD (Voice Activity Detection) with silence auto-stop (`silence_timeout_secs: 60`).

### macos_daily_briefing

Generate morning or evening briefings aggregated from Calendar, Reminders, Mail, and other sources. Saves output to Notes.app in the "Daily Briefings" folder.

| Action | Description |
|--------|-------------|
| `morning` | Generate morning briefing |
| `evening` | Generate end-of-day summary |
| `custom` | Generate briefing with custom options |

---

## Screen Automation

These tools work together for GUI automation workflows: `app_control` (launch) -> `accessibility` (inspect) -> `gui_scripting` (interact) -> `screen_analyze` (verify via OCR).

### macos_gui_scripting

UI interaction via macOS System Events. Requires Accessibility permission.

| Action | Description |
|--------|-------------|
| `list_elements` | List UI elements of an application |
| `click_element` | Click a UI element by description |
| `type_text` | Type text into the focused element |
| `read_text` | Read text from a UI element |
| `menu_action` | Trigger a menu item |
| `get_window_info` | Get window position and size |
| `click_at_position` | Click at absolute screen coordinates |
| `keyboard_shortcut` | Send a keyboard shortcut |

**Denied applications** (blocked for security): `loginwindow`, `SecurityAgent`, `SystemUIServer`, `WindowServer`, `kernel_task`, `securityd`, `authorizationhost`.

**Example:**
```json
{
  "action": "menu_action",
  "app_name": "Safari",
  "menu_path": "File > New Window"
}
```

### macos_accessibility

Read-only inspection of the accessibility tree. Useful for understanding application UI structure before interacting with it.

| Action | Description |
|--------|-------------|
| `get_tree` | Get the accessibility tree for an application |
| `find_element` | Find a specific UI element |
| `get_focused` | Get the currently focused element |
| `get_frontmost_app` | Get the frontmost application info |

### macos_screen_analyze

OCR and visual analysis using the macOS Vision framework.

| Action | Description |
|--------|-------------|
| `ocr` | Extract text from a screen region or screenshot |
| `find_on_screen` | Find a text string on screen and return coordinates |

### macos_safari

Automate Safari browser via AppleScript.

| Action | Description |
|--------|-------------|
| `navigate` | Navigate to a URL |
| `get_url` | Get the current page URL |
| `get_text` | Get the page text content |
| `run_javascript` | Execute JavaScript in the page |
| `list_tabs` | List open tabs |
| `new_tab` | Open a new tab |

---

## Smart Home

### homekit

Control HomeKit-compatible smart home devices via the macOS Shortcuts CLI (macOS 12+).

| Action | Description |
|--------|-------------|
| `list_shortcuts` | List available HomeKit Shortcuts |
| `run_shortcut` | Run a HomeKit Shortcut |
| `run_with_input` | Run with specific parameters |

Requires that HomeKit Shortcuts are pre-configured in the Shortcuts app.

---

## Integration

### siri_integration

Manage Siri Shortcuts for voice-controlled Rustant interactions. Requires the daemon to be running and the `siri_integration` feature flag enabled.

| Action | Description |
|--------|-------------|
| `install_shortcuts` | Install Rustant Siri Shortcuts into the Shortcuts app |
| `list_shortcuts` | List installed Rustant-related shortcuts |
| `status` | Check Siri integration status (active/inactive) |
| `activate` | Activate Siri integration (creates `~/.rustant/siri_active` flag) |
| `deactivate` | Deactivate Siri integration (removes flag file) |

**Example:**
```json
{
  "action": "activate"
}
```

Activation creates a flag file at `~/.rustant/siri_active` and ensures the background daemon is running to handle Siri voice commands.

---

## Error Handling

All macOS tools sanitize error messages from AppleScript via `sanitize_error_message()`, which removes home directory paths from error output to prevent leaking user information.
