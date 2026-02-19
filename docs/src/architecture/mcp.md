# MCP Protocol

Rustant implements the Model Context Protocol (MCP) for tool interoperability with external systems.

## Overview

The `rustant-mcp` crate provides both server and client implementations over JSON-RPC 2.0.

## Server

The MCP server exposes Rustant's tools to external clients:

- **Stdio transport** — Communication over stdin/stdout for subprocess integration
- **Channel transport** — In-process async channel communication
- **Process transport** — Spawn and communicate with child processes via NDJSON

All registered tools (72 base on macOS, 45 on other platforms, plus 33 security tools) are exposed via the `tools/list` and `tools/call` methods.

### Output Limits

MCP output is capped at `MAX_OUTPUT_BYTES` (10MB) to prevent client out-of-memory issues.

## Client

The MCP client connects to external MCP servers for tool discovery:

```toml
[[mcp_servers]]
name = "chrome-devtools"
command = "npx"
args = ["-y", "chrome-devtools-mcp@latest"]
auto_connect = true
env = { "DISPLAY" = ":0" }
working_directory = "/tmp"
```

### Chrome DevTools Integration

The Chrome DevTools MCP server provides 26 browser automation tools:
- Navigation (navigate, go_back, go_forward)
- Interaction (click, type, scroll, hover)
- JavaScript execution (evaluate_script)
- Screenshots and DOM inspection
- Network monitoring and performance traces

## Protocol

JSON-RPC 2.0 with standard MCP methods:

| Method | Description |
|--------|-------------|
| `initialize` | Handshake with capabilities |
| `tools/list` | List available tools with schemas |
| `tools/call` | Execute a tool by name with arguments |
| `resources/list` | List available resources |
| `resources/read` | Read a resource |

## Configuration

```toml
[[mcp_servers]]
name = "my-server"
command = "/path/to/server"
args = ["--flag"]
auto_connect = true
env = { "API_KEY" = "env:MY_API_KEY" }
```

Multiple external MCP servers can be configured. Each is spawned as a child process with NDJSON communication over stdin/stdout.
