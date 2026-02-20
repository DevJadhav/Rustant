# Background Daemon

Rustant can run as a persistent background daemon, keeping the agent warm with loaded memory, MoE caches, and active sessions. This enables instant response to CLI commands, Siri voice input, and scheduled cron jobs.

## Quick Start

```bash
# Start the daemon
rustant daemon start

# Check status
rustant daemon status

# Stop the daemon
rustant daemon stop
```

## Architecture

The daemon runs as a background process with:

- **IPC Socket** at `~/.rustant/daemon.sock` (Unix) for local communication
- **PID File** at `~/.rustant/daemon.pid` to prevent duplicate instances
- **Gateway Server** (optional) at `127.0.0.1:18790` for WebSocket access
- **Cron Scheduler** for periodic jobs
- **Warm Agent** with pre-loaded MoE cache and memory

## Auto-Start on Login

### macOS (launchd)

```bash
rustant daemon install
```

This creates `~/Library/LaunchAgents/com.rustant.daemon.plist`. The daemon starts automatically on login and restarts on failure.

To remove:

```bash
rustant daemon uninstall
```

### Linux (systemd)

```bash
rustant daemon install
```

This creates `~/.config/systemd/user/rustant.service`. Enable with:

```bash
systemctl --user enable rustant
```

## IPC Protocol

The daemon listens on a Unix socket for JSON-encoded messages:

| Message | Description |
|---------|-------------|
| `ExecuteCommand` | Run a command and return the result |
| `StatusQuery` | Get daemon state, uptime, active tasks |
| `ConfirmAction` | Confirm or deny a pending approval |
| `Shutdown` | Gracefully stop the daemon |

## Configuration

```toml
[daemon]
auto_start = false
idle_timeout_mins = 0        # 0 = never stop
preload_moe = true           # Warm MoE cache on start
gateway_enabled = true       # Start WebSocket gateway
# ipc_socket_path = "~/.rustant/daemon.sock"
# pid_file_path = "~/.rustant/daemon.pid"
```

## REPL Commands

| Command | Description |
|---------|-------------|
| `/daemon start` | Start the background daemon |
| `/daemon stop` | Stop the daemon |
| `/daemon status` | Show daemon state and uptime |
| `/daemon install` | Install auto-start service |
| `/daemon uninstall` | Remove auto-start service |

## Safety

- The daemon runs with the user's configured safety mode
- IPC socket has user-only permissions (0600)
- PID file prevents unauthorized duplicate instances
- All daemon interactions are logged in the audit trail with Merkle chain
