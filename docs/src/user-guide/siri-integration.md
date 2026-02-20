# Siri Integration (macOS)

Control Rustant with your voice via "Hey Siri" on macOS. The integration uses Apple Shortcuts to bridge Siri voice commands to the Rustant daemon.

## Activation Model

Rustant uses an opt-in activation model:

1. **Activate**: "Hey Siri, activate Rustant" — starts daemon, enables voice routing
2. **Use**: "Hey Siri, [task]" — routes through Rustant while active
3. **Deactivate**: "Hey Siri, deactivate Rustant" — stops voice routing

When Rustant is not activated, Siri shortcuts are no-ops and Siri behaves normally.

## Setup

### 1. Install Shortcuts

```bash
rustant siri setup
```

This generates a guided installation for creating the following shortcuts in the Shortcuts app:

| Shortcut | Trigger Phrase |
|----------|---------------|
| Activate Rustant | "Hey Siri, activate Rustant" |
| Deactivate Rustant | "Hey Siri, deactivate Rustant" |
| Ask Rustant | "Hey Siri, ask Rustant [question]" |
| Rustant Calendar | "Hey Siri, check my calendar with Rustant" |
| Rustant Briefing | "Hey Siri, Rustant briefing" |
| Rustant Security | "Hey Siri, Rustant security scan" |
| Rustant Research | "Hey Siri, Rustant research [topic]" |
| Rustant Status | "Hey Siri, Rustant status" |

### 2. Start the Daemon

```bash
rustant daemon start
```

For auto-start on login:

```bash
rustant daemon install
```

## Voice Confirmation Flow

For destructive actions, Rustant asks for voice confirmation:

1. You say: "Hey Siri, delete the temp files"
2. Siri responds: "This will delete 3 files. Should I proceed? Say yes or no."
3. You say: "Yes" or "No"
4. Action proceeds or is cancelled

## Safety

- Siri commands always run in Safe mode (minimum)
- Write/destructive actions require voice confirmation
- All Siri commands are logged in the audit trail
- IPC socket has user-only permissions (0600)

## Configuration

```toml
[siri]
enabled = true
safety_mode = "safe"
max_speech_duration_secs = 30
require_confirmation_for_writes = true
# voice = "Samantha"  # macOS voice name
```

## REPL Commands

| Command | Description |
|---------|-------------|
| `/siri setup` | Install Siri shortcuts |
| `/siri activate` | Activate Siri mode |
| `/siri deactivate` | Deactivate Siri mode |
| `/siri shortcuts` | List available shortcuts |
| `/siri status` | Show Siri integration status |
| `/siri test <phrase>` | Test a Siri phrase |
