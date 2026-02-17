# First Run

## Provider Setup

Rustant needs an LLM provider to function. Run the interactive setup wizard:

```bash
rustant setup
```

The wizard will guide you through:

1. Choosing a provider (OpenAI, Anthropic, Gemini)
2. Selecting an authentication method (API key or OAuth)
3. Picking a model
4. Setting an approval mode

You can also set your API key directly:

```bash
export OPENAI_API_KEY=sk-...
```

## Running a Single Task

Pass a task as a positional argument:

```bash
rustant "List all Rust files in this directory"
```

## Interactive Mode

Start the interactive REPL by running `rustant` with no arguments:

```bash
rustant
```

To use the TUI (terminal UI) interface instead of the default REPL:

```bash
rustant --tui
```

## Configuration File

Initialize a config file in your project directory:

```bash
rustant config init
```

This creates `.rustant/config.toml` with sensible defaults. See
[Configuration](configuration.md) for the full reference.

## Approval Modes

Rustant supports four safety levels:

| Mode | Behavior |
|------|----------|
| `safe` (default) | Auto-approve read operations, prompt for writes/executes |
| `cautious` | Prompt for most operations |
| `paranoid` | Prompt for every single operation |
| `yolo` | Auto-approve everything (use with caution) |

Set via CLI flag:

```bash
rustant --approval cautious "Delete old log files"
```

Or in your config file under `[safety]`.
