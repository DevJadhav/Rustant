# Troubleshooting

## Diagnostics

Run the built-in diagnostic tool:

```
/doctor
```

This checks: LLM connectivity, tool registration, config validation, workspace writability, session index integrity, and audit chain.

## Common Issues

### LLM Provider

**"Authentication failed"**
- Verify your API key is set: `echo $OPENAI_API_KEY`
- Check key validity with the provider's dashboard
- Try `/setup` to re-run the setup wizard

**"Connection timeout"**
- For Ollama: ensure `ollama serve` is running
- Check network connectivity
- Increase timeout: `[llm.retry] max_backoff_ms = 120000`

**"Rate limited (429)"**
- Automatic retry with exponential backoff handles this
- Reduce request frequency or upgrade API tier
- Check `/cost` for usage tracking

### macOS Tools

**"Permission denied" for Calendar/Reminders/Notes**
- Grant Automation permission: System Settings > Privacy & Security > Automation
- Ensure the terminal app has appropriate permissions

**"osascript not found"**
- Install Xcode Command Line Tools: `xcode-select --install`

**GUI scripting not working**
- Enable Accessibility: System Settings > Privacy & Security > Accessibility
- Add your terminal application to the allowed list

### Sessions

**"Session corrupt or unloadable"**
- Sessions auto-recover on startup
- Try `/sessions` to list available sessions
- Delete corrupt session files from `.rustant/sessions/`

### Memory

**"Context window full"**
- Use `/compact` to compress conversation context
- Use `/pin` to preserve important messages before compression
- Start a new session for unrelated tasks

### Tools

**"Tool timeout"**
- Increase timeout in config: `[tools] default_timeout_secs = 120`
- Check if external dependencies (git, kubectl, etc.) are installed

**"Tool not found"**
- Run `/tools` to list available tools
- Some tools are macOS-only (`#[cfg(target_os = "macos")]`)
- Security and ML tools are registered separately

### Browser Automation

**"Chrome not found"**
- Install Chrome or Chromium
- Set path: `[browser] chrome_path = "/path/to/chrome"`

**"CDP connection failed"**
- Close other Chrome debugging sessions
- Try `/browser test` to verify setup

### Channels

**"Channel test failed"**
- Verify credentials: `/auth status`
- Check token expiry: `/auth refresh <provider>`
- Ensure bot has required permissions in the target platform

### Build / Development

**"Compilation errors"**
- Ensure Rust 1.88+: `rustup update stable`
- Install system deps (Linux): `sudo apt-get install -y cmake pkg-config libdbus-1-dev libssl-dev`
- Clean build: `cargo clean && cargo build --workspace`

**"Test failures"**
- Configure git: `git config --global user.email "test@test.com"`
- Set `commit.gpgsign false` for test environment
- Some tests are platform-specific (macOS only)

## Getting Help

- `/help [topic]` — Detailed help for any command or topic
- `/verbose` — Toggle verbose output for debugging
- [GitHub Issues](https://github.com/DevJadhav/Rustant/issues) — Bug reports and feature requests
