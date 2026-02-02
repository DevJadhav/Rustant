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
    let approval_id: uuid::Uuid = id.parse().map_err(|e| format!("Invalid UUID: {}", e))?;
    let mut gw = state.gateway.lock().await;
    Ok(gw.resolve_approval(&approval_id, approved))
}

/// Get the current configuration JSON.
pub async fn fetch_config(state: &AppState) -> Result<serde_json::Value, String> {
    let gw = state.gateway.lock().await;
    let json_str = gw.config_json();
    serde_json::from_str(json_str).map_err(|e| format!("Invalid config JSON: {}", e))
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

/// Create a new shared gateway instance for the UI.
pub fn create_gateway() -> SharedGateway {
    Arc::new(Mutex::new(GatewayServer::new(GatewayConfig::default())))
}
