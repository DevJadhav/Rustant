//! Rustant UI — Tauri dashboard application.
//!
//! Launches the gateway server in the background and opens a webview
//! dashboard that connects to it via WebSocket and REST API.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rustant_core::gateway::{self, GatewayConfig, GatewayServer, SharedGateway};
use rustant_ui::AppState;
use std::sync::Arc;
use tokio::sync::Mutex;

// --- Tauri IPC commands ---
// These must be defined in the binary crate for Tauri 2's macro to work correctly.

#[tauri::command]
async fn get_status(
    state: tauri::State<'_, AppState>,
) -> Result<rustant_ui::DashboardStatus, String> {
    Ok(rustant_ui::fetch_status(&state).await)
}

#[tauri::command]
async fn get_approvals(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    Ok(rustant_ui::fetch_approvals(&state).await)
}

#[tauri::command]
async fn resolve_approval(
    state: tauri::State<'_, AppState>,
    id: String,
    approved: bool,
) -> Result<bool, String> {
    rustant_ui::do_resolve_approval(&state, &id, approved).await
}

#[tauri::command]
async fn get_config(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    rustant_ui::fetch_config(&state).await
}

#[tauri::command]
async fn get_metrics(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(rustant_ui::fetch_metrics(&state).await)
}

fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter("info")
        .init();

    let gateway_port: u16 = std::env::var("RUSTANT_GATEWAY_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(18790);

    let config = GatewayConfig {
        enabled: true,
        host: "127.0.0.1".into(),
        port: gateway_port,
        auth_tokens: Vec::new(),
        max_connections: 50,
        session_timeout_secs: 3600,
    };

    let gw: SharedGateway = Arc::new(Mutex::new(GatewayServer::new(config.clone())));
    let gw_for_server = gw.clone();

    // Spawn the gateway HTTP/WebSocket server in the background
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _gateway_handle = rt.spawn(async move {
        tracing::info!("Starting gateway on {}:{}", config.host, config.port);
        if let Err(e) = gateway::run_gateway(gw_for_server).await {
            tracing::error!("Gateway server error: {}", e);
        }
    });

    let app_state = AppState {
        gateway: gw,
        gateway_port,
    };

    tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_approvals,
            resolve_approval,
            get_config,
            get_metrics,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
