# Voice

Rustant supports voice interaction through text-to-speech (TTS) and speech-to-text (STT) via the OpenAI API.

## Prerequisites

Ensure your OpenAI API key is stored in the OS keychain:

```bash
rustant setup        # Interactive wizard (stores key in OS keychain)
# Or manually:
rustant auth login openai
```

Voice features use the same credential as your LLM provider. Rustant resolves the key from the OS keychain automatically — no environment variables needed.

## Text-to-Speech

Synthesize text into audio:

```bash
rustant voice speak "Hello, I am Rustant!" --voice alloy
```

Available voices: `alloy`, `echo`, `fable`, `onyx`, `nova`, `shimmer`.

The output WAV file is saved to a temporary location and the path is printed.

## TTS/STT Roundtrip

Test the full pipeline — synthesize text, then transcribe it back:

```bash
rustant voice roundtrip "The quick brown fox jumps over the lazy dog"
```

This performs:
1. TTS: Text to audio
2. STT: Audio back to text
3. Comparison of input vs. transcribed output

## Voice Configuration

```toml
[voice]
enabled = true
provider = "openai"
tts_model = "tts-1"
stt_model = "whisper-1"
default_voice = "alloy"
```

## Audio Pipeline

The voice system consists of:

- **TTS Provider** — Converts text to audio chunks (PCM samples)
- **STT Provider** — Converts audio to text transcriptions
- **Audio I/O** — WAV encoding/decoding, sample rate handling
- **Wake Word** — Optional keyword detection for hands-free activation

Audio is represented internally as `AudioChunk` structs containing PCM samples, sample rate, and channel count.
