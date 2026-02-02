//! Integration tests for voice types, traits, and mock providers.
//!
//! These tests exercise the voice module's public API using mock providers.
//! They do NOT require the `voice` feature flag or any audio hardware.

use rustant_core::voice::{
    AudioChunk, MockSttProvider, MockTtsProvider, SttWakeDetector,
    SynthesisRequest, TranscriptionResult, VoiceActivityDetector,
    audio_convert,
};

#[tokio::test]
async fn test_mock_stt_tts_roundtrip() {
    // Create mock STT that returns a known transcription
    let mock_stt = MockSttProvider::with_responses(vec![TranscriptionResult {
        text: "Hello, Rustant!".into(),
        confidence: 0.95,
        language: Some("en".into()),
        segments: Vec::new(),
        duration_secs: 1.5,
    }]);

    // Create mock TTS
    let mock_tts = MockTtsProvider::new();

    // Simulate: audio → STT → text → TTS → audio
    let input_audio = AudioChunk::new(vec![0.3; 16000], 16000, 1);

    // Step 1: Transcribe
    use rustant_core::voice::SttProvider;
    let transcription = mock_stt.transcribe(&input_audio).await.unwrap();
    assert_eq!(transcription.text, "Hello, Rustant!");

    // Step 2: Synthesize the transcription back to speech
    use rustant_core::voice::TtsProvider;
    let request = SynthesisRequest::new(&transcription.text);
    let result = mock_tts.synthesize(&request).await.unwrap();
    assert!(!result.audio.is_empty());
    assert!(result.duration_secs > 0.0);
}

#[test]
fn test_vad_with_generated_audio() {
    let mut vad = VoiceActivityDetector::new(0.01);

    // Generate a sine wave (speech-like)
    let samples: Vec<f32> = (0..4800)
        .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0).sin() * 0.5)
        .collect();
    let speech = AudioChunk::new(samples, 16000, 1);

    // Feed speech chunks until SpeechStart
    use rustant_core::voice::VadEvent;
    let mut detected = false;
    for _ in 0..5 {
        let event = vad.process_chunk(&speech);
        if event == VadEvent::SpeechStart {
            detected = true;
            break;
        }
    }
    assert!(detected, "VAD should detect speech from sine wave");
    assert!(vad.is_speaking());
}

#[tokio::test]
async fn test_stt_wake_detection_flow() {
    // Set up mock STT that returns the wake word
    let mock_stt = MockSttProvider::with_responses(vec![
        // First call: no wake word
        TranscriptionResult {
            text: "some random noise".into(),
            confidence: 0.8,
            language: None,
            segments: Vec::new(),
            duration_secs: 1.0,
        },
        // Second call: wake word present
        TranscriptionResult {
            text: "hey rustant what is the weather".into(),
            confidence: 0.95,
            language: Some("en".into()),
            segments: Vec::new(),
            duration_secs: 2.0,
        },
    ]);

    let detector = SttWakeDetector::new(
        Box::new(mock_stt),
        vec!["hey rustant".to_string()],
        0.5,
    );

    let chunk = AudioChunk::silence(16000, 1, 480);

    // First detection: no match
    use rustant_core::voice::WakeWordDetector;
    let result1 = detector.detect(&chunk).await.unwrap();
    assert!(result1.is_none());

    // Second detection: match!
    let result2 = detector.detect(&chunk).await.unwrap();
    assert_eq!(result2, Some("hey rustant".to_string()));
}

#[test]
fn test_audio_convert_wav_roundtrip() {
    // Create a simple audio chunk
    let original = AudioChunk::new(
        vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.4, 0.3, 0.2, 0.1],
        16000,
        1,
    );

    // Encode to WAV
    let wav_bytes = audio_convert::encode_wav(&original).unwrap();
    assert!(!wav_bytes.is_empty());

    // Decode back
    let decoded = audio_convert::decode_wav(&wav_bytes).unwrap();
    assert_eq!(decoded.sample_rate, original.sample_rate);
    assert_eq!(decoded.channels, original.channels);
    assert_eq!(decoded.samples.len(), original.samples.len());

    // Check that values are close (i16 quantization introduces small errors)
    for (a, b) in original.samples.iter().zip(decoded.samples.iter()) {
        assert!(
            (a - b).abs() < 0.001,
            "sample mismatch: expected {}, got {}",
            a,
            b
        );
    }
}
