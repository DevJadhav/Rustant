# Productivity Tools

Rustant includes 14 cross-platform productivity tools for personal organization, communication, and daily workflows. These tools cover file management, communication, project templates, time management, finance, learning, and travel.

## Tool Summary

| Tool | Risk Level | Actions | Description |
|------|-----------|---------|-------------|
| `file_organizer` | Write | 4 | Organize, deduplicate, and clean up files |
| `compress` | Write | 3 | Create, extract, and list ZIP archives |
| `http_api` | Execute | 4 | Make HTTP API requests (GET, POST, PUT, DELETE) |
| `template` | Read-only | 2 | Render templates and list available templates |
| `pdf_generate` | Write | 1 | Generate PDF documents from content |
| `pomodoro` | Write | 4 | Pomodoro timer for focused work sessions |
| `inbox` | Write | 6 | Universal inbox for tasks, ideas, and notes |
| `relationships` | Write | 5 | Contact relationship tracking with interaction logs |
| `finance` | Write | 6 | Personal finance tracking and budgeting |
| `flashcards` | Write | 5 | Spaced-repetition flashcard study system |
| `travel` | Write | 5 | Travel itinerary planning and timezone tools |
| `imessage_send` | Write | - | Send iMessages (macOS only) |
| `imessage_read` | Read-only | - | Read iMessage conversations (macOS only) |
| `imessage_contacts` | Read-only | - | Search iMessage contacts (macOS only) |

---

## File Management

### file_organizer

Organize files by type, detect duplicates, and clean up directories.

| Action | Description |
|--------|-------------|
| `organize` | Sort files into categorized folders |
| `dedup` | Find and report duplicate files |
| `cleanup` | Remove empty directories and temp files |
| `preview` | Preview what an organize operation would do |

**Example:**
```json
{
  "action": "organize",
  "path": "~/Downloads"
}
```

### compress

Create and manage ZIP archives.

| Action | Description |
|--------|-------------|
| `create_zip` | Create a ZIP archive from files or directories |
| `extract_zip` | Extract a ZIP archive |
| `list_zip` | List contents of a ZIP archive |

**Example:**
```json
{
  "action": "create_zip",
  "source": "src/",
  "output": "source-backup.zip"
}
```

---

## Web & API

### http_api

Make HTTP API requests with full control over method, headers, and body.

| Action | Description |
|--------|-------------|
| `get` | HTTP GET request |
| `post` | HTTP POST request |
| `put` | HTTP PUT request |
| `delete` | HTTP DELETE request |

**Key Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `action` | string | HTTP method |
| `url` | string | Request URL |
| `headers` | object | HTTP headers |
| `body` | string | Request body (for POST/PUT) |

**Example:**
```json
{
  "action": "post",
  "url": "https://api.example.com/data",
  "headers": {"Content-Type": "application/json"},
  "body": "{\"key\": \"value\"}"
}
```

---

## Documents & Templates

### template

Render text templates with variable substitution.

| Action | Description |
|--------|-------------|
| `render` | Render a template with provided variables |
| `list_templates` | List available template files |

### pdf_generate

Generate PDF documents from Markdown or structured content.

| Action | Description |
|--------|-------------|
| `generate` | Generate a PDF from content |

**Key Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `action` | string | `generate` |
| `content` | string | Content to render (Markdown) |
| `output` | string | Output file path |
| `title` | string | Document title |

---

## Time Management

### pomodoro

Pomodoro technique timer for focused work sessions. State is persisted to `.rustant/pomodoro/`.

| Action | Description |
|--------|-------------|
| `start` | Start a new pomodoro session (25 min default) |
| `stop` | Stop the current session |
| `status` | Check current timer status |
| `history` | View completed session history |

**Example:**
```json
{
  "action": "start",
  "task": "Implement authentication module",
  "duration_mins": 25
}
```

---

## Task Management

### inbox

A universal inbox for capturing tasks, ideas, notes, and quick thoughts. Items can be tagged and marked as done.

| Action | Description |
|--------|-------------|
| `add` | Add a new item to the inbox |
| `list` | List inbox items |
| `search` | Search items by keyword |
| `clear` | Clear completed items |
| `tag` | Add tags to an item |
| `done` | Mark an item as done |

**Example:**
```json
{
  "action": "add",
  "text": "Review PR #142 for security implications",
  "tags": ["work", "security"]
}
```

---

## People & Relationships

### relationships

Track personal and professional relationships with interaction logging.

| Action | Description |
|--------|-------------|
| `add_contact` | Add a new contact |
| `update` | Update contact information |
| `search` | Search contacts |
| `list` | List all contacts |
| `log_interaction` | Log an interaction with a contact |

**Example:**
```json
{
  "action": "log_interaction",
  "name": "Jane Smith",
  "type": "coffee meeting",
  "notes": "Discussed collaboration on ML project"
}
```

---

## Finance

### finance

Personal finance tracking with budgeting and transaction management.

| Action | Description |
|--------|-------------|
| `add_transaction` | Record a financial transaction |
| `list` | List recent transactions |
| `summary` | Get financial summary for a period |
| `budget_check` | Check budget status for a category |
| `set_budget` | Set a monthly budget for a category |
| `export_csv` | Export transactions to CSV |

**Example:**
```json
{
  "action": "add_transaction",
  "amount": -42.50,
  "category": "dining",
  "description": "Team lunch"
}
```

---

## Learning

### flashcards

Spaced-repetition flashcard system for learning and retention.

| Action | Description |
|--------|-------------|
| `add_card` | Add a new flashcard to a deck |
| `study` | Start a study session, retrieving due cards |
| `answer` | Submit an answer and get feedback |
| `list_decks` | List all flashcard decks |
| `stats` | View study statistics and retention rates |

**Example:**
```json
{
  "action": "add_card",
  "deck": "Rust",
  "front": "What is the ownership model in Rust?",
  "back": "Each value has exactly one owner. When the owner goes out of scope, the value is dropped."
}
```

---

## Travel

### travel

Travel itinerary planning with timezone tools and packing lists.

| Action | Description |
|--------|-------------|
| `create_itinerary` | Create a new travel itinerary |
| `add_segment` | Add a flight, hotel, or activity segment |
| `list` | List all itineraries |
| `timezone_convert` | Convert times between timezones |
| `packing_list` | Generate a packing list based on destination and duration |

**Example:**
```json
{
  "action": "timezone_convert",
  "time": "2026-03-15 09:00",
  "from_tz": "America/New_York",
  "to_tz": "Asia/Tokyo"
}
```

---

## iMessage Tools (macOS only)

Three iMessage tools are available on macOS for reading and sending messages. These use AppleScript via `osascript`.

### imessage_send

Send an iMessage to a contact. **Risk Level:** Write -- requires safety approval.

| Parameter | Type | Description |
|-----------|------|-------------|
| `to` | string | Recipient (phone number or email) |
| `message` | string | Message text |

### imessage_read

Read recent messages from a conversation.

| Parameter | Type | Description |
|-----------|------|-------------|
| `from` | string | Contact to read messages from |
| `limit` | integer | Number of recent messages to retrieve |

### imessage_contacts

Search iMessage contacts by name.

| Parameter | Type | Description |
|-----------|------|-------------|
| `query` | string | Contact name to search for |

---

## State Persistence

All stateful productivity tools store their data in subdirectories under `.rustant/`:

| Tool | State Location |
|------|---------------|
| `pomodoro` | `.rustant/pomodoro/` |
| `inbox` | `.rustant/inbox/` |
| `relationships` | `.rustant/relationships/` |
| `finance` | `.rustant/finance/` |
| `flashcards` | `.rustant/flashcards/` |
| `travel` | `.rustant/travel/` |

State files use the atomic write pattern (write to `.tmp`, then rename) to prevent corruption during updates.
