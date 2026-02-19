//! Meeting recording, transcription, and summarization tool.
//!
//! Records audio during meetings, transcribes via OpenAI Whisper API,
//! generates AI summaries, and saves everything to Notes.app.
//! macOS only.

use crate::macos::{require_str, run_command, run_osascript, sanitize_applescript_string};
use crate::registry::Tool;
use async_trait::async_trait;
use chrono::Utc;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use rustant_core::voice::audio_io::audio_convert;
use rustant_core::voice::stt::OpenAiSttProvider;
use rustant_core::voice::stt::SttProvider;
use rustant_core::voice::types::AudioChunk;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tracing::{debug, info, warn};

/// Maximum chunk duration for Whisper API (10 minutes at 16kHz mono).
const CHUNK_SAMPLES: usize = 16000 * 600; // 10 min * 16000 samples/sec

/// Global cancellation channel for the silence monitor background task.
static SILENCE_MONITOR_STOP: LazyLock<Mutex<Option<watch::Sender<bool>>>> =
    LazyLock::new(|| Mutex::new(None));

/// Recording state persisted to `.rustant/meeting-recording.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingState {
    pub is_recording: bool,
    pub started_at: String,
    pub audio_path: String,
    pub meeting_app: Option<String>,
    pub pid: Option<u32>,
    pub title: Option<String>,
    /// Whether the silence monitor background task is active.
    #[serde(default)]
    pub silence_monitor_active: bool,
    /// Whether auto-transcribe/save is enabled (record_and_transcribe flow).
    #[serde(default)]
    pub auto_flow: bool,
}

impl RecordingState {
    fn state_path() -> PathBuf {
        PathBuf::from(".rustant/meeting-recording.json")
    }

    pub fn load() -> Option<Self> {
        let path = Self::state_path();
        let content = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&content) {
            Ok(state) => Some(state),
            Err(e) => {
                warn!(
                    path = %path.display(),
                    error = %e,
                    "Failed to parse recording state file — ignoring corrupted state"
                );
                None
            }
        }
    }

    fn save(&self) -> Result<(), String> {
        let path = Self::state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create .rustant dir: {e}"))?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize recording state: {e}"))?;
        // Atomic write: write to temp file then rename to prevent corruption on crash.
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, &json)
            .map_err(|e| format!("Failed to write recording state temp file: {e}"))?;
        std::fs::rename(&tmp_path, &path)
            .map_err(|e| format!("Failed to rename recording state file: {e}"))?;
        Ok(())
    }

    fn clear() -> Result<(), String> {
        let path = Self::state_path();
        if path.exists() {
            std::fs::remove_file(path)
                .map_err(|e| format!("Failed to remove recording state: {e}"))?;
        }
        Ok(())
    }
}

/// Detect which meeting apps are currently running.
async fn detect_meeting_apps() -> Result<String, String> {
    let script = r#"tell application "System Events"
    set runningApps to name of every application process whose visible is true
    set meetingApps to {}
    if "zoom.us" is in runningApps then set end of meetingApps to "Zoom"
    if "Microsoft Teams" is in runningApps then set end of meetingApps to "Microsoft Teams"
    if "FaceTime" is in runningApps then set end of meetingApps to "FaceTime"
    if "Webex" is in runningApps then set end of meetingApps to "Webex"
    if "Slack" is in runningApps then set end of meetingApps to "Slack"
    if "Discord" is in runningApps then set end of meetingApps to "Discord"
    if (count of meetingApps) is 0 then
        return "No active meeting applications detected."
    end if
    set AppleScript's text item delimiters to ", "
    return "Active meeting apps: " & (meetingApps as string)
end tell"#;
    run_osascript(script).await
}

/// Start audio recording using macOS `afrecord` (AudioToolbox CLI).
pub async fn start_recording(audio_path: &str, sample_rate: u32) -> Result<u32, String> {
    // Use afrecord for WAV recording from default input device
    let child = tokio::process::Command::new("afrecord")
        .args([
            "-f", "WAVE",
            "-d", "LEI16",
            "-c", "1",
            "-r", &sample_rate.to_string(),
            audio_path,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to start afrecord: {e}. Make sure Microphone access is granted in System Settings > Privacy & Security > Microphone."))?;

    let pid = child
        .id()
        .ok_or_else(|| "Failed to get afrecord PID".to_string())?;
    Ok(pid)
}

/// Stop a recording by killing the afrecord process.
pub async fn stop_recording(pid: u32) -> Result<(), String> {
    run_command("kill", &["-SIGINT", &pid.to_string()])
        .await
        .ok();
    // Give it a moment to flush the file
    tokio::time::sleep(Duration::from_millis(500)).await;
    Ok(())
}

/// Transcribe a WAV file, chunking if necessary for the Whisper API size limit.
pub async fn transcribe_audio_file(audio_path: &str, api_key: &str) -> Result<String, String> {
    let wav_data = std::fs::read(audio_path)
        .map_err(|e| format!("Failed to read audio file '{audio_path}': {e}"))?;

    let chunk =
        audio_convert::decode_wav(&wav_data).map_err(|e| format!("Failed to decode WAV: {e}"))?;

    let provider = OpenAiSttProvider::new(api_key);
    let total_samples = chunk.samples.len();

    if total_samples <= CHUNK_SAMPLES {
        // Single chunk — transcribe directly
        let result = provider
            .transcribe(&chunk)
            .await
            .map_err(|e| format!("Transcription failed: {e}"))?;
        return Ok(result.text);
    }

    // Multi-chunk transcription for long recordings
    let mut full_transcript = String::new();
    let mut offset = 0;
    let mut chunk_num = 1;

    while offset < total_samples {
        let end = (offset + CHUNK_SAMPLES).min(total_samples);
        let sub_chunk = AudioChunk::new(
            chunk.samples[offset..end].to_vec(),
            chunk.sample_rate,
            chunk.channels,
        );

        info!(
            chunk = chunk_num,
            samples = end - offset,
            "Transcribing chunk"
        );

        let result = provider
            .transcribe(&sub_chunk)
            .await
            .map_err(|e| format!("Transcription failed on chunk {chunk_num}: {e}"))?;

        if !full_transcript.is_empty() {
            full_transcript.push(' ');
        }
        full_transcript.push_str(&result.text);

        offset = end;
        chunk_num += 1;
    }

    Ok(full_transcript)
}

/// Ensure a folder exists in Notes.app, creating it if needed.
async fn ensure_notes_folder(folder: &str) -> Result<(), String> {
    let folder_safe = sanitize_applescript_string(folder);
    let script = format!(
        r#"tell application "Notes"
    try
        set targetFolder to folder "{folder_safe}"
    on error
        make new folder with properties {{name:"{folder_safe}"}}
    end try
end tell"#
    );
    run_osascript(&script).await.map(|_| ())
}

/// Save a meeting transcript and summary to Notes.app.
pub async fn save_to_notes(
    title: &str,
    summary: &str,
    action_items: &str,
    transcript: &str,
    folder: &str,
) -> Result<String, String> {
    ensure_notes_folder(folder).await?;

    let title_safe = sanitize_applescript_string(title);
    let summary_safe = sanitize_applescript_string(summary);
    let actions_safe = sanitize_applescript_string(action_items);
    let transcript_safe = sanitize_applescript_string(transcript);
    let folder_safe = sanitize_applescript_string(folder);

    let date_str = Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();

    let html_body = format!(
        "<h1>{title_safe}</h1>\
         <p><em>{date_str}</em></p>\
         <h2>Summary</h2>\
         <p>{summary_safe}</p>\
         <h2>Action Items</h2>\
         <p>{actions_safe}</p>\
         <h2>Full Transcript</h2>\
         <p>{transcript_safe}</p>"
    );

    let script = format!(
        r#"tell application "Notes"
    set targetFolder to folder "{folder_safe}"
    make new note at targetFolder with properties {{name:"{title_safe} - {date_str}", body:"{html_body}"}}
    return "Meeting note saved: {title_safe}"
end tell"#
    );
    run_osascript(&script).await
}

/// Get the OpenAI API key via centralized credential resolution.
fn get_api_key() -> Result<String, String> {
    rustant_core::resolve_api_key_by_env("OPENAI_API_KEY")
}

/// Announce a message via macOS text-to-speech (`say` command).
async fn tts_announce(message: &str) {
    if let Err(e) = tokio::process::Command::new("say")
        .args(["-v", "Samantha", message])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
    {
        warn!(error = %e, "TTS announcement failed");
    }
}

/// Background silence monitor that auto-stops recording after sustained silence.
///
/// Records short audio samples at regular intervals, feeds them to a
/// `VoiceActivityDetector`, and tracks consecutive silence duration.
/// When silence exceeds `silence_timeout_secs`, stops recording,
/// announces via TTS, auto-transcribes, and saves to Notes.app.
async fn silence_monitor(
    pid: u32,
    audio_path: String,
    title: String,
    silence_timeout_secs: u64,
    mut cancel_rx: watch::Receiver<bool>,
) {
    use rustant_core::voice::audio_io::record_audio_chunk;
    use rustant_core::voice::vad::VoiceActivityDetector;

    let sample_duration_secs: f32 = 2.0;
    let check_interval = Duration::from_secs(5);
    let mut silence_duration_secs: u64 = 0;
    // Lower threshold for meeting recording (ambient noise expected).
    let mut vad = VoiceActivityDetector::new(0.005);

    info!(
        pid = pid,
        timeout = silence_timeout_secs,
        "Silence monitor started"
    );

    loop {
        // Check for manual cancellation.
        if *cancel_rx.borrow() {
            debug!("Silence monitor cancelled");
            return;
        }

        // Wait for the check interval or cancellation.
        tokio::select! {
            _ = tokio::time::sleep(check_interval) => {}
            _ = cancel_rx.changed() => {
                debug!("Silence monitor cancelled during sleep");
                return;
            }
        }

        // Record a short audio sample for VAD analysis.
        match record_audio_chunk(sample_duration_secs, 16000).await {
            Ok(chunk) => {
                let event = vad.process_chunk(&chunk);
                match event {
                    rustant_core::voice::vad::VadEvent::SpeechStart => {
                        silence_duration_secs = 0;
                        debug!("Speech detected, resetting silence counter");
                    }
                    rustant_core::voice::vad::VadEvent::SpeechEnd => {
                        silence_duration_secs += check_interval.as_secs();
                        debug!(
                            silence_secs = silence_duration_secs,
                            "Speech ended, counting silence"
                        );
                    }
                    rustant_core::voice::vad::VadEvent::NoChange => {
                        if !vad.is_speaking() {
                            silence_duration_secs += check_interval.as_secs();
                        } else {
                            silence_duration_secs = 0;
                        }
                    }
                }
            }
            Err(e) => {
                warn!(error = %e, "Silence monitor: failed to record sample");
                // Don't count as silence on error — keep monitoring.
                continue;
            }
        }

        // Check if silence threshold exceeded.
        if silence_duration_secs >= silence_timeout_secs {
            info!(
                silence_secs = silence_duration_secs,
                "Silence timeout reached, auto-stopping recording"
            );

            // Stop the recording.
            if let Err(e) = stop_recording(pid).await {
                warn!(error = %e, "Silence monitor: failed to stop recording");
            }

            // Announce stop via TTS.
            tts_announce("Meeting recording has stopped due to silence.").await;

            // Auto-transcribe and save.
            auto_transcribe_and_save(&audio_path, &title).await;

            // Clear recording state.
            RecordingState::clear().ok();

            // Clear the monitor sender.
            if let Ok(mut guard) = SILENCE_MONITOR_STOP.lock() {
                *guard = None;
            }

            return;
        }
    }
}

/// Transcribe an audio file and save the result to Notes.app.
async fn auto_transcribe_and_save(audio_path: &str, title: &str) {
    let api_key = match get_api_key() {
        Ok(key) => key,
        Err(e) => {
            warn!(error = %e, "Auto-transcribe: no API key");
            return;
        }
    };

    if !Path::new(audio_path).exists() {
        warn!(path = audio_path, "Auto-transcribe: audio file not found");
        return;
    }

    info!(path = audio_path, "Auto-transcribing audio");
    match transcribe_audio_file(audio_path, &api_key).await {
        Ok(transcript) if !transcript.is_empty() => {
            let folder = "Meeting Transcripts";
            let summary = "(Auto-transcribed — use LLM for a detailed summary)";
            let action_items = "(Auto-transcribed — use LLM to extract action items)";

            match save_to_notes(title, summary, action_items, &transcript, folder).await {
                Ok(msg) => info!(result = %msg, "Auto-save to Notes.app succeeded"),
                Err(e) => warn!(error = %e, "Auto-save to Notes.app failed"),
            }
        }
        Ok(_) => info!("Auto-transcribe: no speech detected in audio"),
        Err(e) => warn!(error = %e, "Auto-transcription failed"),
    }
}

pub struct MacosMeetingRecorderTool;

#[async_trait]
impl Tool for MacosMeetingRecorderTool {
    fn name(&self) -> &str {
        "macos_meeting_recorder"
    }

    fn description(&self) -> &str {
        "Record, transcribe, and summarize meetings on macOS. Actions: \
         detect_meeting (check for active Zoom/Teams/FaceTime/etc.), \
         record (start recording microphone audio — manual flow), \
         record_and_transcribe (RECOMMENDED: announces via TTS, records with silence auto-stop, \
         auto-transcribes, and saves to Notes.app), \
         stop (stop recording — auto-transcribes if using record_and_transcribe flow), \
         transcribe (transcribe audio file via OpenAI Whisper), \
         summarize_to_notes (save transcript summary to Notes.app), \
         status (check recording status)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["detect_meeting", "record", "record_and_transcribe", "stop", "transcribe", "summarize_to_notes", "status"],
                    "description": "Action to perform. Use 'record_and_transcribe' for the full automated flow."
                },
                "title": {
                    "type": "string",
                    "description": "Meeting title (for record and summarize_to_notes)"
                },
                "audio_path": {
                    "type": "string",
                    "description": "Path to audio file (for transcribe)"
                },
                "transcript": {
                    "type": "string",
                    "description": "Transcript text (for summarize_to_notes)"
                },
                "summary": {
                    "type": "string",
                    "description": "Meeting summary (for summarize_to_notes; LLM generates if omitted)"
                },
                "action_items": {
                    "type": "string",
                    "description": "Action items from meeting (for summarize_to_notes)"
                },
                "folder": {
                    "type": "string",
                    "description": "Notes.app folder name (default: Meeting Transcripts)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = require_str(&args, "action", "macos_meeting_recorder")?;

        match action {
            "detect_meeting" => {
                debug!("Detecting active meeting applications");
                let result =
                    detect_meeting_apps()
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_meeting_recorder".into(),
                            message: e,
                        })?;
                Ok(ToolOutput::text(result))
            }

            "record" => {
                // Check if already recording
                if let Some(state) = RecordingState::load()
                    && state.is_recording
                {
                    return Err(ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: format!(
                            "Already recording since {}. Use 'stop' first.",
                            state.started_at
                        ),
                    });
                }

                let title = args["title"].as_str().map(|s| s.to_string());
                let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
                let audio_path = format!("/tmp/rustant_meeting_{timestamp}.wav");

                // Detect meeting app for metadata
                let meeting_app = detect_meeting_apps().await.ok();

                debug!(audio_path = %audio_path, "Starting meeting recording");
                let pid = start_recording(&audio_path, 16000).await.map_err(|e| {
                    ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: e,
                    }
                })?;

                let state = RecordingState {
                    is_recording: true,
                    started_at: Utc::now().to_rfc3339(),
                    audio_path: audio_path.clone(),
                    meeting_app: meeting_app.clone(),
                    pid: Some(pid),
                    title,
                    silence_monitor_active: false,
                    auto_flow: false,
                };
                state.save().map_err(|e| ToolError::ExecutionFailed {
                    name: "macos_meeting_recorder".into(),
                    message: e,
                })?;

                info!(pid = pid, path = %audio_path, "Meeting recording started");
                let app_info = meeting_app.map(|a| format!(" ({a})")).unwrap_or_default();
                Ok(ToolOutput::text(format!(
                    "Recording started{app_info}.\nAudio: {audio_path}\nPID: {pid}\n\nUse action 'stop' to stop recording."
                )))
            }

            "record_and_transcribe" => {
                // Check if already recording
                if let Some(state) = RecordingState::load()
                    && state.is_recording
                {
                    return Err(ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: format!(
                            "Already recording since {}. Use 'stop' first.",
                            state.started_at
                        ),
                    });
                }

                let title = args["title"].as_str().unwrap_or("Meeting").to_string();
                let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
                let audio_path = format!("/tmp/rustant_meeting_{timestamp}.wav");

                // Detect meeting app for metadata
                let meeting_app = detect_meeting_apps().await.ok();

                // Announce recording start via macOS TTS
                tts_announce("Meeting recording has started.").await;

                debug!(audio_path = %audio_path, "Starting meeting recording with auto-flow");
                let pid = start_recording(&audio_path, 16000).await.map_err(|e| {
                    ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: e,
                    }
                })?;

                // Load silence timeout from config (default 60s)
                let silence_timeout = rustant_core::config::load_config(None, None)
                    .ok()
                    .and_then(|c| c.meeting.map(|m| m.silence_timeout_secs))
                    .unwrap_or(60);

                let state = RecordingState {
                    is_recording: true,
                    started_at: Utc::now().to_rfc3339(),
                    audio_path: audio_path.clone(),
                    meeting_app: meeting_app.clone(),
                    pid: Some(pid),
                    title: Some(title.clone()),
                    silence_monitor_active: silence_timeout > 0,
                    auto_flow: true,
                };
                state.save().map_err(|e| ToolError::ExecutionFailed {
                    name: "macos_meeting_recorder".into(),
                    message: e,
                })?;

                // Spawn silence monitor if timeout is configured
                if silence_timeout > 0 {
                    let (cancel_tx, cancel_rx) = watch::channel(false);
                    if let Ok(mut guard) = SILENCE_MONITOR_STOP.lock() {
                        *guard = Some(cancel_tx);
                    }

                    let monitor_path = audio_path.clone();
                    let monitor_title = title.clone();
                    tokio::spawn(silence_monitor(
                        pid,
                        monitor_path,
                        monitor_title,
                        silence_timeout,
                        cancel_rx,
                    ));
                }

                info!(pid = pid, path = %audio_path, auto_flow = true, "Meeting recording started with auto-flow");
                let app_info = meeting_app.map(|a| format!(" ({a})")).unwrap_or_default();
                let silence_info = if silence_timeout > 0 {
                    format!(" Auto-stop after {silence_timeout}s of silence.")
                } else {
                    String::new()
                };
                Ok(ToolOutput::text(format!(
                    "Recording started{app_info} with auto-transcribe.\n\
                     Audio: {audio_path}\nPID: {pid}\n\
                     TTS announced to participants.{silence_info}\n\n\
                     Recording will auto-transcribe and save to Notes.app when stopped.\n\
                     Say 'stop the recording' or use /meeting stop to end manually."
                )))
            }

            "stop" => {
                let state = RecordingState::load().ok_or_else(|| ToolError::ExecutionFailed {
                    name: "macos_meeting_recorder".into(),
                    message: "No active recording found. Use 'record' first.".into(),
                })?;

                if !state.is_recording {
                    return Err(ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: "No active recording. Use 'record' first.".into(),
                    });
                }

                let pid = state.pid.ok_or_else(|| ToolError::ExecutionFailed {
                    name: "macos_meeting_recorder".into(),
                    message: "Recording state corrupted: missing PID.".into(),
                })?;

                // Cancel the silence monitor if running
                if state.silence_monitor_active
                    && let Ok(mut guard) = SILENCE_MONITOR_STOP.lock()
                    && let Some(sender) = guard.take()
                {
                    let _ = sender.send(true);
                }

                debug!(pid = pid, "Stopping meeting recording");
                stop_recording(pid)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: e,
                    })?;

                // Announce stop via TTS if auto-flow
                if state.auto_flow {
                    tts_announce("Meeting recording has stopped.").await;
                }

                // Verify the file exists
                let path = Path::new(&state.audio_path);
                if !path.exists() {
                    warn!(path = %state.audio_path, "Audio file not found after stopping");
                    RecordingState::clear().ok();
                    return Err(ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: format!(
                            "Recording stopped but audio file not found: {}",
                            state.audio_path
                        ),
                    });
                }

                let file_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);

                // Auto-flow: transcribe and save to Notes.app
                if state.auto_flow {
                    let title = state.title.as_deref().unwrap_or("Meeting");

                    info!(
                        path = %state.audio_path,
                        size_bytes = file_size,
                        "Meeting recording stopped, auto-transcribing"
                    );

                    RecordingState::clear().ok();

                    // Transcribe and save
                    let api_key = get_api_key().map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: e,
                    })?;

                    let transcript = transcribe_audio_file(&state.audio_path, &api_key)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_meeting_recorder".into(),
                            message: format!("Auto-transcription failed: {e}"),
                        })?;

                    if transcript.is_empty() {
                        return Ok(ToolOutput::text(
                            "Recording stopped. No speech detected in the audio.".to_string(),
                        ));
                    }

                    let folder = "Meeting Transcripts";
                    let summary = "(Auto-transcribed — use LLM for a detailed summary)";
                    let action_items = "(Auto-transcribed — use LLM to extract action items)";

                    save_to_notes(title, summary, action_items, &transcript, folder)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_meeting_recorder".into(),
                            message: format!("Failed to save to Notes.app: {e}"),
                        })?;

                    Ok(ToolOutput::text(format!(
                        "Recording stopped. Transcript ({} chars) saved to Notes.app \
                         in '{folder}' folder.\nAudio: {}\nSize: {:.1} MB",
                        transcript.len(),
                        state.audio_path,
                        file_size as f64 / 1_048_576.0
                    )))
                } else {
                    // Legacy manual flow — just stop
                    RecordingState::clear().ok();

                    info!(
                        path = %state.audio_path,
                        size_bytes = file_size,
                        "Meeting recording stopped"
                    );
                    Ok(ToolOutput::text(format!(
                        "Recording stopped.\nAudio file: {}\nSize: {:.1} MB\n\nUse action 'transcribe' with audio_path to transcribe.",
                        state.audio_path,
                        file_size as f64 / 1_048_576.0
                    )))
                }
            }

            "transcribe" => {
                let audio_path = require_str(&args, "audio_path", "macos_meeting_recorder")?;
                let api_key = get_api_key().map_err(|e| ToolError::ExecutionFailed {
                    name: "macos_meeting_recorder".into(),
                    message: e,
                })?;

                if !Path::new(audio_path).exists() {
                    return Err(ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: format!("Audio file not found: {audio_path}"),
                    });
                }

                debug!(path = audio_path, "Transcribing audio file");
                let transcript =
                    transcribe_audio_file(audio_path, &api_key)
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: "macos_meeting_recorder".into(),
                            message: e,
                        })?;

                if transcript.is_empty() {
                    return Ok(ToolOutput::text(
                        "Transcription completed but no speech was detected in the audio."
                            .to_string(),
                    ));
                }

                info!(chars = transcript.len(), "Transcription completed");
                Ok(ToolOutput::text(format!(
                    "Transcription ({} characters):\n\n{}",
                    transcript.len(),
                    transcript
                )))
            }

            "summarize_to_notes" => {
                let transcript = require_str(&args, "transcript", "macos_meeting_recorder")?;
                let title = args["title"].as_str().unwrap_or("Untitled Meeting");
                let folder = args["folder"].as_str().unwrap_or("Meeting Transcripts");
                let summary = args["summary"].as_str().unwrap_or(
                    "(Summary not provided — use LLM to generate one from the transcript)",
                );
                let action_items = args["action_items"]
                    .as_str()
                    .unwrap_or("(No action items extracted)");

                debug!(
                    title = title,
                    folder = folder,
                    "Saving transcript to Notes.app"
                );
                let result = save_to_notes(title, summary, action_items, transcript, folder)
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: "macos_meeting_recorder".into(),
                        message: e,
                    })?;

                info!(title = title, "Meeting transcript saved to Notes.app");
                Ok(ToolOutput::text(result))
            }

            "status" => match RecordingState::load() {
                Some(state) if state.is_recording => {
                    let app_info = state
                        .meeting_app
                        .map(|a| format!("\nMeeting app: {a}"))
                        .unwrap_or_default();
                    let title_info = state
                        .title
                        .map(|t| format!("\nTitle: {t}"))
                        .unwrap_or_default();
                    let flow_info = if state.auto_flow {
                        "\nMode: auto-transcribe (record_and_transcribe)"
                    } else {
                        "\nMode: manual (record)"
                    };
                    let silence_info = if state.silence_monitor_active {
                        "\nSilence monitor: active"
                    } else {
                        ""
                    };
                    Ok(ToolOutput::text(format!(
                        "Recording in progress.\nStarted: {}\nAudio: {}{app_info}{title_info}{flow_info}{silence_info}",
                        state.started_at, state.audio_path
                    )))
                }
                _ => Ok(ToolOutput::text("No active recording.".to_string())),
            },

            other => Err(ToolError::InvalidArguments {
                name: "macos_meeting_recorder".to_string(),
                reason: format!(
                    "unknown action '{other}'. Valid: detect_meeting, record, record_and_transcribe, stop, transcribe, summarize_to_notes, status"
                ),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(300)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema_has_required_fields() {
        let tool = MacosMeetingRecorderTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(
            schema["properties"]["action"]["enum"]
                .as_array()
                .unwrap()
                .len()
                >= 7 // detect_meeting, record, record_and_transcribe, stop, transcribe, summarize_to_notes, status
        );
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("action"))
        );
    }

    #[test]
    fn test_tool_metadata() {
        let tool = MacosMeetingRecorderTool;
        assert_eq!(tool.name(), "macos_meeting_recorder");
        assert!(tool.description().contains("Record"));
        assert!(tool.description().contains("transcribe"));
        assert!(tool.description().contains("record_and_transcribe"));
        assert_eq!(tool.risk_level(), RiskLevel::Execute);
        assert_eq!(tool.timeout(), Duration::from_secs(300));
    }

    #[test]
    fn test_recording_state_serde() {
        let state = RecordingState {
            is_recording: true,
            started_at: "2026-02-06T10:00:00Z".to_string(),
            audio_path: "/tmp/test.wav".to_string(),
            meeting_app: Some("Zoom".to_string()),
            pid: Some(12345),
            title: Some("Test Meeting".to_string()),
            silence_monitor_active: false,
            auto_flow: false,
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: RecordingState = serde_json::from_str(&json).unwrap();
        assert!(deserialized.is_recording);
        assert_eq!(deserialized.audio_path, "/tmp/test.wav");
        assert_eq!(deserialized.meeting_app, Some("Zoom".to_string()));
        assert_eq!(deserialized.pid, Some(12345));
        assert_eq!(deserialized.title, Some("Test Meeting".to_string()));
        assert!(!deserialized.silence_monitor_active);
        assert!(!deserialized.auto_flow);
    }

    #[test]
    fn test_recording_state_serde_backward_compat() {
        // Old state format without silence_monitor_active and auto_flow
        let old_json = r#"{
            "is_recording": true,
            "started_at": "2026-02-06T10:00:00Z",
            "audio_path": "/tmp/test.wav",
            "meeting_app": "Zoom",
            "pid": 12345,
            "title": "Old Meeting"
        }"#;
        let state: RecordingState = serde_json::from_str(old_json).unwrap();
        assert!(state.is_recording);
        assert!(!state.silence_monitor_active);
        assert!(!state.auto_flow);
    }

    #[test]
    fn test_recording_state_auto_flow() {
        let state = RecordingState {
            is_recording: true,
            started_at: "2026-02-06T10:00:00Z".to_string(),
            audio_path: "/tmp/test.wav".to_string(),
            meeting_app: None,
            pid: Some(99999),
            title: Some("Auto Meeting".to_string()),
            silence_monitor_active: true,
            auto_flow: true,
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: RecordingState = serde_json::from_str(&json).unwrap();
        assert!(deserialized.silence_monitor_active);
        assert!(deserialized.auto_flow);
    }

    #[test]
    fn test_chunk_calculation() {
        // 10 minutes at 16kHz mono = 9,600,000 samples
        assert_eq!(CHUNK_SAMPLES, 9_600_000);

        // 5-minute audio should fit in one chunk
        let five_min_samples = 16000 * 300;
        assert!(five_min_samples < CHUNK_SAMPLES);

        // 20-minute audio needs 2 chunks
        let twenty_min_samples: usize = 16000 * 1200;
        let chunks_needed = twenty_min_samples.div_ceil(CHUNK_SAMPLES);
        assert_eq!(chunks_needed, 2);

        // 1-hour audio needs 6 chunks
        let one_hour_samples: usize = 16000 * 3600;
        let chunks_needed = one_hour_samples.div_ceil(CHUNK_SAMPLES);
        assert_eq!(chunks_needed, 6);
    }

    #[tokio::test]
    async fn test_invalid_action_returns_error() {
        let tool = MacosMeetingRecorderTool;
        let args = json!({"action": "invalid_action"});
        let result = tool.execute(args).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("unknown action"));
    }

    #[tokio::test]
    async fn test_missing_action_returns_error() {
        let tool = MacosMeetingRecorderTool;
        let args = json!({});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_transcribe_missing_file_returns_error() {
        let tool = MacosMeetingRecorderTool;
        let args = json!({"action": "transcribe", "audio_path": "/tmp/nonexistent_audio_file.wav"});
        let result = tool.execute(args).await;
        // Errors either because API key is missing or file not found
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_summarize_missing_transcript_returns_error() {
        let tool = MacosMeetingRecorderTool;
        let args = json!({"action": "summarize_to_notes"});
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }
}
