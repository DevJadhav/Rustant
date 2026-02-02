//! Integration tests for the gateway REST API endpoints.
//!
//! Tests the HTTP REST endpoints added for the Tauri dashboard (Week 17).

use axum::body::Body;
use rustant_core::gateway::{
    gateway_router, GatewayConfig, GatewayServer, PendingApproval, SharedGateway,
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tower::ServiceExt;
use uuid::Uuid;

fn make_gateway() -> SharedGateway {
    Arc::new(Mutex::new(GatewayServer::new(GatewayConfig::default())))
}

fn make_request(uri: &str) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

fn make_post_request(uri: &str, body: serde_json::Value) -> axum::http::Request<Body> {
    axum::http::Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_string(&body).unwrap()))
        .unwrap()
}

async fn get_json(gw: SharedGateway, uri: &str) -> (axum::http::StatusCode, serde_json::Value) {
    let app = gateway_router(gw);
    let resp = ServiceExt::<axum::http::Request<Body>>::oneshot(app, make_request(uri))
        .await
        .unwrap();
    let status = resp.status();
    let body = axum::body::to_bytes(resp.into_body(), 100_000)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    (status, json)
}

// --- /api/status ---

#[tokio::test]
async fn test_api_status_returns_json() {
    let gw = make_gateway();
    let (status, json) = get_json(gw, "/api/status").await;
    assert_eq!(status, 200);
    assert!(json.get("version").is_some());
    assert!(json.get("uptime_secs").is_some());
    assert_eq!(json["active_connections"], 0);
    assert_eq!(json["active_sessions"], 0);
    assert_eq!(json["total_tool_calls"], 0);
    assert_eq!(json["total_llm_requests"], 0);
}

#[tokio::test]
async fn test_api_status_reflects_metrics() {
    let gw = make_gateway();
    {
        let mut g = gw.lock().await;
        g.record_tool_call();
        g.record_tool_call();
        g.record_llm_request();
    }
    let (_, json) = get_json(gw, "/api/status").await;
    assert_eq!(json["total_tool_calls"], 2);
    assert_eq!(json["total_llm_requests"], 1);
}

#[tokio::test]
async fn test_api_status_shows_channels_and_nodes() {
    let gw = make_gateway();
    {
        let mut g = gw.lock().await;
        g.set_status_provider(Box::new(TestStatusProvider));
    }
    let (_, json) = get_json(gw, "/api/status").await;
    let channels = json["channels"].as_array().unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["name"], "slack");
    let nodes = json["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0]["name"], "macbook");
}

// --- /api/sessions ---

#[tokio::test]
async fn test_api_sessions_empty() {
    let gw = make_gateway();
    let (status, json) = get_json(gw, "/api/sessions").await;
    assert_eq!(status, 200);
    assert_eq!(json["total"], 0);
    assert!(json["sessions"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_api_sessions_with_active() {
    let gw = make_gateway();
    {
        let mut g = gw.lock().await;
        g.sessions_mut().create_session(Uuid::new_v4());
        g.sessions_mut().create_session(Uuid::new_v4());
    }
    let (_, json) = get_json(gw, "/api/sessions").await;
    assert_eq!(json["total"], 2);
    let sessions = json["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(sessions[0].get("id").is_some());
    assert!(sessions[0].get("state").is_some());
    assert!(sessions[0].get("created_at").is_some());
}

// --- /api/config ---

#[tokio::test]
async fn test_api_config_default() {
    let gw = make_gateway();
    let (status, json) = get_json(gw, "/api/config").await;
    assert_eq!(status, 200);
    // Default is "{}" which parses to empty object
    assert!(json.is_object());
}

#[tokio::test]
async fn test_api_config_custom_json() {
    let gw = make_gateway();
    {
        let mut g = gw.lock().await;
        g.set_config_json(
            serde_json::json!({"llm": {"provider": "anthropic"}, "safety": {"mode": "cautious"}})
                .to_string(),
        );
    }
    let (_, json) = get_json(gw, "/api/config").await;
    assert_eq!(json["llm"]["provider"], "anthropic");
    assert_eq!(json["safety"]["mode"], "cautious");
}

// --- /api/metrics ---

#[tokio::test]
async fn test_api_metrics() {
    let gw = make_gateway();
    {
        let mut g = gw.lock().await;
        for _ in 0..5 {
            g.record_tool_call();
        }
        for _ in 0..3 {
            g.record_llm_request();
        }
    }
    let (status, json) = get_json(gw, "/api/metrics").await;
    assert_eq!(status, 200);
    assert_eq!(json["total_tool_calls"], 5);
    assert_eq!(json["total_llm_requests"], 3);
    assert!(json["uptime_secs"].as_u64().is_some());
}

// --- /api/audit ---

#[tokio::test]
async fn test_api_audit_returns_empty() {
    let gw = make_gateway();
    let (status, json) = get_json(gw, "/api/audit").await;
    assert_eq!(status, 200);
    assert_eq!(json["total"], 0);
    assert!(json["entries"].as_array().unwrap().is_empty());
}

// --- /api/approvals ---

#[tokio::test]
async fn test_api_approvals_empty() {
    let gw = make_gateway();
    let (status, json) = get_json(gw, "/api/approvals").await;
    assert_eq!(status, 200);
    assert!(json["approvals"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_api_approvals_with_pending() {
    let gw = make_gateway();
    let approval_id = Uuid::new_v4();
    {
        let mut g = gw.lock().await;
        g.add_approval(PendingApproval {
            id: approval_id,
            tool_name: "shell_exec".into(),
            description: "rm -rf /tmp/test".into(),
            risk_level: "high".into(),
        });
    }
    let (_, json) = get_json(gw, "/api/approvals").await;
    let approvals = json["approvals"].as_array().unwrap();
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals[0]["tool_name"], "shell_exec");
    assert_eq!(approvals[0]["risk_level"], "high");
}

// --- /api/approval/{id} POST ---

#[tokio::test]
async fn test_api_approval_decision_approve() {
    let gw = make_gateway();
    let approval_id = Uuid::new_v4();
    {
        let mut g = gw.lock().await;
        g.add_approval(PendingApproval {
            id: approval_id,
            tool_name: "file_write".into(),
            description: "Write to config".into(),
            risk_level: "medium".into(),
        });
    }

    let app = gateway_router(gw.clone());
    let req = make_post_request(
        &format!("/api/approval/{}", approval_id),
        serde_json::json!({"approved": true}),
    );
    let resp = ServiceExt::<axum::http::Request<Body>>::oneshot(app, req)
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    // Verify approval was removed
    let g = gw.lock().await;
    assert!(g.pending_approvals().is_empty());
}

#[tokio::test]
async fn test_api_approval_decision_not_found() {
    let gw = make_gateway();
    let fake_id = Uuid::new_v4();

    let app = gateway_router(gw);
    let req = make_post_request(
        &format!("/api/approval/{}", fake_id),
        serde_json::json!({"approved": false}),
    );
    let resp = ServiceExt::<axum::http::Request<Body>>::oneshot(app, req)
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_api_approval_invalid_uuid() {
    let gw = make_gateway();
    let app = gateway_router(gw);
    let req = make_post_request(
        "/api/approval/not-a-uuid",
        serde_json::json!({"approved": true}),
    );
    let resp = ServiceExt::<axum::http::Request<Body>>::oneshot(app, req)
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);
}

// --- Health endpoint still works ---

#[tokio::test]
async fn test_health_endpoint_still_works() {
    let gw = make_gateway();
    let (status, json) = get_json(gw, "/health").await;
    assert_eq!(status, 200);
    assert_eq!(json["status"], "ok");
}

// --- Helper ---

struct TestStatusProvider;

impl rustant_core::gateway::StatusProvider for TestStatusProvider {
    fn channel_statuses(&self) -> Vec<(String, String)> {
        vec![("slack".into(), "connected".into())]
    }
    fn node_statuses(&self) -> Vec<(String, String)> {
        vec![("macbook".into(), "healthy".into())]
    }
}
