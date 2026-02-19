//! Database tool — migrations, seeds, queries, and schema inspection.
//!
//! Wraps framework-specific DB CLIs (diesel, prisma, alembic, rails).
//! Also supports direct SQLite queries via rusqlite.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::project_detect::{ProjectType, detect_project};
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

const TOOL_NAME: &str = "database";

pub struct DatabaseTool {
    workspace: PathBuf,
}

impl DatabaseTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl crate::registry::Tool for DatabaseTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Database operations: run migrations, rollback, seed data, execute queries, inspect schema. Supports SQLite directly, and wraps diesel/prisma/alembic CLIs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["migrate", "rollback", "seed", "query", "schema", "status"],
                    "description": "The database action to perform"
                },
                "query": {
                    "type": "string",
                    "description": "SQL query to execute (for 'query' action)"
                },
                "database": {
                    "type": "string",
                    "description": "Database path or connection string (defaults to auto-detect)"
                },
                "migration_name": {
                    "type": "string",
                    "description": "Name for a new migration (for 'migrate' action with generate)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: TOOL_NAME.into(),
                reason: "Missing 'action' parameter".to_string(),
            }
        })?;

        match action {
            "query" => {
                let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: TOOL_NAME.into(),
                        reason: "Missing 'query' parameter for query action".to_string(),
                    }
                })?;
                let db_path = args
                    .get("database")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.execute_query(query, db_path.as_deref()).await
            }
            "schema" => {
                let db_path = args
                    .get("database")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.show_schema(db_path.as_deref()).await
            }
            "status" => self.migration_status().await,
            "migrate" => self.run_migration(&args).await,
            "rollback" => self.run_rollback().await,
            "seed" => self.run_seed().await,
            _ => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.into(),
                reason: format!("Unknown database action: {action}"),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(120)
    }
}

impl DatabaseTool {
    fn find_sqlite_db(&self) -> Option<PathBuf> {
        let candidates = [
            "app.db",
            "db.sqlite3",
            "database.sqlite",
            "dev.db",
            "data.db",
            "sqlite.db",
        ];
        for name in &candidates {
            let path = self.workspace.join(name);
            if path.exists() {
                return Some(path);
            }
        }
        None
    }

    async fn execute_query(
        &self,
        query: &str,
        db_path: Option<&str>,
    ) -> Result<ToolOutput, ToolError> {
        let path = if let Some(p) = db_path {
            PathBuf::from(p)
        } else if let Some(p) = self.find_sqlite_db() {
            p
        } else {
            return Err(ToolError::ExecutionFailed {
                name: TOOL_NAME.into(),
                message: "No SQLite database found. Specify 'database' parameter.".to_string(),
            });
        };

        let path_clone = path.clone();
        let query_owned = query.to_string();

        let result = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path_clone).map_err(|e| {
                ToolError::ExecutionFailed {
                    name: TOOL_NAME.into(),
                    message: format!("Failed to open database: {e}"),
                }
            })?;

            if query_owned.trim().to_uppercase().starts_with("SELECT")
                || query_owned.trim().to_uppercase().starts_with("PRAGMA")
                || query_owned.trim().to_uppercase().starts_with("EXPLAIN")
            {
                let mut stmt =
                    conn.prepare(&query_owned)
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: TOOL_NAME.into(),
                            message: format!("SQL prepare error: {e}"),
                        })?;

                let col_count = stmt.column_count();
                let col_names: Vec<String> = (0..col_count)
                    .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                    .collect();

                let mut rows_output = Vec::new();
                rows_output.push(col_names.join(" | "));
                rows_output.push("-".repeat(rows_output[0].len()));

                let rows = stmt
                    .query_map([], |row| {
                        let vals: Vec<String> = (0..col_count)
                            .map(|i| {
                                row.get::<_, rusqlite::types::Value>(i)
                                    .map(|v| match v {
                                        rusqlite::types::Value::Null => "NULL".to_string(),
                                        rusqlite::types::Value::Integer(i) => i.to_string(),
                                        rusqlite::types::Value::Real(f) => f.to_string(),
                                        rusqlite::types::Value::Text(s) => s,
                                        rusqlite::types::Value::Blob(b) => {
                                            format!("<blob {} bytes>", b.len())
                                        }
                                    })
                                    .unwrap_or_else(|_| "?".to_string())
                            })
                            .collect();
                        Ok(vals.join(" | "))
                    })
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: TOOL_NAME.into(),
                        message: format!("SQL query error: {e}"),
                    })?;

                let mut row_count = 0;
                for row in rows {
                    if row_count >= 1000 {
                        rows_output.push("... (truncated at 1000 rows)".to_string());
                        break;
                    }
                    rows_output.push(row.map_err(|e| ToolError::ExecutionFailed {
                        name: TOOL_NAME.into(),
                        message: format!("Row error: {e}"),
                    })?);
                    row_count += 1;
                }

                Ok::<String, ToolError>(format!(
                    "Query: {}\n\n{}\n\n({} rows)",
                    query_owned,
                    rows_output.join("\n"),
                    row_count
                ))
            } else {
                let affected =
                    conn.execute(&query_owned, [])
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: TOOL_NAME.into(),
                            message: format!("SQL execute error: {e}"),
                        })?;
                Ok(format!("Query: {query_owned}\nRows affected: {affected}"))
            }
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.into(),
            message: format!("Task join error: {e}"),
        })??;

        Ok(ToolOutput::text(result))
    }

    async fn show_schema(&self, db_path: Option<&str>) -> Result<ToolOutput, ToolError> {
        let path = if let Some(p) = db_path {
            PathBuf::from(p)
        } else if let Some(p) = self.find_sqlite_db() {
            p
        } else {
            return self.framework_schema().await;
        };

        let path_clone = path.clone();
        let result = tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open(&path_clone).map_err(|e| {
                ToolError::ExecutionFailed {
                    name: TOOL_NAME.into(),
                    message: format!("Failed to open database: {e}"),
                }
            })?;

            let mut stmt = conn
                .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
                .map_err(|e| ToolError::ExecutionFailed {
                    name: TOOL_NAME.into(),
                    message: format!("Schema query error: {e}"),
                })?;

            let tables: Vec<(String, String)> = stmt
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1).unwrap_or_default(),
                    ))
                })
                .map_err(|e| ToolError::ExecutionFailed {
                    name: TOOL_NAME.into(),
                    message: format!("Schema query error: {e}"),
                })?
                .filter_map(|r| r.ok())
                .collect();

            let mut output = format!("Database: {}\n\n", path_clone.display());
            if tables.is_empty() {
                output.push_str("No tables found.");
            } else {
                for (name, sql) in &tables {
                    output.push_str(&format!("Table: {name}\n{sql};\n\n"));
                }
                output.push_str(&format!("{} tables total", tables.len()));
            }
            Ok::<String, ToolError>(output)
        })
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.into(),
            message: format!("Task join error: {e}"),
        })??;

        Ok(ToolOutput::text(result))
    }

    async fn framework_schema(&self) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => "diesel print-schema",
            ProjectType::Node => "npx prisma db pull --print",
            ProjectType::Python => "alembic heads",
            ProjectType::Ruby => "bundle exec rails db:schema:dump",
            _ => {
                return Ok(ToolOutput::text(
                    "No database found and could not detect framework for schema inspection."
                        .to_string(),
                ));
            }
        };

        run_command(&self.workspace, cmd).await
    }

    async fn migration_status(&self) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => "diesel migration list",
            ProjectType::Node => {
                if self.workspace.join("prisma").exists() {
                    "npx prisma migrate status"
                } else {
                    "npx drizzle-kit status"
                }
            }
            ProjectType::Python => "alembic current",
            ProjectType::Ruby => "bundle exec rails db:migrate:status",
            _ => {
                return Ok(ToolOutput::text(
                    "Cannot detect project type for migration status.".to_string(),
                ));
            }
        };

        run_command(&self.workspace, cmd).await
    }

    async fn run_migration(&self, args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => "diesel migration run".to_string(),
            ProjectType::Node => {
                if self.workspace.join("prisma").exists() {
                    let name = args
                        .get("migration_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("migration");
                    format!("npx prisma migrate dev --name {name}")
                } else {
                    "npx drizzle-kit push".to_string()
                }
            }
            ProjectType::Python => "alembic upgrade head".to_string(),
            ProjectType::Ruby => "bundle exec rails db:migrate".to_string(),
            _ => {
                return Err(ToolError::ExecutionFailed {
                    name: TOOL_NAME.into(),
                    message: "Cannot detect project type for migrations.".to_string(),
                });
            }
        };

        run_command(&self.workspace, &cmd).await
    }

    async fn run_rollback(&self) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Rust => "diesel migration revert",
            ProjectType::Node => "npx prisma migrate reset --skip-seed",
            ProjectType::Python => "alembic downgrade -1",
            ProjectType::Ruby => "bundle exec rails db:rollback",
            _ => {
                return Err(ToolError::ExecutionFailed {
                    name: TOOL_NAME.into(),
                    message: "Cannot detect project type for rollback.".to_string(),
                });
            }
        };

        run_command(&self.workspace, cmd).await
    }

    async fn run_seed(&self) -> Result<ToolOutput, ToolError> {
        let project = detect_project(&self.workspace);
        let cmd = match project.project_type {
            ProjectType::Node => {
                if self.workspace.join("prisma/seed.ts").exists()
                    || self.workspace.join("prisma/seed.js").exists()
                {
                    "npx prisma db seed"
                } else {
                    "npm run seed"
                }
            }
            ProjectType::Python => "python -m app.seed",
            ProjectType::Ruby => "bundle exec rails db:seed",
            _ => {
                return Err(ToolError::ExecutionFailed {
                    name: TOOL_NAME.into(),
                    message: "Cannot detect project type for seeding.".to_string(),
                });
            }
        };

        run_command(&self.workspace, cmd).await
    }
}

async fn run_command(workspace: &std::path::Path, cmd: &str) -> Result<ToolOutput, ToolError> {
    let parts: Vec<&str> = cmd.split_whitespace().collect();
    if parts.is_empty() {
        return Err(ToolError::ExecutionFailed {
            name: TOOL_NAME.into(),
            message: "Empty command".to_string(),
        });
    }

    let output = tokio::process::Command::new(parts[0])
        .args(&parts[1..])
        .current_dir(workspace)
        .output()
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.into(),
            message: format!("Failed to run '{cmd}': {e}"),
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = format!("$ {cmd}\n\n");
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !stdout.is_empty() {
            result.push('\n');
        }
        result.push_str(&stderr);
    }

    if !output.status.success() {
        result.push_str(&format!(
            "\n\nExit code: {}",
            output.status.code().unwrap_or(-1)
        ));
    }

    Ok(ToolOutput::text(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_status_no_project() {
        let dir = TempDir::new().unwrap();
        let tool = DatabaseTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "status"})).await.unwrap();
        assert!(result.content.contains("Cannot detect"));
    }

    #[tokio::test]
    async fn test_schema_sqlite() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("app.db");
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)",
                [],
            )
            .unwrap();
        }
        let tool = DatabaseTool::new(dir.path().to_path_buf());
        let result = tool.execute(json!({"action": "schema"})).await.unwrap();
        assert!(result.content.contains("users"));
    }

    #[tokio::test]
    async fn test_query_sqlite() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("app.db");
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute("CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)", [])
                .unwrap();
            conn.execute("INSERT INTO items (name) VALUES ('test')", [])
                .unwrap();
        }
        let tool = DatabaseTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "query",
                "query": "SELECT * FROM items"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("test"));
        assert!(result.content.contains("1 rows"));
    }

    #[tokio::test]
    async fn test_query_no_db() {
        let dir = TempDir::new().unwrap();
        let tool = DatabaseTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "query",
                "query": "SELECT 1"
            }))
            .await;
        assert!(result.is_err());
    }
}
