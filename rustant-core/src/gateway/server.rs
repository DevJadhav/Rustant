//! WebSocket gateway server built on axum.

use super::GatewayConfig;
use super::auth::GatewayAuth;
use super::connection::ConnectionManager;
use super::events::{ClientMessage, GatewayEvent, ServerMessage};
use super::session::SessionManager;
use axum::{
    Router,
    extract::{
        Path, State,
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use futures::SinkExt;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast};
use uuid::Uuid;

/// Provides channel and node status snapshots for the gateway.
///
/// Implement this trait to wire real `ChannelManager` / `NodeManager` data
/// into the gateway's `ListChannels` and `ListNodes` handlers.
pub trait StatusProvider: Send + Sync {
    /// Return `(name, status_string)` pairs for every registered channel.
    fn channel_statuses(&self) -> Vec<(String, String)>;
    /// Return `(name, health_string)` pairs for every registered node.
    fn node_statuses(&self) -> Vec<(String, String)>;
}

/// Thread-safe shared gateway reference for axum handlers.
pub type SharedGateway = Arc<Mutex<GatewayServer>>;

/// The WebSocket gateway server.
pub struct GatewayServer {
    config: GatewayConfig,
    auth: GatewayAuth,
    connections: ConnectionManager,
    sessions: SessionManager,
    event_tx: broadcast::Sender<GatewayEvent>,
    started_at: chrono::DateTime<Utc>,
    status_provider: Option<Box<dyn StatusProvider>>,
    /// Counters for metrics dashboard.
    total_tool_calls: u64,
    total_llm_requests: u64,
    /// Pending approvals for security queue (HashMap for O(1) lookup/removal).
    pending_approvals: std::collections::HashMap<Uuid, PendingApproval>,
    /// Snapshot of configuration JSON for the UI.
    config_json: String,
    /// Shared toggle state for voice/meeting sessions.
    toggle_state: Option<Arc<crate::voice::toggle::ToggleState>>,
    /// Shared audit store for the `/api/audit` endpoint.
    audit_store: Option<Arc<Mutex<crate::audit::AuditStore>>>,
}

/// A pending approval request awaiting user decision.
#[derive(Debug, Clone)]
pub struct PendingApproval {
    /// Unique approval ID.
    pub id: Uuid,
    /// Tool requesting approval.
    pub tool_name: String,
    /// Description of the action.
    pub description: String,
    /// Risk level string.
    pub risk_level: String,
}

impl std::fmt::Debug for GatewayServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GatewayServer")
            .field("config", &self.config)
            .field("connections", &self.connections.active_count())
            .field("sessions", &self.sessions.total_count())
            .finish()
    }
}

impl GatewayServer {
    /// Create a new gateway server from configuration.
    pub fn new(config: GatewayConfig) -> Self {
        let auth = GatewayAuth::from_config(&config);
        let connections = ConnectionManager::new(config.max_connections);
        let sessions = SessionManager::new();
        let (event_tx, _) = broadcast::channel(config.broadcast_capacity);

        Self {
            config,
            auth,
            connections,
            sessions,
            event_tx,
            started_at: Utc::now(),
            status_provider: None,
            total_tool_calls: 0,
            total_llm_requests: 0,
            pending_approvals: std::collections::HashMap::new(),
            config_json: "{}".to_string(),
            toggle_state: None,
            audit_store: None,
        }
    }

    /// Get a reference to the gateway configuration.
    pub fn config(&self) -> &GatewayConfig {
        &self.config
    }

    /// Get a reference to the auth validator.
    pub fn auth(&self) -> &GatewayAuth {
        &self.auth
    }

    /// Get a mutable reference to the connection manager.
    pub fn connections_mut(&mut self) -> &mut ConnectionManager {
        &mut self.connections
    }

    /// Get a reference to the connection manager.
    pub fn connections(&self) -> &ConnectionManager {
        &self.connections
    }

    /// Get a mutable reference to the session manager.
    pub fn sessions_mut(&mut self) -> &mut SessionManager {
        &mut self.sessions
    }

    /// Get a reference to the session manager.
    pub fn sessions(&self) -> &SessionManager {
        &self.sessions
    }

    /// Subscribe to gateway events.
    pub fn subscribe(&self) -> broadcast::Receiver<GatewayEvent> {
        self.event_tx.subscribe()
    }

    /// Broadcast an event to all subscribers.
    pub fn broadcast(&self, event: GatewayEvent) -> usize {
        self.event_tx.send(event).unwrap_or(0)
    }

    /// Uptime in seconds since the server was created.
    pub fn uptime_secs(&self) -> u64 {
        let elapsed = Utc::now() - self.started_at;
        elapsed.num_seconds().max(0) as u64
    }

    /// Set a status provider for channel/node listings.
    pub fn set_status_provider(&mut self, provider: Box<dyn StatusProvider>) {
        self.status_provider = Some(provider);
    }

    /// Set the shared toggle state for voice/meeting controls.
    pub fn set_toggle_state(&mut self, state: Arc<crate::voice::toggle::ToggleState>) {
        self.toggle_state = Some(state);
    }

    /// Get a reference to the toggle state (if set).
    pub fn toggle_state(&self) -> Option<&Arc<crate::voice::toggle::ToggleState>> {
        self.toggle_state.as_ref()
    }

    /// Set the shared audit store for the audit API endpoint.
    pub fn set_audit_store(&mut self, store: Arc<Mutex<crate::audit::AuditStore>>) {
        self.audit_store = Some(store);
    }

    /// Get a reference to the audit store (if configured).
    pub fn audit_store(&self) -> Option<&Arc<Mutex<crate::audit::AuditStore>>> {
        self.audit_store.as_ref()
    }

    /// Number of active connections.
    pub fn active_connections(&self) -> usize {
        self.connections.active_count()
    }

    /// Number of active sessions.
    pub fn active_sessions(&self) -> usize {
        self.sessions.active_count()
    }

    /// Increment the tool call counter.
    pub fn record_tool_call(&mut self) {
        self.total_tool_calls += 1;
    }

    /// Increment the LLM request counter.
    pub fn record_llm_request(&mut self) {
        self.total_llm_requests += 1;
    }

    /// Total tool calls since startup.
    pub fn total_tool_calls(&self) -> u64 {
        self.total_tool_calls
    }

    /// Total LLM requests since startup.
    pub fn total_llm_requests(&self) -> u64 {
        self.total_llm_requests
    }

    /// Add a pending approval request.
    pub fn add_approval(&mut self, approval: PendingApproval) {
        let id = approval.id;
        let tool_name = approval.tool_name.clone();
        let description = approval.description.clone();
        let risk_level = approval.risk_level.clone();
        self.pending_approvals.insert(id, approval);
        self.broadcast(GatewayEvent::ApprovalRequest {
            approval_id: id,
            tool_name,
            description,
            risk_level,
        });
    }

    /// Resolve a pending approval (returns true if found). O(1) via HashMap.
    pub fn resolve_approval(&mut self, approval_id: &Uuid, _approved: bool) -> bool {
        self.pending_approvals.remove(approval_id).is_some()
    }

    /// Get all pending approvals.
    pub fn pending_approvals(&self) -> Vec<&PendingApproval> {
        self.pending_approvals.values().collect()
    }

    /// Set the configuration JSON snapshot for the UI.
    pub fn set_config_json(&mut self, json: String) {
        self.config_json = json;
    }

    /// Get the current configuration JSON snapshot.
    pub fn config_json(&self) -> &str {
        &self.config_json
    }

    /// Handle a client message and produce a server response.
    pub fn handle_client_message(&mut self, msg: ClientMessage, conn_id: Uuid) -> ServerMessage {
        match msg {
            ClientMessage::Authenticate { token } => {
                if self.auth.validate(&token) {
                    self.connections.authenticate(&conn_id);
                    self.broadcast(GatewayEvent::Connected {
                        connection_id: conn_id,
                    });
                    ServerMessage::Authenticated {
                        connection_id: conn_id,
                    }
                } else {
                    ServerMessage::AuthFailed {
                        reason: "Invalid token".to_string(),
                    }
                }
            }
            ClientMessage::SubmitTask { description } => {
                if !self.connections.is_authenticated(&conn_id) {
                    return ServerMessage::AuthFailed {
                        reason: "Not authenticated".to_string(),
                    };
                }
                let task_id = Uuid::new_v4();
                let _session_id = self.sessions.create_session(conn_id);
                self.broadcast(GatewayEvent::TaskSubmitted {
                    task_id,
                    description: description.clone(),
                });
                ServerMessage::Event {
                    event: GatewayEvent::TaskSubmitted {
                        task_id,
                        description,
                    },
                }
            }
            ClientMessage::CancelTask { task_id } => {
                if !self.connections.is_authenticated(&conn_id) {
                    return ServerMessage::AuthFailed {
                        reason: "Not authenticated".to_string(),
                    };
                }
                self.broadcast(GatewayEvent::TaskCompleted {
                    task_id,
                    success: false,
                    summary: "Cancelled by client".to_string(),
                });
                ServerMessage::Event {
                    event: GatewayEvent::TaskCompleted {
                        task_id,
                        success: false,
                        summary: "Cancelled by client".to_string(),
                    },
                }
            }
            ClientMessage::GetStatus => ServerMessage::StatusResponse {
                connected_clients: self.connections.active_count(),
                active_tasks: self.sessions.active_count(),
                uptime_secs: self.uptime_secs(),
            },
            ClientMessage::Ping { timestamp } => ServerMessage::Pong { timestamp },
            ClientMessage::ListChannels => {
                let channels = self
                    .status_provider
                    .as_ref()
                    .map(|p| p.channel_statuses())
                    .unwrap_or_default();
                ServerMessage::ChannelStatus { channels }
            }
            ClientMessage::ListNodes => {
                let nodes = self
                    .status_provider
                    .as_ref()
                    .map(|p| p.node_statuses())
                    .unwrap_or_default();
                ServerMessage::NodeStatus { nodes }
            }
            ClientMessage::GetMetrics => ServerMessage::MetricsResponse {
                active_connections: self.connections.active_count(),
                active_sessions: self.sessions.active_count(),
                total_tool_calls: self.total_tool_calls,
                total_llm_requests: self.total_llm_requests,
                uptime_secs: self.uptime_secs(),
            },
            ClientMessage::GetConfig => ServerMessage::ConfigResponse {
                config_json: self.config_json.clone(),
            },
            ClientMessage::ApprovalDecision {
                approval_id,
                approved,
                reason: _,
            } => {
                let found = self.resolve_approval(&approval_id, approved);
                ServerMessage::ApprovalAck {
                    approval_id,
                    accepted: found,
                }
            }
        }
    }
}

/// Build an axum Router with `/ws`, `/health`, and REST API routes.
pub fn router(shared: SharedGateway) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
        .route("/api/status", get(api_status_handler))
        .route("/api/sessions", get(api_sessions_handler))
        .route("/api/config", get(api_config_handler))
        .route("/api/metrics", get(api_metrics_handler))
        .route("/api/audit", get(api_audit_handler))
        .route("/api/approvals", get(api_approvals_handler))
        .route("/api/approval/{id}", post(api_approval_decision_handler))
        .route("/api/voice/start", post(api_voice_start_handler))
        .route("/api/voice/stop", post(api_voice_stop_handler))
        .route("/api/voice/status", get(api_voice_status_handler))
        .route("/api/meeting/start", post(api_meeting_start_handler))
        .route("/api/meeting/stop", post(api_meeting_stop_handler))
        .route("/api/meeting/status", get(api_meeting_status_handler))
        .with_state(shared)
}

/// WebSocket upgrade handler.
async fn ws_handler(ws: WebSocketUpgrade, State(gw): State<SharedGateway>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, gw))
}

/// Health check endpoint.
async fn health_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let body = serde_json::json!({
        "status": "ok",
        "connections": gw.active_connections(),
        "sessions": gw.active_sessions(),
        "uptime_secs": gw.uptime_secs(),
    });
    axum::Json(body)
}

/// REST API: Get server status overview.
async fn api_status_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let channels = gw
        .status_provider
        .as_ref()
        .map(|p| p.channel_statuses())
        .unwrap_or_default();
    let nodes = gw
        .status_provider
        .as_ref()
        .map(|p| p.node_statuses())
        .unwrap_or_default();

    let body = serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "uptime_secs": gw.uptime_secs(),
        "active_connections": gw.active_connections(),
        "active_sessions": gw.active_sessions(),
        "total_tool_calls": gw.total_tool_calls(),
        "total_llm_requests": gw.total_llm_requests(),
        "channels": channels.iter().map(|(n, s)| serde_json::json!({"name": n, "status": s})).collect::<Vec<_>>(),
        "nodes": nodes.iter().map(|(n, s)| serde_json::json!({"name": n, "status": s})).collect::<Vec<_>>(),
        "pending_approvals": gw.pending_approvals().len(),
    });
    axum::Json(body)
}

/// REST API: Get active sessions.
async fn api_sessions_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let body = serde_json::json!({
        "total": gw.active_sessions(),
        "sessions": gw.sessions().list_active().iter().map(|s| {
            serde_json::json!({
                "id": s.session_id.to_string(),
                "connection_id": s.connection_id.to_string(),
                "state": format!("{:?}", s.state),
                "created_at": s.created_at.to_rfc3339(),
            })
        }).collect::<Vec<_>>(),
    });
    axum::Json(body)
}

/// REST API: Get current configuration snapshot.
async fn api_config_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let config_json = gw.config_json();
    match serde_json::from_str::<serde_json::Value>(config_json) {
        Ok(val) => axum::Json(val),
        Err(_) => axum::Json(serde_json::json!({"error": "Invalid config JSON"})),
    }
}

/// REST API: Get metrics snapshot.
async fn api_metrics_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let body = serde_json::json!({
        "active_connections": gw.active_connections(),
        "active_sessions": gw.active_sessions(),
        "total_tool_calls": gw.total_tool_calls(),
        "total_llm_requests": gw.total_llm_requests(),
        "uptime_secs": gw.uptime_secs(),
    });
    axum::Json(body)
}

/// REST API: Get audit trail — returns recent execution traces.
async fn api_audit_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let store_arc = match gw.audit_store() {
        Some(s) => s.clone(),
        None => {
            return axum::Json(serde_json::json!({
                "entries": [],
                "total": 0,
                "note": "Audit store not configured",
            }));
        }
    };
    drop(gw); // Release gateway lock before locking audit store

    let store = store_arc.lock().await;
    let traces = store.latest(50);
    let entries: Vec<serde_json::Value> = traces
        .iter()
        .map(|t| {
            serde_json::json!({
                "trace_id": t.trace_id.to_string(),
                "goal": t.goal,
                "started_at": t.started_at.to_rfc3339(),
                "completed_at": t.completed_at.map(|dt| dt.to_rfc3339()),
                "success": t.success,
                "events_count": t.events.len(),
                "iterations": t.iterations,
            })
        })
        .collect();
    let total = store.len();
    axum::Json(serde_json::json!({
        "entries": entries,
        "total": total,
    }))
}

/// REST API: Get pending approval requests.
async fn api_approvals_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let approvals: Vec<serde_json::Value> = gw
        .pending_approvals()
        .iter()
        .map(|a| {
            serde_json::json!({
                "id": a.id.to_string(),
                "tool_name": a.tool_name,
                "description": a.description,
                "risk_level": a.risk_level,
            })
        })
        .collect();
    axum::Json(serde_json::json!({ "approvals": approvals }))
}

/// REST API: Submit an approval decision.
async fn api_approval_decision_handler(
    Path(id): Path<String>,
    State(gw): State<SharedGateway>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let approval_id = match Uuid::parse_str(&id) {
        Ok(uuid) => uuid,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                axum::Json(serde_json::json!({"error": "Invalid UUID"})),
            );
        }
    };

    let approved = body
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let mut gw = gw.lock().await;
    let found = gw.resolve_approval(&approval_id, approved);

    if found {
        (
            StatusCode::OK,
            axum::Json(serde_json::json!({"status": "resolved", "approved": approved})),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            axum::Json(serde_json::json!({"error": "Approval not found"})),
        )
    }
}

/// Handle an individual WebSocket connection.
async fn handle_socket(mut socket: WebSocket, gw: SharedGateway) {
    // Try to register the connection
    let conn_id = {
        let mut gw = gw.lock().await;
        match gw.connections_mut().add_connection() {
            Some(id) => id,
            None => {
                // At capacity — send error and close
                let err = ServerMessage::Event {
                    event: GatewayEvent::Error {
                        code: "CAPACITY_FULL".to_string(),
                        message: "Server at maximum connections".to_string(),
                    },
                };
                if let Ok(json) = serde_json::to_string(&err) {
                    let _ = socket.send(WsMessage::Text(json.into())).await;
                }
                let _ = socket.close().await;
                return;
            }
        }
    };

    // Message loop
    while let Some(Ok(ws_msg)) = socket.recv().await {
        let text = match ws_msg {
            WsMessage::Text(t) => t.to_string(),
            WsMessage::Close(_) => break,
            _ => continue,
        };

        let client_msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(e) => {
                let err = ServerMessage::Event {
                    event: GatewayEvent::Error {
                        code: "PARSE_ERROR".to_string(),
                        message: format!("Invalid message: {e}"),
                    },
                };
                if let Ok(json) = serde_json::to_string(&err) {
                    let _ = socket.send(WsMessage::Text(json.into())).await;
                }
                continue;
            }
        };

        let response = {
            let mut gw = gw.lock().await;
            gw.connections_mut().touch(&conn_id);
            gw.handle_client_message(client_msg, conn_id)
        };

        if let Ok(json) = serde_json::to_string(&response)
            && socket.send(WsMessage::Text(json.into())).await.is_err()
        {
            break;
        }
    }

    // Cleanup
    {
        let mut gw = gw.lock().await;
        gw.connections_mut().remove_connection(&conn_id);
        gw.broadcast(GatewayEvent::Disconnected {
            connection_id: conn_id,
        });
    }
}

// ── Voice & Meeting Toggle Endpoints ────────────────────────────────

/// REST API: Start voice command session.
async fn api_voice_start_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let ts = match gw.toggle_state() {
        Some(ts) => ts.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({"error": "Toggle state not configured"})),
            );
        }
    };
    drop(gw); // Release lock before async operation

    if ts.voice_active().await {
        return (
            StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"error": "Voice session already active"})),
        );
    }

    // Voice start requires config and workspace — return instruction to use CLI
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "status": "voice_start_requested",
            "message": "Voice session start requires agent config. Use /voicecmd on in the REPL or Ctrl+V in TUI."
        })),
    )
}

/// REST API: Stop voice command session.
async fn api_voice_stop_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let ts = match gw.toggle_state() {
        Some(ts) => ts.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({"error": "Toggle state not configured"})),
            );
        }
    };
    drop(gw);

    match ts.voice_stop().await {
        Ok(()) => (
            StatusCode::OK,
            axum::Json(serde_json::json!({"status": "stopped"})),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e.to_string()})),
        ),
    }
}

/// REST API: Get voice session status.
async fn api_voice_status_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw = gw.lock().await;
    let ts = match gw.toggle_state() {
        Some(ts) => ts.clone(),
        None => {
            return axum::Json(serde_json::json!({"active": false, "available": false}));
        }
    };
    drop(gw);

    axum::Json(serde_json::json!({
        "active": ts.voice_active().await,
        "available": true,
    }))
}

/// REST API: Start meeting recording.
async fn api_meeting_start_handler(
    State(gw): State<SharedGateway>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> impl IntoResponse {
    let gw_guard = gw.lock().await;
    let ts = match gw_guard.toggle_state() {
        Some(ts) => ts.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({"error": "Toggle state not configured"})),
            );
        }
    };
    drop(gw_guard);

    if ts.meeting_active().await {
        return (
            StatusCode::CONFLICT,
            axum::Json(serde_json::json!({"error": "Meeting recording already active"})),
        );
    }

    let title = body.get("title").and_then(|v| v.as_str()).map(String::from);
    let config = crate::config::MeetingConfig::default();

    match ts.meeting_start(config, title).await {
        Ok(()) => (
            StatusCode::OK,
            axum::Json(serde_json::json!({"status": "recording"})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(serde_json::json!({"error": e})),
        ),
    }
}

/// REST API: Stop meeting recording.
async fn api_meeting_stop_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw_guard = gw.lock().await;
    let ts = match gw_guard.toggle_state() {
        Some(ts) => ts.clone(),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                axum::Json(serde_json::json!({"error": "Toggle state not configured"})),
            );
        }
    };
    drop(gw_guard);

    match ts.meeting_stop().await {
        Ok(result) => (
            StatusCode::OK,
            axum::Json(serde_json::json!({
                "status": "stopped",
                "duration_secs": result.duration_secs,
                "transcript_length": result.transcript.len(),
                "notes_saved": result.notes_saved,
            })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            axum::Json(serde_json::json!({"error": e})),
        ),
    }
}

/// REST API: Get meeting recording status.
async fn api_meeting_status_handler(State(gw): State<SharedGateway>) -> impl IntoResponse {
    let gw_guard = gw.lock().await;
    let ts = match gw_guard.toggle_state() {
        Some(ts) => ts.clone(),
        None => {
            return axum::Json(serde_json::json!({
                "active": false,
                "available": false,
            }));
        }
    };
    drop(gw_guard);

    match ts.meeting_status().await {
        Some(status) => axum::Json(serde_json::json!({
            "active": true,
            "available": true,
            "title": status.title,
            "started_at": status.started_at,
            "elapsed_secs": status.elapsed_secs,
        })),
        None => axum::Json(serde_json::json!({
            "active": false,
            "available": true,
        })),
    }
}

/// Start the gateway server on the configured address.
///
/// This is an async function that runs until cancelled.
pub async fn run(gw: SharedGateway) -> Result<(), std::io::Error> {
    let (host, port) = {
        let gw = gw.lock().await;
        (gw.config().host.clone(), gw.config().port)
    };
    let app = router(gw);
    let addr = format!("{host}:{port}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use tower::ServiceExt;

    #[test]
    fn test_server_construction() {
        let config = GatewayConfig::default();
        let server = GatewayServer::new(config);
        assert_eq!(server.active_connections(), 0);
        assert_eq!(server.active_sessions(), 0);
    }

    #[test]
    fn test_server_with_auth_tokens() {
        let config = GatewayConfig {
            auth_tokens: vec!["tok1".into(), "tok2".into()],
            ..GatewayConfig::default()
        };
        let server = GatewayServer::new(config);
        assert!(server.auth().validate("tok1"));
        assert!(!server.auth().validate("wrong"));
    }

    #[test]
    fn test_server_broadcast_no_subscribers() {
        let server = GatewayServer::new(GatewayConfig::default());
        let sent = server.broadcast(GatewayEvent::Connected {
            connection_id: Uuid::new_v4(),
        });
        assert_eq!(sent, 0);
    }

    #[test]
    fn test_server_broadcast_with_subscriber() {
        let server = GatewayServer::new(GatewayConfig::default());
        let mut rx = server.subscribe();

        let sent = server.broadcast(GatewayEvent::AssistantMessage {
            content: "hello".into(),
        });
        assert_eq!(sent, 1);

        let event = rx.try_recv().unwrap();
        match event {
            GatewayEvent::AssistantMessage { content } => {
                assert_eq!(content, "hello");
            }
            _ => panic!("Wrong event type"),
        }
    }

    #[test]
    fn test_server_uptime() {
        let server = GatewayServer::new(GatewayConfig::default());
        assert!(server.uptime_secs() < 2);
    }

    #[test]
    fn test_server_connection_lifecycle() {
        let config = GatewayConfig {
            max_connections: 5,
            ..GatewayConfig::default()
        };
        let mut server = GatewayServer::new(config);

        let conn_id = server.connections_mut().add_connection().unwrap();
        assert_eq!(server.active_connections(), 1);

        let session_id = server.sessions_mut().create_session(conn_id);
        assert_eq!(server.active_sessions(), 1);

        server.sessions_mut().end_session(&session_id);
        assert_eq!(server.active_sessions(), 0);

        server.connections_mut().remove_connection(&conn_id);
        assert_eq!(server.active_connections(), 0);
    }

    // --- A6: WebSocket handler tests ---

    fn make_shared_gateway(config: GatewayConfig) -> SharedGateway {
        Arc::new(Mutex::new(GatewayServer::new(config)))
    }

    #[test]
    fn test_router_builds() {
        let gw = make_shared_gateway(GatewayConfig::default());
        let _app = router(gw);
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let gw = make_shared_gateway(GatewayConfig::default());
        let app = router(gw);

        let req = axum::http::Request::builder()
            .uri("/health")
            .body(Body::empty())
            .unwrap();

        let resp = ServiceExt::<axum::http::Request<Body>>::oneshot(app, req)
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        let body = axum::body::to_bytes(resp.into_body(), 10_000)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
        assert_eq!(json["connections"], 0);
        assert_eq!(json["sessions"], 0);
    }

    #[test]
    fn test_handle_authenticate_valid() {
        let config = GatewayConfig {
            auth_tokens: vec!["secret".into()],
            ..GatewayConfig::default()
        };
        let mut server = GatewayServer::new(config);
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(
            ClientMessage::Authenticate {
                token: "secret".into(),
            },
            conn_id,
        );
        match resp {
            ServerMessage::Authenticated { connection_id } => {
                assert_eq!(connection_id, conn_id);
            }
            _ => panic!("Expected Authenticated, got {resp:?}"),
        }
        assert!(server.connections().is_authenticated(&conn_id));
    }

    #[test]
    fn test_handle_authenticate_invalid() {
        let config = GatewayConfig {
            auth_tokens: vec!["secret".into()],
            ..GatewayConfig::default()
        };
        let mut server = GatewayServer::new(config);
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(
            ClientMessage::Authenticate {
                token: "wrong".into(),
            },
            conn_id,
        );
        match resp {
            ServerMessage::AuthFailed { reason } => {
                assert!(reason.contains("Invalid"));
            }
            _ => panic!("Expected AuthFailed, got {resp:?}"),
        }
        assert!(!server.connections().is_authenticated(&conn_id));
    }

    #[test]
    fn test_handle_get_status() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(ClientMessage::GetStatus, conn_id);
        match resp {
            ServerMessage::StatusResponse {
                connected_clients,
                active_tasks,
                ..
            } => {
                assert_eq!(connected_clients, 1);
                assert_eq!(active_tasks, 0);
            }
            _ => panic!("Expected StatusResponse"),
        }
    }

    #[test]
    fn test_handle_ping_pong() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        let conn_id = server.connections_mut().add_connection().unwrap();
        let now = Utc::now();

        let resp = server.handle_client_message(ClientMessage::Ping { timestamp: now }, conn_id);
        match resp {
            ServerMessage::Pong { timestamp } => {
                assert_eq!(timestamp, now);
            }
            _ => panic!("Expected Pong"),
        }
    }

    #[test]
    fn test_handle_submit_task_unauthenticated() {
        let config = GatewayConfig {
            auth_tokens: vec!["secret".into()],
            ..GatewayConfig::default()
        };
        let mut server = GatewayServer::new(config);
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(
            ClientMessage::SubmitTask {
                description: "test task".into(),
            },
            conn_id,
        );
        match resp {
            ServerMessage::AuthFailed { reason } => {
                assert!(reason.contains("Not authenticated"));
            }
            _ => panic!("Expected AuthFailed for unauthenticated submit"),
        }
    }

    #[test]
    fn test_handle_submit_task_authenticated() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        let conn_id = server.connections_mut().add_connection().unwrap();
        // Open mode — auto-authenticated by validate("")
        server.connections_mut().authenticate(&conn_id);

        let resp = server.handle_client_message(
            ClientMessage::SubmitTask {
                description: "build feature X".into(),
            },
            conn_id,
        );
        match resp {
            ServerMessage::Event {
                event: GatewayEvent::TaskSubmitted { description, .. },
            } => {
                assert_eq!(description, "build feature X");
            }
            _ => panic!("Expected TaskSubmitted event"),
        }
        // Session should have been created
        assert_eq!(server.active_sessions(), 1);
    }

    #[test]
    fn test_handle_cancel_task() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        let conn_id = server.connections_mut().add_connection().unwrap();
        server.connections_mut().authenticate(&conn_id);
        let task_id = Uuid::new_v4();

        let resp = server.handle_client_message(ClientMessage::CancelTask { task_id }, conn_id);
        match resp {
            ServerMessage::Event {
                event:
                    GatewayEvent::TaskCompleted {
                        task_id: tid,
                        success,
                        summary,
                    },
            } => {
                assert_eq!(tid, task_id);
                assert!(!success);
                assert!(summary.contains("Cancelled"));
            }
            _ => panic!("Expected TaskCompleted with cancel"),
        }
    }

    // --- StatusProvider wiring tests ---

    struct MockStatusProvider {
        channels: Vec<(String, String)>,
        nodes: Vec<(String, String)>,
    }

    impl StatusProvider for MockStatusProvider {
        fn channel_statuses(&self) -> Vec<(String, String)> {
            self.channels.clone()
        }
        fn node_statuses(&self) -> Vec<(String, String)> {
            self.nodes.clone()
        }
    }

    #[test]
    fn test_list_channels_without_provider() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(ClientMessage::ListChannels, conn_id);
        match resp {
            ServerMessage::ChannelStatus { channels } => {
                assert!(channels.is_empty());
            }
            _ => panic!("Expected ChannelStatus"),
        }
    }

    #[test]
    fn test_list_nodes_without_provider() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(ClientMessage::ListNodes, conn_id);
        match resp {
            ServerMessage::NodeStatus { nodes } => {
                assert!(nodes.is_empty());
            }
            _ => panic!("Expected NodeStatus"),
        }
    }

    #[test]
    fn test_list_channels_with_provider() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        server.set_status_provider(Box::new(MockStatusProvider {
            channels: vec![
                ("slack".into(), "Connected".into()),
                ("telegram".into(), "Disconnected".into()),
            ],
            nodes: vec![],
        }));
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(ClientMessage::ListChannels, conn_id);
        match resp {
            ServerMessage::ChannelStatus { channels } => {
                assert_eq!(channels.len(), 2);
                assert_eq!(channels[0].0, "slack");
                assert_eq!(channels[0].1, "Connected");
                assert_eq!(channels[1].0, "telegram");
                assert_eq!(channels[1].1, "Disconnected");
            }
            _ => panic!("Expected ChannelStatus"),
        }
    }

    #[test]
    fn test_list_nodes_with_provider() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        server.set_status_provider(Box::new(MockStatusProvider {
            channels: vec![],
            nodes: vec![
                ("macos-local".into(), "Healthy".into()),
                ("linux-remote".into(), "Degraded".into()),
            ],
        }));
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(ClientMessage::ListNodes, conn_id);
        match resp {
            ServerMessage::NodeStatus { nodes } => {
                assert_eq!(nodes.len(), 2);
                assert_eq!(nodes[0].0, "macos-local");
                assert_eq!(nodes[0].1, "Healthy");
                assert_eq!(nodes[1].0, "linux-remote");
                assert_eq!(nodes[1].1, "Degraded");
            }
            _ => panic!("Expected NodeStatus"),
        }
    }

    #[test]
    fn test_status_provider_can_be_replaced() {
        let mut server = GatewayServer::new(GatewayConfig::default());
        server.set_status_provider(Box::new(MockStatusProvider {
            channels: vec![("a".into(), "x".into())],
            nodes: vec![],
        }));
        // Replace the provider
        server.set_status_provider(Box::new(MockStatusProvider {
            channels: vec![("b".into(), "y".into()), ("c".into(), "z".into())],
            nodes: vec![],
        }));
        let conn_id = server.connections_mut().add_connection().unwrap();

        let resp = server.handle_client_message(ClientMessage::ListChannels, conn_id);
        match resp {
            ServerMessage::ChannelStatus { channels } => {
                assert_eq!(channels.len(), 2);
                assert_eq!(channels[0].0, "b");
            }
            _ => panic!("Expected ChannelStatus"),
        }
    }
}
