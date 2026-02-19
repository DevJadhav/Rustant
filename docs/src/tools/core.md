# Core Tools

Rustant ships with 17 core tools available on all platforms. These tools cover file operations, version control, shell execution, utilities, web access, and intelligent code editing.

## Tool Summary

| Tool | Risk Level | Description |
|------|-----------|-------------|
| `file_read` | Read-only | Read file contents with optional line range |
| `file_list` | Read-only | List directory contents |
| `file_search` | Read-only | Search for files by name pattern |
| `file_write` | Write | Create or overwrite a file |
| `file_patch` | Write | Apply a patch to an existing file |
| `git_status` | Read-only | Show working tree status |
| `git_diff` | Read-only | Show changes between commits or working tree |
| `git_commit` | Write | Create a git commit |
| `shell_exec` | Execute | Run a shell command |
| `echo` | Read-only | Echo text back (useful for agent reasoning) |
| `datetime` | Read-only | Get current date and time |
| `calculator` | Read-only | Evaluate mathematical expressions |
| `web_search` | Read-only | Search the web |
| `web_fetch` | Read-only | Fetch a URL and extract content |
| `document_read` | Read-only | Read PDF, DOCX, and other document formats |
| `smart_edit` | Write | Fuzzy-location code editing with diff preview |
| `codebase_search` | Read-only | Indexed project-wide code search |

## File Operations

### file_read

Read the contents of a file within the workspace. Supports optional line ranges for reading specific sections of large files.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Path to the file (relative to workspace or absolute) |
| `start_line` | integer | No | Starting line number (1-based, inclusive) |
| `end_line` | integer | No | Ending line number (1-based, inclusive) |

**Example:**
```json
{
  "path": "src/main.rs",
  "start_line": 10,
  "end_line": 50
}
```

All file paths are validated against the workspace boundary. Paths that escape the workspace via `..` components are rejected.

### file_list

List the contents of a directory, showing files and subdirectories.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Directory path to list |

### file_search

Search for files matching a name pattern within the workspace.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `pattern` | string | Yes | Glob or substring pattern to match file names |
| `path` | string | No | Starting directory for search |

### file_write

Create a new file or overwrite an existing file. Content size is capped at 10 MB (`MAX_WRITE_BYTES`).

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Path for the file to write |
| `content` | string | Yes | Content to write |

### file_patch

Apply a text patch to an existing file. Useful for targeted modifications without rewriting the entire file.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Path to the file to patch |
| `patch` | string | Yes | Unified diff or patch content |

## Git Tools

### git_status

Show the current status of the git working tree, including staged, unstaged, and untracked files.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | No | Repository path (defaults to workspace) |

### git_diff

Show differences between commits, staged changes, or the working tree.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | No | Repository path |
| `staged` | boolean | No | Show only staged changes |
| `commit` | string | No | Compare against a specific commit |

### git_commit

Create a git commit with the staged changes.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `message` | string | Yes | Commit message |
| `path` | string | No | Repository path |

## Shell Execution

### shell_exec

Run a shell command in the workspace directory. Supports streaming progress output when a progress channel is configured.

**Risk Level:** Execute -- requires approval in Safe/Cautious modes.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `command` | string | Yes | The shell command to execute |
| `timeout_secs` | integer | No | Timeout in seconds (default: 60) |

**Example:**
```json
{
  "command": "cargo test --workspace",
  "timeout_secs": 120
}
```

## Utilities

### echo

Echoes text back. Primarily used as a no-op tool for agent reasoning steps.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `text` | string | Yes | Text to echo |

### datetime

Returns the current date, time, and timezone. No parameters required.

### calculator

Evaluate a mathematical expression and return the result.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `expression` | string | Yes | Math expression to evaluate |

**Example:**
```json
{ "expression": "sqrt(144) + 3 * 7" }
```

## Web Tools

### web_search

Search the web using a search engine and return results.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | Yes | Search query |
| `max_results` | integer | No | Maximum number of results |

### web_fetch

Fetch the contents of a URL and return the text content. HTML is converted to readable text.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `url` | string | Yes | URL to fetch |

### document_read

Read content from document files such as PDF, DOCX, and other formats.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | Path to the document file |

## Intelligent Editing

### smart_edit

A semantic code editing tool with fuzzy location matching and auto-checkpoint. Accepts natural language descriptions of edit locations (e.g., "the function that handles authentication") and applies precise edits with a unified diff preview.

**Risk Level:** Write

**Features:**
- Exact text matching, line-number patterns (`line 42`), and fuzzy substring matching
- Edit operations: `replace`, `insert_after`, `insert_before`, `delete`
- Automatic checkpoint creation before destructive edits
- Unified diff output showing exactly what changed

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `path` | string | Yes | File to edit |
| `location` | string | Yes | Text, line number, or fuzzy description of the edit location |
| `operation` | string | Yes | One of: `replace`, `insert_after`, `insert_before`, `delete` |
| `content` | string | No | New content (required for replace/insert operations) |

**Example:**
```json
{
  "path": "src/auth.rs",
  "location": "fn validate_token",
  "operation": "replace",
  "content": "fn validate_token(token: &str) -> Result<Claims, AuthError> {\n    // new implementation\n}"
}
```

### codebase_search

Indexed project-wide search powered by `ProjectIndexer`. Supports filtering by code block kind and programming language across 50+ languages.

**Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | Yes | Search query |
| `filter` | string | No | Code block kind filter (e.g., `function`, `struct`, `class`) |
| `language` | string | No | Language filter (e.g., `rust`, `python`, `typescript`) |

The indexer uses `.gitignore`-aware file walking and `FileHashRegistry` for incremental re-indexing, skipping files that have not changed since the last index.

## Safety and Workspace Boundaries

All file-based tools enforce workspace path validation:

- Relative paths are resolved against the workspace root
- Absolute paths must fall within the workspace
- Symlinks are resolved via `canonicalize()` before checking boundaries
- Paths containing `..` that escape the workspace are rejected

The `SafetyGuardian` gates tool execution based on the configured safety mode (`Safe`, `Cautious`, `Paranoid`, `Yolo`). Write and Execute risk-level tools require approval in stricter modes.
