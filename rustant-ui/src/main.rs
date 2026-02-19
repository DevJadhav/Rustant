//! Rustant UI — Tauri dashboard application.
//!
//! Launches the gateway server in the background and opens a webview
//! dashboard that connects to it via WebSocket and REST API.
//!
//! The gateway server also serves the frontend static files so the
//! dashboard is accessible from any browser at `http://127.0.0.1:<port>/`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use rustant_core::gateway::{GatewayConfig, GatewayServer, SharedGateway, gateway_router};
use rustant_ui::AppState;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tower_http::services::{ServeDir, ServeFile};

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

#[tauri::command]
async fn get_toggle_status(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    Ok(rustant_ui::fetch_toggle_status(&state).await)
}

#[tauri::command]
async fn toggle_meeting(
    state: tauri::State<'_, AppState>,
    title: Option<String>,
) -> Result<String, String> {
    rustant_ui::do_toggle_meeting(&state, title).await
}

/// Resolve the path to the `frontend/` directory containing static assets.
///
/// Checks several locations in order:
/// 1. `RUSTANT_FRONTEND_DIR` environment variable
/// 2. `./frontend` relative to the current executable
/// 3. `./rustant-ui/frontend` relative to the workspace root (development)
fn resolve_frontend_dir() -> PathBuf {
    // 1. Explicit env var
    if let Ok(dir) = std::env::var("RUSTANT_FRONTEND_DIR") {
        let p = PathBuf::from(dir);
        if p.join("index.html").exists() {
            return p;
        }
    }

    // 2. Next to the executable
    if let Ok(exe) = std::env::current_exe()
        && let Some(exe_dir) = exe.parent()
    {
        let p = exe_dir.join("frontend");
        if p.join("index.html").exists() {
            return p;
        }
    }

    // 3. Workspace development layout
    let workspace_candidates = ["rustant-ui/frontend", "frontend"];
    if let Ok(cwd) = std::env::current_dir() {
        for candidate in &workspace_candidates {
            let p = cwd.join(candidate);
            if p.join("index.html").exists() {
                return p;
            }
        }
    }

    // Fallback — return relative path and let ServeDir handle the 404 gracefully
    PathBuf::from("rustant-ui/frontend")
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
        broadcast_capacity: 256,
    };

    let gw: SharedGateway = Arc::new(Mutex::new(GatewayServer::new(config.clone())));
    let gw_for_server = gw.clone();

    // Resolve the frontend static assets directory
    let frontend_dir = resolve_frontend_dir();
    tracing::info!("Serving frontend from: {}", frontend_dir.display());

    // Spawn the gateway HTTP/WebSocket server in the background.
    // The server merges API routes with static file serving so the
    // dashboard is accessible at http://127.0.0.1:<port>/ in any browser.
    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _gateway_handle = rt.spawn(async move {
        let host = config.host.clone();
        let port = config.port;
        let addr = format!("{host}:{port}");

        // Build the gateway API router (WebSocket, REST endpoints)
        let api_router = gateway_router(gw_for_server);

        // Serve static frontend files as a fallback so `/` returns index.html
        let index_file = frontend_dir.join("index.html");
        let static_service =
            ServeDir::new(&frontend_dir).not_found_service(ServeFile::new(&index_file));

        // Merge: API routes take priority, static files are the fallback
        let app = api_router.fallback_service(static_service);

        tracing::info!("Starting gateway + dashboard on http://{}:{}", host, port);

        match tokio::net::TcpListener::bind(&addr).await {
            Ok(listener) => {
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!("Gateway server error: {}", e);
                }
            }
            Err(e) => {
                tracing::error!("Failed to bind to {}: {}", addr, e);
            }
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
            get_toggle_status,
            toggle_meeting,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
