# Daemon & Siri Architecture

## Background Daemon

The Rustant daemon (`rustant daemon`) is a long-lived background process that maintains a warm agent instance, enabling instant response to commands from CLI, Siri, API, and cron.

### Process Architecture

```
┌─────────────────────────────┐
│       RustantDaemon         │
│                             │
│  ┌──────────┐  ┌─────────┐ │
│  │ IPC      │  │ Gateway │ │    Unix Socket
│  │ Server   │  │ (axum)  │ │ ◄── ~/.rustant/daemon.sock
│  └──────────┘  └─────────┘ │    WebSocket
│  ┌──────────┐  ┌─────────┐ │ ◄── 127.0.0.1:18790
│  │ Cron     │  │  Agent  │ │
│  │ Scheduler│  │ (warm)  │ │
│  └──────────┘  └─────────┘ │
│  ┌──────────────────────────│
│  │ MoE Cache │ Memory │ PID│
│  └──────────────────────────│
└─────────────────────────────┘
```

### IPC Protocol

JSON-encoded messages over Unix socket (`~/.rustant/daemon.sock`):

```
Client                          Daemon
  │ ─── ExecuteCommand ──────► │
  │ ◄── CommandResult ──────── │
  │                             │
  │ ─── StatusQuery ─────────► │
  │ ◄── StatusResponse ─────── │
  │                             │
  │ ─── ConfirmAction ───────► │  (for approval flows)
  │ ◄── CommandResult ──────── │
  │                             │
  │ ─── Shutdown ────────────► │
```

### Lifecycle Management

- **PID file** at `~/.rustant/daemon.pid` prevents double-launch
- `check_daemon_running()` reads PID + sends `kill(pid, 0)` to verify
- Graceful shutdown on SIGTERM/SIGINT
- **launchd** plist for macOS auto-start (`~/Library/LaunchAgents/com.rustant.daemon.plist`)
- **systemd** service for Linux auto-start (`~/.config/systemd/user/rustant.service`)

### Daemon States

```
Starting → Running → ShuttingDown → Stopped
```

## Siri Integration (macOS)

The Siri integration uses Apple Shortcuts as a bridge between "Hey Siri" voice commands and the Rustant daemon.

### Activation Model

Rustant uses an opt-in activation model to avoid interfering with normal Siri behavior:

```
"Hey Siri, activate Rustant"
    │
    ▼
┌────────────────────────┐
│ Shortcut: Activate     │
│ → rustant daemon start │
│   --siri-mode          │
│ → writes siri_active   │
└────────────────────────┘
    │
    ▼  (Rustant is now active)
"Hey Siri, [any task]"
    │
    ▼
┌────────────────────────┐
│ Shortcut: Task         │
│ → check siri_active    │
│ → if active:           │
│   rustant siri send    │
│   "$input"             │
│ → if not: exit (noop)  │
└────────────────────────┘
    │
    ▼
"Hey Siri, deactivate Rustant"
    │
    ▼
┌────────────────────────┐
│ Shortcut: Deactivate   │
│ → removes siri_active  │
│ → optional daemon stop │
└────────────────────────┘
```

### Module Structure

```
rustant-core/src/siri/   (macOS only)
  mod.rs        — Module declarations
  bridge.rs     — SiriBridge: activate, deactivate, send, approve
  shortcuts.rs  — ShortcutGenerator: preset shortcut definitions
  responder.rs  — SiriResponder: format output for voice delivery
```

### Voice Confirmation Flow

For destructive actions, a two-step approval flow via Siri:

1. Agent detects action needs approval → returns `needs_confirmation: true`
2. Shortcut speaks confirmation prompt: "This will delete 3 files. Should I proceed?"
3. Shortcut captures yes/no via "Ask for Input"
4. Shortcut sends `rustant siri confirm <session_id> yes|no`
5. Daemon resumes or cancels the action

### Pillar Compliance

| Pillar | Implementation |
|--------|---------------|
| **Safety** | Siri always runs in Safe mode minimum. Write/destructive require voice confirmation. Daemon runs with user trust level |
| **Security** | IPC socket is user-owned (0600). All commands pass through SecretRedactor. Merkle chain captures all interactions |
| **Interpretability** | Decision log records Siri-originated decisions with `source: "siri"` metadata |
| **Transparency** | Data flow tracker records `SiriVoiceInput` sources. Consent check before first use |
