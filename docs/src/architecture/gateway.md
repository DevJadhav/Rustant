# Gateway & WebSocket

The WebSocket gateway enables remote access to Rustant via a network interface.

## Architecture

Built on `axum` with WebSocket support:

```toml
[gateway]
enabled = false
host = "127.0.0.1"
port = 18790
auth_tokens = ["secret-token"]
max_connections = 50
```

## Features

- **Task Submission** — Submit tasks via REST API or WebSocket
- **Real-Time Streaming** — Event streaming for tool execution progress
- **Channel Bridging** — Route channel messages through the gateway
- **Session Management** — Create, resume, and manage sessions remotely
- **TLS Support** — Self-signed certificate generation via `rcgen` for local HTTPS

## REST API

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/task` | POST | Submit a new task |
| `/api/status` | GET | Agent status |
| `/api/sessions` | GET | List sessions |
| `/ws` | WebSocket | Real-time event stream |

### SRE Endpoints

When SRE mode is enabled:

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/sre/status` | GET | SRE system status |
| `/api/sre/trust` | GET | Trust level info |
| `/api/sre/circuit` | GET/POST | Circuit breaker control |

## Authentication

Token-based authentication via `auth_tokens` configuration. Tokens are passed as Bearer tokens in the Authorization header or as a query parameter for WebSocket connections.
