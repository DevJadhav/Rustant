//! Voice Activity Detection — RMS energy-based, pure computation.

use super::types::AudioChunk;

/// Events produced by the VAD.
#[derive(Debug, Clone, PartialEq)]
pub enum VadEvent {
    /// No change in speech/silence state.
    NoChange,
    /// Voice activity started.
    SpeechStart,
    /// Voice activity ended.
    SpeechEnd,
}

/// A voice activity detector using RMS energy thresholding with smoothing.
pub struct VoiceActivityDetector {
    threshold: f32,
    energy_history: Vec<f32>,
    history_size: usize,
    is_speaking: bool,
    speech_frames: usize,
    silence_frames: usize,
    min_speech_frames: usize,
    min_silence_frames: usize,
}

impl VoiceActivityDetector {
    /// Create a new VAD with the given energy threshold.
    pub fn new(threshold: f32) -> Self {
        Self::with_settings(threshold, 5, 2, 3)
    }

    /// Create a VAD with custom settings.
    pub fn with_settings(
        threshold: f32,
        history_size: usize,
        min_speech_frames: usize,
        min_silence_frames: usize,
    ) -> Self {
        Self {
            threshold,
            energy_history: Vec::with_capacity(history_size),
            history_size: history_size.max(1),
            is_speaking: false,
            speech_frames: 0,
            silence_frames: 0,
            min_speech_frames,
            min_silence_frames,
        }
    }

    /// Process an audio chunk and return a VAD event.
    pub fn process_chunk(&mut self, chunk: &AudioChunk) -> VadEvent {
        let energy = chunk.rms_energy();

        // Update energy history
        if self.energy_history.len() >= self.history_size {
            self.energy_history.remove(0);
        }
        self.energy_history.push(energy);

        let avg_energy = self.current_energy();
        let is_speech = avg_energy > self.threshold;

        if is_speech {
            self.speech_frames += 1;
            self.silence_frames = 0;
        } else {
            self.silence_frames += 1;
            self.speech_frames = 0;
        }

        if !self.is_speaking && self.speech_frames >= self.min_speech_frames {
            self.is_speaking = true;
            return VadEvent::SpeechStart;
        }

        if self.is_speaking && self.silence_frames >= self.min_silence_frames {
            self.is_speaking = false;
            return VadEvent::SpeechEnd;
        }

        VadEvent::NoChange
    }

    /// Whether the VAD is currently in "speaking" state.
    pub fn is_speaking(&self) -> bool {
        self.is_speaking
    }

    /// Reset the VAD state.
    pub fn reset(&mut self) {
        self.energy_history.clear();
        self.is_speaking = false;
        self.speech_frames = 0;
        self.silence_frames = 0;
    }

    /// Average energy over recent history.
    pub fn current_energy(&self) -> f32 {
        if self.energy_history.is_empty() {
            return 0.0;
        }
        self.energy_history.iter().sum::<f32>() / self.energy_history.len() as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vad_silence_detection() {
        let mut vad = VoiceActivityDetector::new(0.01);
        // Feed silence
        for _ in 0..10 {
            let chunk = AudioChunk::silence(16000, 1, 480);
            let event = vad.process_chunk(&chunk);
            assert_ne!(event, VadEvent::SpeechStart);
        }
        assert!(!vad.is_speaking());
    }

    #[test]
    fn test_vad_speech_detection() {
        let mut vad = VoiceActivityDetector::new(0.01);
        // Feed loud signal (min_speech_frames=2 by default)
        let loud = AudioChunk::new(vec![0.5; 480], 16000, 1);
        let event1 = vad.process_chunk(&loud);
        // First frame might not trigger yet (needs min_speech_frames)
        assert_eq!(event1, VadEvent::NoChange);

        let event2 = vad.process_chunk(&loud);
        assert_eq!(event2, VadEvent::SpeechStart);
        assert!(vad.is_speaking());
    }

    #[test]
    fn test_vad_speech_to_silence() {
        // Use history_size=1 so smoothing doesn't delay silence detection
        let mut vad = VoiceActivityDetector::with_settings(0.01, 1, 2, 3);
        let loud = AudioChunk::new(vec![0.5; 480], 16000, 1);
        let silent = AudioChunk::silence(16000, 1, 480);

        // Trigger speech (need min_speech_frames=2)
        vad.process_chunk(&loud);
        vad.process_chunk(&loud);
        assert!(vad.is_speaking());

        // Now silence (need min_silence_frames=3)
        vad.process_chunk(&silent);
        assert!(vad.is_speaking()); // 1 silence frame, not enough
        vad.process_chunk(&silent);
        assert!(vad.is_speaking()); // 2 silence frames, not enough
        let event = vad.process_chunk(&silent);
        assert_eq!(event, VadEvent::SpeechEnd); // 3 silence frames → SpeechEnd
        assert!(!vad.is_speaking());
    }

    #[test]
    fn test_vad_threshold_sensitivity() {
        // Very low threshold detects quiet speech
        let mut vad = VoiceActivityDetector::new(0.001);
        let quiet = AudioChunk::new(vec![0.01; 480], 16000, 1);
        vad.process_chunk(&quiet);
        let event = vad.process_chunk(&quiet);
        assert_eq!(event, VadEvent::SpeechStart);

        // Higher threshold does NOT detect the same signal
        let mut vad_high = VoiceActivityDetector::new(0.1);
        vad_high.process_chunk(&quiet);
        let event = vad_high.process_chunk(&quiet);
        assert_eq!(event, VadEvent::NoChange);
        assert!(!vad_high.is_speaking());
    }

    #[test]
    fn test_vad_reset() {
        let mut vad = VoiceActivityDetector::new(0.01);
        let loud = AudioChunk::new(vec![0.5; 480], 16000, 1);
        vad.process_chunk(&loud);
        vad.process_chunk(&loud);
        assert!(vad.is_speaking());

        vad.reset();
        assert!(!vad.is_speaking());
        assert!((vad.current_energy() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_vad_energy_history() {
        let mut vad = VoiceActivityDetector::with_settings(0.01, 3, 1, 1);
        let chunk1 = AudioChunk::new(vec![0.3; 100], 16000, 1);
        let chunk2 = AudioChunk::new(vec![0.6; 100], 16000, 1);
        let chunk3 = AudioChunk::new(vec![0.9; 100], 16000, 1);

        vad.process_chunk(&chunk1);
        let e1 = vad.current_energy();
        assert!(e1 > 0.0);

        vad.process_chunk(&chunk2);
        let e2 = vad.current_energy();
        assert!(e2 > e1); // average should increase

        vad.process_chunk(&chunk3);
        let e3 = vad.current_energy();
        assert!(e3 > e2); // average should increase further

        // History is full (size=3), next push evicts oldest
        let silence = AudioChunk::silence(16000, 1, 100);
        vad.process_chunk(&silence);
        let e4 = vad.current_energy();
        assert!(e4 < e3); // evicted loud chunk, added silence
    }
}
