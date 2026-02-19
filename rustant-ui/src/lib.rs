//! Rustant UI — Shared types and helpers for the Tauri dashboard.
//!
//! Provides application state and helper functions used by both
//! the Tauri IPC commands and the gateway REST API.

use rustant_core::gateway::{GatewayConfig, GatewayServer, SharedGateway};
use serde::Serialize;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Application state shared with Tauri commands.
pub struct AppState {
    pub gateway: SharedGateway,
    pub gateway_port: u16,
}

/// Status response for the dashboard.
#[derive(Serialize)]
pub struct DashboardStatus {
    pub version: String,
    pub uptime_secs: u64,
    pub active_connections: usize,
    pub active_sessions: usize,
    pub total_tool_calls: u64,
    pub total_llm_requests: u64,
    pub gateway_url: String,
}

/// Get the dashboard status from the gateway.
pub async fn fetch_status(state: &AppState) -> DashboardStatus {
    let gw = state.gateway.lock().await;
    DashboardStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: gw.uptime_secs(),
        active_connections: gw.connections().active_count(),
        active_sessions: gw.sessions().active_count(),
        total_tool_calls: gw.total_tool_calls(),
        total_llm_requests: gw.total_llm_requests(),
        gateway_url: format!("ws://127.0.0.1:{}", state.gateway_port),
    }
}

/// Get pending approvals from the gateway.
pub async fn fetch_approvals(state: &AppState) -> Vec<serde_json::Value> {
    let gw = state.gateway.lock().await;
    gw.pending_approvals()
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id.to_string(),
                "tool_name": a.tool_name,
                "description": a.description,
                "risk_level": a.risk_level,
            })
        })
        .collect()
}

/// Resolve an approval by ID.
pub async fn do_resolve_approval(
    state: &AppState,
    id: &str,
    approved: bool,
) -> Result<bool, String> {
    let approval_id: uuid::Uuid = id.parse().map_err(|e| format!("Invalid UUID: {e}"))?;
    let mut gw = state.gateway.lock().await;
    Ok(gw.resolve_approval(&approval_id, approved))
}

/// Get the current configuration JSON.
pub async fn fetch_config(state: &AppState) -> Result<serde_json::Value, String> {
    let gw = state.gateway.lock().await;
    let json_str = gw.config_json();
    serde_json::from_str(json_str).map_err(|e| format!("Invalid config JSON: {e}"))
}

/// Get metrics snapshot.
pub async fn fetch_metrics(state: &AppState) -> serde_json::Value {
    let gw = state.gateway.lock().await;
    serde_json::json!({
        "total_tool_calls": gw.total_tool_calls(),
        "total_llm_requests": gw.total_llm_requests(),
        "uptime_secs": gw.uptime_secs(),
        "active_connections": gw.connections().active_count(),
        "active_sessions": gw.sessions().active_count(),
    })
}

/// Get voice/meeting toggle status.
pub async fn fetch_toggle_status(state: &AppState) -> serde_json::Value {
    let gw = state.gateway.lock().await;
    match gw.toggle_state() {
        Some(ts) => {
            let voice_active = ts.voice_active().await;
            let meeting_active = ts.meeting_active().await;
            let meeting_status = ts.meeting_status().await;
            serde_json::json!({
                "voice_active": voice_active,
                "meeting_active": meeting_active,
                "meeting_title": meeting_status.as_ref().and_then(|s| s.title.clone()),
                "meeting_elapsed_secs": meeting_status.as_ref().map(|s| s.elapsed_secs),
            })
        }
        None => serde_json::json!({
            "voice_active": false,
            "meeting_active": false,
            "available": false,
        }),
    }
}

/// Toggle meeting recording via gateway.
pub async fn do_toggle_meeting(state: &AppState, title: Option<String>) -> Result<String, String> {
    let gw = state.gateway.lock().await;
    let ts = gw
        .toggle_state()
        .cloned()
        .ok_or_else(|| "Toggle state not configured".to_string())?;
    drop(gw);

    if ts.meeting_active().await {
        let result = ts.meeting_stop().await?;
        Ok(format!(
            "Recording stopped. Duration: {}s, Transcript: {} chars",
            result.duration_secs,
            result.transcript.len()
        ))
    } else {
        let config = rustant_core::config::MeetingConfig::default();
        ts.meeting_start(config, title).await?;
        Ok("Recording started".to_string())
    }
}

/// Create a new shared gateway instance for the UI.
pub fn create_gateway() -> SharedGateway {
    Arc::new(Mutex::new(GatewayServer::new(GatewayConfig::default())))
}
