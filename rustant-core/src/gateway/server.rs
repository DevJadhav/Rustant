//! WebSocket gateway server built on axum.

use super::auth::GatewayAuth;
use super::connection::ConnectionManager;
use super::events::{ClientMessage, GatewayEvent, ServerMessage};
use super::session::SessionManager;
use super::GatewayConfig;
use axum::{
    extract::{
        ws::{Message as WsMessage, WebSocket, WebSocketUpgrade},
        State,
    },
    response::IntoResponse,
    routing::get,
    Router,
};
use chrono::Utc;
use futures::SinkExt;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
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
        let (event_tx, _) = broadcast::channel(256);

        Self {
            config,
            auth,
            connections,
            sessions,
            event_tx,
            started_at: Utc::now(),
            status_provider: None,
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

    /// Number of active connections.
    pub fn active_connections(&self) -> usize {
        self.connections.active_count()
    }

    /// Number of active sessions.
    pub fn active_sessions(&self) -> usize {
        self.sessions.active_count()
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
        }
    }
}

/// Build an axum Router with `/ws` and `/health` routes.
pub fn router(shared: SharedGateway) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/health", get(health_handler))
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
                        message: format!("Invalid message: {}", e),
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

        if let Ok(json) = serde_json::to_string(&response) {
            if socket.send(WsMessage::Text(json.into())).await.is_err() {
                break;
            }
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

/// Start the gateway server on the configured address.
///
/// This is an async function that runs until cancelled.
pub async fn run(gw: SharedGateway) -> Result<(), std::io::Error> {
    let (host, port) = {
        let gw = gw.lock().await;
        (gw.config().host.clone(), gw.config().port)
    };
    let app = router(gw);
    let addr = format!("{}:{}", host, port);
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
            _ => panic!("Expected Authenticated, got {:?}", resp),
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
            _ => panic!("Expected AuthFailed, got {:?}", resp),
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
