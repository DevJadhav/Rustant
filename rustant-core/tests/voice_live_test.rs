//! Live integration tests for OpenAI voice providers.
//!
//! These tests make real API calls to OpenAI and require:
//! - `OPENAI_API_KEY` environment variable set
//!
//! Run with:
//!   OPENAI_API_KEY=... cargo test -p rustant-core -- --ignored voice_live

use rustant_core::voice::{
    AudioChunk, OpenAiSttProvider, OpenAiTtsProvider, SttProvider, SynthesisRequest, TtsProvider,
    audio_convert,
};

/// Helper to get the API key from the environment.
fn get_api_key() -> String {
    std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY must be set to run live voice tests")
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and makes real API calls"]
async fn voice_live_test_tts_synthesize_hello() {
    let api_key = get_api_key();
    let tts = OpenAiTtsProvider::new(&api_key);
    let request = SynthesisRequest::new("Hello, this is a test.");

    let result = tts.synthesize(&request).await.unwrap();

    assert!(
        !result.audio.is_empty(),
        "TTS should return non-empty audio"
    );
    assert!(result.duration_secs > 0.0, "Duration should be positive");
    assert_eq!(result.audio.channels, 1, "Should be mono");
    assert!(
        result.audio.sample_rate > 0,
        "Sample rate should be positive"
    );
    assert!(result.characters_used > 0, "Should report characters used");

    println!(
        "TTS result: {} samples, {:.2}s, {}Hz",
        result.audio.samples.len(),
        result.duration_secs,
        result.audio.sample_rate
    );
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and makes real API calls"]
async fn voice_live_test_stt_transcribe_generated_audio() {
    let api_key = get_api_key();

    // Step 1: Use TTS to generate known speech
    let tts = OpenAiTtsProvider::new(&api_key);
    let request = SynthesisRequest::new("Hello world");
    let tts_result = tts.synthesize(&request).await.unwrap();
    assert!(!tts_result.audio.is_empty());

    // Step 2: Feed the TTS output to STT
    let stt = OpenAiSttProvider::new(&api_key);
    let transcription = stt.transcribe(&tts_result.audio).await.unwrap();

    // The transcription should contain "hello" (case-insensitive)
    let text_lower = transcription.text.to_lowercase();
    assert!(
        text_lower.contains("hello"),
        "Transcription '{}' should contain 'hello'",
        transcription.text
    );

    println!("STT transcription: '{}'", transcription.text);
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and makes real API calls"]
async fn voice_live_test_tts_stt_roundtrip() {
    let api_key = get_api_key();
    let original_text = "The quick brown fox jumps over the lazy dog";

    // TTS: text -> audio
    let tts = OpenAiTtsProvider::new(&api_key);
    let request = SynthesisRequest::new(original_text);
    let tts_result = tts.synthesize(&request).await.unwrap();

    // Verify audio is valid WAV-encodable
    let wav_bytes = audio_convert::encode_wav(&tts_result.audio).unwrap();
    assert!(wav_bytes.len() > 44, "WAV should be larger than header");
    let decoded = audio_convert::decode_wav(&wav_bytes).unwrap();
    assert_eq!(decoded.samples.len(), tts_result.audio.samples.len());

    // STT: audio -> text
    let stt = OpenAiSttProvider::new(&api_key);
    let transcription = stt.transcribe(&tts_result.audio).await.unwrap();

    // Verify key words are preserved in the roundtrip
    let text_lower = transcription.text.to_lowercase();
    assert!(
        text_lower.contains("quick") || text_lower.contains("fox") || text_lower.contains("dog"),
        "Roundtrip transcription '{}' should contain key words from original",
        transcription.text
    );
    assert!(transcription.duration_secs > 0.0);

    println!(
        "Roundtrip: '{}' -> audio ({:.2}s) -> '{}'",
        original_text, tts_result.duration_secs, transcription.text
    );
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and makes real API calls"]
async fn voice_live_test_stt_empty_audio_returns_empty() {
    let api_key = get_api_key();
    let stt = OpenAiSttProvider::new(&api_key);
    let empty = AudioChunk::new(vec![], 16000, 1);

    // Empty audio should return empty transcription (handled locally, no API call)
    let result = stt.transcribe(&empty).await.unwrap();
    assert!(result.text.is_empty());
}

#[tokio::test]
#[ignore = "requires OPENAI_API_KEY and makes real API calls"]
async fn voice_live_test_tts_different_voices() {
    let api_key = get_api_key();
    let tts = OpenAiTtsProvider::new(&api_key);

    for voice in &["alloy", "nova"] {
        let request = SynthesisRequest::new("Test").with_voice(*voice);
        let result = tts.synthesize(&request).await.unwrap();
        assert!(
            !result.audio.is_empty(),
            "Voice '{}' should produce audio",
            voice
        );
        println!(
            "Voice '{}': {} samples, {:.2}s",
            voice,
            result.audio.samples.len(),
            result.duration_secs
        );
    }
}
