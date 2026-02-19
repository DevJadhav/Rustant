# MCP Server Integration

Rustant can connect to external MCP (Model Context Protocol) servers to discover and use additional tools.

## Configuration

Add external MCP servers in `.rustant/config.toml`:

```toml
[[mcp_servers]]
name = "chrome-devtools"
command = "npx"
args = ["-y", "chrome-devtools-mcp@latest"]
auto_connect = true

[[mcp_servers]]
name = "custom-server"
command = "/path/to/my-mcp-server"
args = ["--port", "3000"]
auto_connect = false
env = { "API_KEY" = "env:MY_SERVER_API_KEY" }
working_directory = "/tmp"
```

## Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique server identifier |
| `command` | Yes | Executable to launch |
| `args` | No | Command-line arguments |
| `auto_connect` | No | Connect on startup (default: false) |
| `env` | No | Environment variables for the process |
| `working_directory` | No | Working directory for the process |

## Communication

External servers communicate over NDJSON (newline-delimited JSON) via stdin/stdout using the JSON-RPC 2.0 protocol. The `ProcessTransport` handles spawning and communication.

## Tool Discovery

Once connected, tools from external servers are automatically available to the agent. They appear in `/tools` listing and can be called like any built-in tool.

## Chrome DevTools Example

The Chrome DevTools MCP server provides 26 browser automation tools:

```toml
[[mcp_servers]]
name = "chrome-devtools"
command = "npx"
args = ["-y", "chrome-devtools-mcp@latest"]
auto_connect = true
```

This enables tools like `click`, `navigate`, `evaluate_script`, `screenshot`, etc.

## Building Your Own MCP Server

An MCP server must:

1. Read JSON-RPC 2.0 requests from stdin (one per line)
2. Write JSON-RPC 2.0 responses to stdout (one per line)
3. Implement the `initialize`, `tools/list`, and `tools/call` methods
4. Return tool definitions with JSON Schema parameter descriptions

See the [MCP specification](https://modelcontextprotocol.io/) for details.
