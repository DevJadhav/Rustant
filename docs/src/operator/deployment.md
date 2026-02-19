# Deployment & Scaling

## Installation Methods

### Binary Releases

Pre-built binaries for Linux (x86_64, aarch64), macOS (x86_64, aarch64), and Windows (x86_64):

```bash
# GitHub Releases
curl -fsSL https://raw.githubusercontent.com/DevJadhav/Rustant/main/scripts/install.sh | bash

# Homebrew
brew install DevJadhav/rustant/rustant

# cargo-binstall
cargo binstall rustant

# From source
cargo install rustant
```

### systemd Service (Linux)

```ini
[Unit]
Description=Rustant Agent Gateway
After=network.target

[Service]
Type=simple
User=rustant
ExecStart=/usr/local/bin/rustant --config /etc/rustant/config.toml
Restart=on-failure
RestartSec=10
Environment=RUSTANT_GATEWAY__ENABLED=true

[Install]
WantedBy=multi-user.target
```

### launchd Service (macOS)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>dev.rustant.agent</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/local/bin/rustant</string>
        <string>--config</string>
        <string>/etc/rustant/config.toml</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

## Gateway Configuration

For remote access via WebSocket:

```toml
[gateway]
enabled = true
host = "0.0.0.0"       # Listen on all interfaces
port = 18790
auth_tokens = ["your-secret-token"]
max_connections = 50
```

### TLS

The gateway supports TLS via self-signed certificates (generated with `rcgen`) or custom certificates:

```toml
[gateway.tls]
cert_path = "/path/to/cert.pem"
key_path = "/path/to/key.pem"
```

## Resource Considerations

- **Memory**: Base ~50MB, grows with context window and long-term memory
- **CPU**: Minimal idle usage; spikes during tool execution and LLM streaming
- **Disk**: Session data, search indices, and state files in `.rustant/`
- **Network**: LLM API calls, channel polling, MCP server communication
