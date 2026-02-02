# Developing Plugins

Plugins extend Rustant's capabilities by registering new tools, hooks, and channels.

## Plugin Types

| Type | Loading | Sandbox | Use Case |
|------|---------|---------|----------|
| Native | `libloading` (.so/.dll/.dylib) | None | High-performance extensions |
| WASM | `wasmi` | Sandboxed | Untrusted/third-party plugins |
| Managed | In-process | Rust trait | Internal extensions |

## Plugin Trait

```rust
#[async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;
    async fn on_load(&mut self) -> Result<(), PluginError>;
    async fn on_unload(&mut self) -> Result<(), PluginError>;
    fn tools(&self) -> Vec<PluginToolDef>;
    fn hooks(&self) -> Vec<(HookPoint, Box<dyn Hook>)>;
}
```

## Hook System

Plugins can intercept agent behavior at seven hook points:

| Hook Point | When |
|------------|------|
| `BeforeToolExecution` | Before a tool runs |
| `AfterToolExecution` | After a tool completes |
| `BeforeLlmRequest` | Before sending to LLM |
| `AfterLlmResponse` | After LLM responds |
| `OnSessionStart` | When a session begins |
| `OnSessionEnd` | When a session ends |
| `OnError` | When an error occurs |

Each hook returns a `HookResult`:

- `Continue` — Allow execution to proceed
- `Block(reason)` — Stop execution with a reason
- `Modified` — Continue with modified context

## Creating a Native Plugin

1. Create a new Rust library crate with `crate-type = ["cdylib"]`
2. Implement the `Plugin` trait
3. Export the creation function:

```rust
#[no_mangle]
pub extern "C" fn rustant_plugin_create() -> Box<dyn Plugin> {
    Box::new(MyPlugin::new())
}
```

4. Build: `cargo build --release`
5. Place the `.so`/`.dll`/`.dylib` in the plugins directory

## Plugin Security

Plugins declare required capabilities:

- `ToolRegistration` — Can register new tools
- `HookRegistration` — Can register hooks
- `FileSystemAccess` — Can access the filesystem
- `NetworkAccess` — Can make network requests
- `ShellExecution` — Can execute shell commands
- `SecretAccess` — Can access stored credentials

The `PluginSecurityValidator` verifies that plugins only request capabilities they need and blocks plugins that exceed their declared scope.

## CLI

```bash
rustant plugin list                # List discovered plugins
rustant plugin info <name>         # Show plugin details
```

## Configuration

```toml
[plugins]
plugins_dir = "~/.config/rustant/plugins"
allowed_plugins = ["my-plugin"]
```
