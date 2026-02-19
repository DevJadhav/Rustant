//! Schema definition and type inference for datasets.

use serde::{Deserialize, Serialize};

/// Column data type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColumnType {
    Integer,
    Float,
    String,
    Boolean,
    DateTime,
    Json,
    Null,
    Unknown,
}

/// Schema definition for a dataset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaDefinition {
    pub columns: Vec<ColumnSchema>,
}

/// Schema for a single column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnSchema {
    pub name: String,
    pub dtype: ColumnType,
    pub nullable: bool,
    pub description: Option<String>,
}

/// Statistics for a single column.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnStats {
    pub name: String,
    pub dtype: ColumnType,
    pub null_count: usize,
    pub unique_count: usize,
    pub min: Option<serde_json::Value>,
    pub max: Option<serde_json::Value>,
    pub mean: Option<f64>,
    pub std_dev: Option<f64>,
}

/// Infer column type from a sample of values.
pub fn infer_column_type(values: &[serde_json::Value]) -> ColumnType {
    let non_null: Vec<_> = values.iter().filter(|v| !v.is_null()).collect();
    if non_null.is_empty() {
        return ColumnType::Null;
    }

    let mut has_int = false;
    let mut has_float = false;
    let mut has_bool = false;
    let mut has_string = false;

    for v in &non_null {
        match v {
            serde_json::Value::Number(n) => {
                if n.is_f64() {
                    has_float = true;
                } else {
                    has_int = true;
                }
            }
            serde_json::Value::Bool(_) => has_bool = true,
            serde_json::Value::String(_) => has_string = true,
            _ => {}
        }
    }

    if has_string {
        return ColumnType::String;
    }
    if has_float {
        return ColumnType::Float;
    }
    if has_int {
        return ColumnType::Integer;
    }
    if has_bool {
        return ColumnType::Boolean;
    }
    ColumnType::Unknown
}

/// Infer schema from a data batch.
pub fn infer_schema(columns: &[String], rows: &[Vec<serde_json::Value>]) -> SchemaDefinition {
    let mut schema_columns = Vec::new();

    for (i, col_name) in columns.iter().enumerate() {
        let values: Vec<serde_json::Value> =
            rows.iter().filter_map(|row| row.get(i).cloned()).collect();

        let dtype = infer_column_type(&values);
        let nullable = values.iter().any(|v| v.is_null());

        schema_columns.push(ColumnSchema {
            name: col_name.clone(),
            dtype,
            nullable,
            description: None,
        });
    }

    SchemaDefinition {
        columns: schema_columns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_column_type_int() {
        let values = vec![
            serde_json::json!(1),
            serde_json::json!(2),
            serde_json::json!(3),
        ];
        assert_eq!(infer_column_type(&values), ColumnType::Integer);
    }

    #[test]
    fn test_infer_column_type_string() {
        let values = vec![serde_json::json!("a"), serde_json::json!("b")];
        assert_eq!(infer_column_type(&values), ColumnType::String);
    }

    #[test]
    fn test_infer_schema() {
        let columns = vec!["name".to_string(), "age".to_string()];
        let rows = vec![
            vec![serde_json::json!("Alice"), serde_json::json!(30)],
            vec![serde_json::json!("Bob"), serde_json::json!(25)],
        ];
        let schema = infer_schema(&columns, &rows);
        assert_eq!(schema.columns.len(), 2);
        assert_eq!(schema.columns[0].dtype, ColumnType::String);
        assert_eq!(schema.columns[1].dtype, ColumnType::Integer);
    }
}
