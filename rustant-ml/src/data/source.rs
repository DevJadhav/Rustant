//! Data source abstraction for loading datasets from various formats.

use crate::data::schema::{ColumnSchema, ColumnType, SchemaDefinition, infer_schema};
use crate::error::MlError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// The type of data source to load from.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DataSourceType {
    Csv {
        path: PathBuf,
        #[serde(default = "default_delimiter")]
        delimiter: char,
    },
    Json {
        path: PathBuf,
        #[serde(default)]
        json_path: Option<String>,
    },
    Jsonl {
        path: PathBuf,
    },
    Sqlite {
        db_path: PathBuf,
        query: String,
    },
    Api {
        url: String,
        #[serde(default = "default_method")]
        method: String,
        #[serde(default)]
        headers: HashMap<String, String>,
    },
    HuggingFace {
        dataset_name: String,
        #[serde(default = "default_split")]
        split: String,
    },
    /// Columnar format via Apache Parquet (requires `columnar` feature).
    // NOTE: Actual parquet reading depends on the `columnar` feature gate for arrow/parquet deps.
    Parquet {
        path: PathBuf,
    },
}

fn default_delimiter() -> char {
    ','
}
fn default_method() -> String {
    "GET".to_string()
}
fn default_split() -> String {
    "train".to_string()
}

/// A batch of data rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataBatch {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub total_rows: usize,
}

impl DataBatch {
    pub fn empty() -> Self {
        Self {
            columns: Vec::new(),
            rows: Vec::new(),
            total_rows: 0,
        }
    }

    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    pub fn column_count(&self) -> usize {
        self.columns.len()
    }
}

/// Information about a data source for lineage tracking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceInfo {
    pub source_type: String,
    pub location: String,
    pub accessed_at: chrono::DateTime<chrono::Utc>,
    pub row_count: Option<usize>,
}

/// Trait for loading data from a source.
#[async_trait]
pub trait DataSource: Send + Sync {
    /// Load data from this source, optionally limiting the number of rows.
    async fn load(&self, limit: Option<usize>) -> Result<DataBatch, MlError>;

    /// Return metadata about this source for lineage tracking.
    fn source_info(&self) -> DataSourceInfo;

    /// Infer or return the schema of this data source.
    fn schema(&self) -> Result<SchemaDefinition, MlError>;
}

// ---------------------------------------------------------------------------
// CsvSource
// ---------------------------------------------------------------------------

/// CSV file data source.
pub struct CsvSource {
    pub path: PathBuf,
    pub delimiter: char,
}

#[async_trait]
impl DataSource for CsvSource {
    async fn load(&self, limit: Option<usize>) -> Result<DataBatch, MlError> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let mut lines = content.lines();

        // Parse header
        let columns: Vec<String> = lines
            .next()
            .ok_or_else(|| MlError::dataset("Empty CSV file".to_string()))?
            .split(self.delimiter)
            .map(|s| s.trim().trim_matches('"').to_string())
            .collect();

        let mut rows = Vec::new();
        for line in lines {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(max) = limit {
                if rows.len() >= max {
                    break;
                }
            }
            let row: Vec<serde_json::Value> = line
                .split(self.delimiter)
                .map(|s| serde_json::Value::String(s.trim().trim_matches('"').to_string()))
                .collect();
            rows.push(row);
        }

        let total_rows = rows.len();
        Ok(DataBatch {
            columns,
            rows,
            total_rows,
        })
    }

    fn source_info(&self) -> DataSourceInfo {
        DataSourceInfo {
            source_type: "csv".to_string(),
            location: self.path.display().to_string(),
            accessed_at: chrono::Utc::now(),
            row_count: None,
        }
    }

    fn schema(&self) -> Result<SchemaDefinition, MlError> {
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| MlError::dataset(format!("Failed to read CSV for schema: {e}")))?;
        let mut lines = content.lines();
        let columns: Vec<String> = lines
            .next()
            .ok_or_else(|| MlError::dataset("Empty CSV file"))?
            .split(self.delimiter)
            .map(|s| s.trim().trim_matches('"').to_string())
            .collect();

        // Sample up to 100 rows for type inference
        let mut rows = Vec::new();
        for line in lines.take(100) {
            if line.trim().is_empty() {
                continue;
            }
            let row: Vec<serde_json::Value> = line
                .split(self.delimiter)
                .map(|s| {
                    let s = s.trim().trim_matches('"');
                    if let Ok(i) = s.parse::<i64>() {
                        serde_json::Value::Number(i.into())
                    } else if let Ok(f) = s.parse::<f64>() {
                        serde_json::Number::from_f64(f)
                            .map(serde_json::Value::Number)
                            .unwrap_or_else(|| serde_json::Value::String(s.to_string()))
                    } else if s == "true" || s == "false" {
                        serde_json::Value::Bool(s == "true")
                    } else {
                        serde_json::Value::String(s.to_string())
                    }
                })
                .collect();
            rows.push(row);
        }
        Ok(infer_schema(&columns, &rows))
    }
}

// ---------------------------------------------------------------------------
// JsonSource
// ---------------------------------------------------------------------------

/// JSON file data source.
pub struct JsonSource {
    pub path: PathBuf,
    pub json_path: Option<String>,
}

#[async_trait]
impl DataSource for JsonSource {
    async fn load(&self, limit: Option<usize>) -> Result<DataBatch, MlError> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        let items = match &value {
            serde_json::Value::Array(arr) => arr.clone(),
            serde_json::Value::Object(_) => vec![value],
            _ => return Err(MlError::dataset("JSON must be an array or object")),
        };

        let limited = if let Some(max) = limit {
            items.into_iter().take(max).collect()
        } else {
            items
        };

        // Extract columns from first item
        let columns = if let Some(first) = limited.first() {
            if let serde_json::Value::Object(map) = first {
                map.keys().cloned().collect()
            } else {
                vec!["value".to_string()]
            }
        } else {
            Vec::new()
        };

        let rows: Vec<Vec<serde_json::Value>> = limited
            .iter()
            .map(|item| {
                columns
                    .iter()
                    .map(|col| item.get(col).cloned().unwrap_or(serde_json::Value::Null))
                    .collect()
            })
            .collect();

        let total_rows = rows.len();
        Ok(DataBatch {
            columns,
            rows,
            total_rows,
        })
    }

    fn source_info(&self) -> DataSourceInfo {
        DataSourceInfo {
            source_type: "json".to_string(),
            location: self.path.display().to_string(),
            accessed_at: chrono::Utc::now(),
            row_count: None,
        }
    }

    fn schema(&self) -> Result<SchemaDefinition, MlError> {
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| MlError::dataset(format!("Failed to read JSON for schema: {e}")))?;
        let value: serde_json::Value = serde_json::from_str(&content)?;

        let items: Vec<serde_json::Value> = match &value {
            serde_json::Value::Array(arr) => arr.iter().take(100).cloned().collect(),
            serde_json::Value::Object(_) => vec![value],
            _ => return Err(MlError::dataset("JSON must be an array or object")),
        };

        let columns: Vec<String> = if let Some(first) = items.first() {
            if let serde_json::Value::Object(map) = first {
                map.keys().cloned().collect()
            } else {
                vec!["value".to_string()]
            }
        } else {
            return Ok(SchemaDefinition {
                columns: Vec::new(),
            });
        };

        let rows: Vec<Vec<serde_json::Value>> = items
            .iter()
            .map(|item| {
                columns
                    .iter()
                    .map(|col| item.get(col).cloned().unwrap_or(serde_json::Value::Null))
                    .collect()
            })
            .collect();

        Ok(infer_schema(&columns, &rows))
    }
}

// ---------------------------------------------------------------------------
// JsonlSource
// ---------------------------------------------------------------------------

/// JSON Lines (JSONL) file data source — one JSON object per line.
pub struct JsonlSource {
    pub path: PathBuf,
}

#[async_trait]
impl DataSource for JsonlSource {
    async fn load(&self, limit: Option<usize>) -> Result<DataBatch, MlError> {
        let content = tokio::fs::read_to_string(&self.path).await?;
        let mut items = Vec::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(max) = limit {
                if items.len() >= max {
                    break;
                }
            }
            let value: serde_json::Value = serde_json::from_str(line)?;
            items.push(value);
        }

        let columns: Vec<String> = if let Some(first) = items.first() {
            if let serde_json::Value::Object(map) = first {
                map.keys().cloned().collect()
            } else {
                vec!["value".to_string()]
            }
        } else {
            return Ok(DataBatch::empty());
        };

        let rows: Vec<Vec<serde_json::Value>> = items
            .iter()
            .map(|item| {
                columns
                    .iter()
                    .map(|col| item.get(col).cloned().unwrap_or(serde_json::Value::Null))
                    .collect()
            })
            .collect();

        let total_rows = rows.len();
        Ok(DataBatch {
            columns,
            rows,
            total_rows,
        })
    }

    fn source_info(&self) -> DataSourceInfo {
        DataSourceInfo {
            source_type: "jsonl".to_string(),
            location: self.path.display().to_string(),
            accessed_at: chrono::Utc::now(),
            row_count: None,
        }
    }

    fn schema(&self) -> Result<SchemaDefinition, MlError> {
        let content = std::fs::read_to_string(&self.path)
            .map_err(|e| MlError::dataset(format!("Failed to read JSONL for schema: {e}")))?;
        let mut items = Vec::new();
        for line in content.lines().take(100) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                items.push(value);
            }
        }

        let columns: Vec<String> = if let Some(first) = items.first() {
            if let serde_json::Value::Object(map) = first {
                map.keys().cloned().collect()
            } else {
                vec!["value".to_string()]
            }
        } else {
            return Ok(SchemaDefinition {
                columns: Vec::new(),
            });
        };

        let rows: Vec<Vec<serde_json::Value>> = items
            .iter()
            .map(|item| {
                columns
                    .iter()
                    .map(|col| item.get(col).cloned().unwrap_or(serde_json::Value::Null))
                    .collect()
            })
            .collect();

        Ok(infer_schema(&columns, &rows))
    }
}

// ---------------------------------------------------------------------------
// SqliteSource
// ---------------------------------------------------------------------------

/// SQLite database data source. Loads data via a SQL query.
pub struct SqliteSource {
    pub db_path: PathBuf,
    pub query: String,
}

#[async_trait]
impl DataSource for SqliteSource {
    async fn load(&self, limit: Option<usize>) -> Result<DataBatch, MlError> {
        let db_path = self.db_path.clone();
        let query = if let Some(max) = limit {
            format!("{} LIMIT {max}", self.query.trim_end_matches(';'))
        } else {
            self.query.clone()
        };

        // Run blocking SQLite operations on a blocking thread
        tokio::task::spawn_blocking(move || {
            let conn = rusqlite::Connection::open_with_flags(
                &db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let mut stmt = conn.prepare(&query)?;
            let column_count = stmt.column_count();
            let columns: Vec<String> = (0..column_count)
                .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
                .collect();

            let mut rows = Vec::new();
            let mut result_rows = stmt.query([])?;
            while let Some(row) = result_rows.next()? {
                let mut values = Vec::with_capacity(column_count);
                for i in 0..column_count {
                    let val = match row.get_ref(i) {
                        Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                        Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::json!(n),
                        Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::Number::from_f64(f)
                            .map(serde_json::Value::Number)
                            .unwrap_or(serde_json::Value::Null),
                        Ok(rusqlite::types::ValueRef::Text(t)) => {
                            serde_json::Value::String(String::from_utf8_lossy(t).into_owned())
                        }
                        Ok(rusqlite::types::ValueRef::Blob(_)) => {
                            serde_json::Value::String("<blob>".to_string())
                        }
                        Err(_) => serde_json::Value::Null,
                    };
                    values.push(val);
                }
                rows.push(values);
            }

            let total_rows = rows.len();
            Ok(DataBatch {
                columns,
                rows,
                total_rows,
            })
        })
        .await
        .map_err(|e| MlError::dataset(format!("SQLite task join error: {e}")))?
    }

    fn source_info(&self) -> DataSourceInfo {
        DataSourceInfo {
            source_type: "sqlite".to_string(),
            location: self.db_path.display().to_string(),
            accessed_at: chrono::Utc::now(),
            row_count: None,
        }
    }

    fn schema(&self) -> Result<SchemaDefinition, MlError> {
        let conn = rusqlite::Connection::open_with_flags(
            &self.db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        // Fetch one row to infer types from actual data (SQLite is dynamically typed)
        let sample_query = format!("{} LIMIT 1", self.query.trim_end_matches(';'));
        let mut stmt = conn.prepare(&sample_query)?;
        let column_count = stmt.column_count();
        let col_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
            .collect();

        let mut result_rows = stmt.query([])?;
        let columns: Vec<ColumnSchema> = if let Some(row) = result_rows.next()? {
            col_names
                .iter()
                .enumerate()
                .map(|(i, name)| {
                    let dtype = match row.get_ref(i) {
                        Ok(rusqlite::types::ValueRef::Integer(_)) => ColumnType::Integer,
                        Ok(rusqlite::types::ValueRef::Real(_)) => ColumnType::Float,
                        Ok(rusqlite::types::ValueRef::Text(_)) => ColumnType::String,
                        Ok(rusqlite::types::ValueRef::Blob(_)) => ColumnType::String,
                        Ok(rusqlite::types::ValueRef::Null) | Err(_) => ColumnType::Unknown,
                    };
                    ColumnSchema {
                        name: name.clone(),
                        dtype,
                        nullable: true,
                        description: None,
                    }
                })
                .collect()
        } else {
            // No rows — return column names with Unknown type
            col_names
                .iter()
                .map(|name| ColumnSchema {
                    name: name.clone(),
                    dtype: ColumnType::Unknown,
                    nullable: true,
                    description: None,
                })
                .collect()
        };

        Ok(SchemaDefinition { columns })
    }
}

// ---------------------------------------------------------------------------
// ApiSource
// ---------------------------------------------------------------------------

/// REST API data source. Fetches JSON data from an HTTP endpoint.
pub struct ApiSource {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
}

#[async_trait]
impl DataSource for ApiSource {
    async fn load(&self, limit: Option<usize>) -> Result<DataBatch, MlError> {
        let client = reqwest::Client::new();
        let mut request = match self.method.to_uppercase().as_str() {
            "POST" => client.post(&self.url),
            _ => client.get(&self.url),
        };
        for (k, v) in &self.headers {
            request = request.header(k.as_str(), v.as_str());
        }

        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(MlError::dataset(format!(
                "API request failed with status {}",
                response.status()
            )));
        }

        let body = response.text().await?;
        let value: serde_json::Value = serde_json::from_str(&body)?;

        let items: Vec<serde_json::Value> = match &value {
            serde_json::Value::Array(arr) => {
                if let Some(max) = limit {
                    arr.iter().take(max).cloned().collect()
                } else {
                    arr.clone()
                }
            }
            serde_json::Value::Object(_) => vec![value],
            _ => {
                return Err(MlError::dataset(
                    "API response must be a JSON array or object",
                ));
            }
        };

        let columns: Vec<String> = if let Some(first) = items.first() {
            if let serde_json::Value::Object(map) = first {
                map.keys().cloned().collect()
            } else {
                vec!["value".to_string()]
            }
        } else {
            return Ok(DataBatch::empty());
        };

        let rows: Vec<Vec<serde_json::Value>> = items
            .iter()
            .map(|item| {
                columns
                    .iter()
                    .map(|col| item.get(col).cloned().unwrap_or(serde_json::Value::Null))
                    .collect()
            })
            .collect();

        let total_rows = rows.len();
        Ok(DataBatch {
            columns,
            rows,
            total_rows,
        })
    }

    fn source_info(&self) -> DataSourceInfo {
        DataSourceInfo {
            source_type: "api".to_string(),
            location: self.url.clone(),
            accessed_at: chrono::Utc::now(),
            row_count: None,
        }
    }

    fn schema(&self) -> Result<SchemaDefinition, MlError> {
        // Schema cannot be determined without making a network request.
        // Return an empty schema; callers should load data first and infer.
        Ok(SchemaDefinition {
            columns: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// HuggingFaceSource
// ---------------------------------------------------------------------------

/// Hugging Face Datasets Hub data source.
///
/// Uses the Hugging Face datasets API to fetch rows. Requires network access.
pub struct HuggingFaceSource {
    pub dataset_name: String,
    pub split: String,
}

#[async_trait]
impl DataSource for HuggingFaceSource {
    async fn load(&self, limit: Option<usize>) -> Result<DataBatch, MlError> {
        let row_limit = limit.unwrap_or(100);
        let url = format!(
            "https://datasets-server.huggingface.co/rows?dataset={}&config=default&split={}&offset=0&length={row_limit}",
            self.dataset_name, self.split
        );

        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;
        if !response.status().is_success() {
            return Err(MlError::dataset(format!(
                "Hugging Face API returned status {}",
                response.status()
            )));
        }

        let body: serde_json::Value = response.json().await?;

        // The HF datasets-server returns { "rows": [ { "row": {...} }, ... ], "features": [...] }
        let rows_array = body
            .get("rows")
            .and_then(|v| v.as_array())
            .ok_or_else(|| MlError::dataset("Unexpected Hugging Face API response format"))?;

        let items: Vec<serde_json::Value> = rows_array
            .iter()
            .filter_map(|entry| entry.get("row").cloned())
            .collect();

        let columns: Vec<String> = if let Some(first) = items.first() {
            if let serde_json::Value::Object(map) = first {
                map.keys().cloned().collect()
            } else {
                vec!["value".to_string()]
            }
        } else {
            return Ok(DataBatch::empty());
        };

        let rows: Vec<Vec<serde_json::Value>> = items
            .iter()
            .map(|item| {
                columns
                    .iter()
                    .map(|col| item.get(col).cloned().unwrap_or(serde_json::Value::Null))
                    .collect()
            })
            .collect();

        let total_rows = rows.len();
        Ok(DataBatch {
            columns,
            rows,
            total_rows,
        })
    }

    fn source_info(&self) -> DataSourceInfo {
        DataSourceInfo {
            source_type: "huggingface".to_string(),
            location: format!("hf://datasets/{}/{}", self.dataset_name, self.split),
            accessed_at: chrono::Utc::now(),
            row_count: None,
        }
    }

    fn schema(&self) -> Result<SchemaDefinition, MlError> {
        // Schema requires an API call; return empty schema for synchronous callers.
        // Use load() + infer_schema() for full schema inference.
        Ok(SchemaDefinition {
            columns: Vec::new(),
        })
    }
}

// ---------------------------------------------------------------------------
// ParquetSource (stub, requires `columnar` feature for real implementation)
// ---------------------------------------------------------------------------

/// Apache Parquet data source (stub).
///
/// A full implementation requires the `columnar` feature with arrow/parquet dependencies.
/// This stub allows the type to exist and returns an informative error on load.
pub struct ParquetSource {
    pub path: PathBuf,
}

#[async_trait]
impl DataSource for ParquetSource {
    async fn load(&self, _limit: Option<usize>) -> Result<DataBatch, MlError> {
        Err(MlError::dataset(
            "Parquet support requires the `columnar` feature flag (arrow + parquet deps)",
        ))
    }

    fn source_info(&self) -> DataSourceInfo {
        DataSourceInfo {
            source_type: "parquet".to_string(),
            location: self.path.display().to_string(),
            accessed_at: chrono::Utc::now(),
            row_count: None,
        }
    }

    fn schema(&self) -> Result<SchemaDefinition, MlError> {
        Err(MlError::dataset(
            "Parquet schema reading requires the `columnar` feature flag (arrow + parquet deps)",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_data_batch_empty() {
        let batch = DataBatch::empty();
        assert_eq!(batch.row_count(), 0);
        assert_eq!(batch.column_count(), 0);
    }

    #[test]
    fn test_data_source_type_serde() {
        let src = DataSourceType::Csv {
            path: PathBuf::from("data.csv"),
            delimiter: ',',
        };
        let json = serde_json::to_string(&src).unwrap();
        assert!(json.contains("csv"));
    }

    #[test]
    fn test_data_source_type_parquet_serde() {
        let src = DataSourceType::Parquet {
            path: PathBuf::from("data.parquet"),
        };
        let json = serde_json::to_string(&src).unwrap();
        assert!(json.contains("parquet"));
    }

    #[test]
    fn test_jsonl_source_info() {
        let src = JsonlSource {
            path: PathBuf::from("data.jsonl"),
        };
        let info = src.source_info();
        assert_eq!(info.source_type, "jsonl");
    }

    #[test]
    fn test_sqlite_source_info() {
        let src = SqliteSource {
            db_path: PathBuf::from("test.db"),
            query: "SELECT * FROM t".into(),
        };
        let info = src.source_info();
        assert_eq!(info.source_type, "sqlite");
    }

    #[test]
    fn test_api_source_info() {
        let src = ApiSource {
            url: "https://example.com/api".into(),
            method: "GET".into(),
            headers: HashMap::new(),
        };
        let info = src.source_info();
        assert_eq!(info.source_type, "api");
        assert_eq!(info.location, "https://example.com/api");
    }

    #[test]
    fn test_huggingface_source_info() {
        let src = HuggingFaceSource {
            dataset_name: "squad".into(),
            split: "train".into(),
        };
        let info = src.source_info();
        assert_eq!(info.source_type, "huggingface");
        assert!(info.location.contains("squad"));
    }

    #[test]
    fn test_parquet_source_info() {
        let src = ParquetSource {
            path: PathBuf::from("data.parquet"),
        };
        let info = src.source_info();
        assert_eq!(info.source_type, "parquet");
    }

    #[test]
    fn test_api_source_empty_schema() {
        let src = ApiSource {
            url: "https://example.com/api".into(),
            method: "GET".into(),
            headers: HashMap::new(),
        };
        let schema = src.schema().unwrap();
        assert!(schema.columns.is_empty());
    }

    #[test]
    fn test_huggingface_source_empty_schema() {
        let src = HuggingFaceSource {
            dataset_name: "test".into(),
            split: "train".into(),
        };
        let schema = src.schema().unwrap();
        assert!(schema.columns.is_empty());
    }
}
