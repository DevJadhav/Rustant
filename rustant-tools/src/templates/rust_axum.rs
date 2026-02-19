//! Axum web service + SQLx + Tokio project template.

use super::{ProjectTemplate, TemplateFile};

pub fn template() -> ProjectTemplate {
    ProjectTemplate {
        name: "axum".into(),
        description: "Axum web service + SQLx + Tokio".into(),
        framework: "Axum".into(),
        files: vec![
            TemplateFile {
                path: "Cargo.toml".into(),
                content: r#"[package]
name = "{{project-name}}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
dotenvy = "0.15"

[dev-dependencies]
reqwest = { version = "0.12", features = ["json"] }
tokio-test = "0.4"
"#
                .into(),
            },
            TemplateFile {
                path: "src/main.rs".into(),
                content: r#"use axum::{Router, routing::get};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod handlers;
mod state;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let app_state = state::AppState::new().await;

    let app = Router::new()
        .route("/", get(handlers::root))
        .route("/health", get(handlers::health))
        .layer(TraceLayer::new_for_http())
        .with_state(app_state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind");

    tracing::info!("Listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}
"#
                .into(),
            },
            TemplateFile {
                path: "src/handlers.rs".into(),
                content: r#"use axum::Json;
use serde_json::{json, Value};

pub async fn root() -> Json<Value> {
    Json(json!({ "message": "Hello from {{ProjectName}}" }))
}

pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
"#
                .into(),
            },
            TemplateFile {
                path: "src/state.rs".into(),
                content: r#"use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
}

impl AppState {
    pub async fn new() -> Self {
        let database_url =
            std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:app.db".into());

        let db = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to database");

        Self { db }
    }
}
"#
                .into(),
            },
            TemplateFile {
                path: "migrations/001_init.sql".into(),
                content: r#"CREATE TABLE IF NOT EXISTS items (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT DEFAULT ''
);
"#
                .into(),
            },
            TemplateFile {
                path: ".env.example".into(),
                content: "DATABASE_URL=sqlite:app.db\nRUST_LOG=info\n".into(),
            },
            TemplateFile {
                path: ".gitignore".into(),
                content: "target\n.env\n*.db\n".into(),
            },
        ],
        post_install: vec!["cargo build".into()],
    }
}
