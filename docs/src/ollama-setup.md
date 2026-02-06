# Ollama Setup Guide — Local LLM for Rustant

## Why Ollama?

Ollama lets you run LLMs locally on your Mac. No data leaves your machine — complete privacy. Rustant supports Ollama natively through the OpenAI-compatible API.

## Installation

```bash
brew install ollama
```

## Model Recommendations

| Model | Download | RAM | Tool Calling | Speed | Best For |
|-------|---------|-----|-------------|-------|----------|
| **qwen2.5:14b** | 9 GB | 16 GB | Excellent | Fast | Daily assistant (recommended) |
| **qwen2.5:32b** | 20 GB | 32 GB | Excellent | Medium | Complex reasoning + tools |
| **llama3.1:8b** | 4.7 GB | 8 GB | Good | Very Fast | Simple tasks, low-RAM Macs |
| **llama3.1:70b** | 40 GB | 64 GB | Excellent | Slow | Best open-source reasoning |
| **mistral-nemo:12b** | 7 GB | 16 GB | Good | Fast | Multilingual tasks |
| **deepseek-coder-v2:16b** | 9 GB | 16 GB | Good | Fast | Code-focused tasks |
| **phi-3:14b** | 8 GB | 16 GB | Good | Fast | General tasks |

### Recommendation by Mac

- **MacBook Air (8GB):** `llama3.1:8b` or `qwen2.5:7b`
- **MacBook Pro (16GB):** `qwen2.5:14b` (best balance)
- **MacBook Pro (32GB+):** `qwen2.5:32b` (best quality)
- **Mac Studio (64GB+):** `llama3.1:70b` (top tier open source)

## Setup

```bash
# Pull your chosen model
ollama pull qwen2.5:14b

# Start the server (runs on http://localhost:11434)
ollama serve
```

## Rustant Configuration

Copy the Ollama config template:

```bash
cp docs/daily-assistant-ollama-config.toml .rustant/config.toml
```

Or manually set in `.rustant/config.toml`:

```toml
[llm]
provider = "openai"
model = "qwen2.5:14b"
api_key_env = "OLLAMA_API_KEY"
base_url = "http://localhost:11434/v1"
max_tokens = 4096
temperature = 0.7
context_window = 32768
input_cost_per_million = 0.0
output_cost_per_million = 0.0
use_streaming = true
```

No API key is needed for Ollama — Rustant detects localhost and skips authentication.

## Verify Connectivity

```bash
# Check Ollama is running
curl http://localhost:11434/v1/models

# Run Rustant
cargo run --bin rustant
# Type: "What time is it?" — should use the datetime tool
```

## Tool Calling Support

Most modern Ollama models support function/tool calling, which is required for Rustant to use its tools (calendar, reminders, file ops, etc.).

Models with **verified tool calling support:**
- qwen2.5 (all sizes)
- llama3.1 (all sizes)
- mistral-nemo
- deepseek-coder-v2

Models that do **NOT** support tool calling:
- codellama (older architecture)
- llama2 (older architecture)

## Performance Tips

1. **Close other GPU-intensive apps** when using large models
2. **Use streaming** (`use_streaming = true`) for better perceived latency
3. **Reduce context window** if you experience slowdowns — 8192 is fine for most tasks
4. **Use the smallest model** that meets your needs — 8b models are 3-5x faster than 32b
