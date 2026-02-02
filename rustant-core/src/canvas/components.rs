//! Canvas component specifications.
//!
//! Defines structured specs for charts, tables, forms, and diagrams
//! that the renderer converts to HTML/JS for display.

use serde::{Deserialize, Serialize};

/// Chart specification (rendered via Chart.js).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSpec {
    /// Chart type: "line", "bar", "pie", "scatter", "doughnut".
    pub chart_type: String,
    /// Data labels (x-axis or category labels).
    pub labels: Vec<String>,
    /// Dataset(s). Each dataset has a label and numeric data.
    pub datasets: Vec<ChartDataset>,
    /// Optional title for the chart.
    #[serde(default)]
    pub title: Option<String>,
}

/// A single dataset in a chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartDataset {
    pub label: String,
    pub data: Vec<f64>,
    #[serde(default)]
    pub color: Option<String>,
}

impl ChartSpec {
    /// Create a simple single-dataset chart.
    pub fn simple(chart_type: &str, labels: Vec<String>, data: Vec<f64>) -> Self {
        Self {
            chart_type: chart_type.into(),
            labels,
            datasets: vec![ChartDataset {
                label: "Data".into(),
                data,
                color: None,
            }],
            title: None,
        }
    }

    /// Validate that chart_type is a known type.
    pub fn is_valid_type(&self) -> bool {
        matches!(
            self.chart_type.as_str(),
            "line" | "bar" | "pie" | "scatter" | "doughnut" | "radar" | "polarArea"
        )
    }
}

/// Table specification (sortable HTML table).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSpec {
    /// Column headers.
    pub headers: Vec<String>,
    /// Row data (each row is a vec of cell values).
    pub rows: Vec<Vec<String>>,
    /// Whether columns should be sortable.
    #[serde(default)]
    pub sortable: bool,
}

impl TableSpec {
    pub fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            headers,
            rows,
            sortable: false,
        }
    }
}

/// Form specification (validated HTML form).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormSpec {
    /// Form fields.
    pub fields: Vec<FormField>,
    /// Optional submit button text.
    #[serde(default = "default_submit_text")]
    pub submit_text: String,
    /// Optional form title.
    #[serde(default)]
    pub title: Option<String>,
}

fn default_submit_text() -> String {
    "Submit".into()
}

/// A form field definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    /// Field name (used as form key).
    pub name: String,
    /// Display label.
    pub label: String,
    /// Field type: "text", "number", "email", "select", "textarea", "checkbox".
    pub field_type: String,
    /// Whether the field is required.
    #[serde(default)]
    pub required: bool,
    /// Placeholder text.
    #[serde(default)]
    pub placeholder: Option<String>,
    /// Options for select fields.
    #[serde(default)]
    pub options: Vec<String>,
    /// Default value.
    #[serde(default)]
    pub default_value: Option<String>,
}

/// Diagram specification (rendered via Mermaid).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagramSpec {
    /// Mermaid source code for the diagram.
    pub source: String,
    /// Optional title.
    #[serde(default)]
    pub title: Option<String>,
}

impl DiagramSpec {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.into(),
            title: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chart_spec_simple() {
        let chart = ChartSpec::simple(
            "bar",
            vec!["A".into(), "B".into(), "C".into()],
            vec![1.0, 2.0, 3.0],
        );
        assert_eq!(chart.chart_type, "bar");
        assert_eq!(chart.labels.len(), 3);
        assert_eq!(chart.datasets.len(), 1);
        assert_eq!(chart.datasets[0].data, vec![1.0, 2.0, 3.0]);
        assert!(chart.is_valid_type());
    }

    #[test]
    fn test_chart_spec_serialization() {
        let chart = ChartSpec::simple("line", vec!["Jan".into(), "Feb".into()], vec![10.0, 20.0]);
        let json = serde_json::to_string(&chart).unwrap();
        let restored: ChartSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.chart_type, "line");
        assert_eq!(restored.datasets[0].data.len(), 2);
    }

    #[test]
    fn test_chart_valid_types() {
        for t in &[
            "line",
            "bar",
            "pie",
            "scatter",
            "doughnut",
            "radar",
            "polarArea",
        ] {
            let c = ChartSpec::simple(t, vec![], vec![]);
            assert!(c.is_valid_type(), "Expected {} to be valid", t);
        }
        let invalid = ChartSpec::simple("unknown", vec![], vec![]);
        assert!(!invalid.is_valid_type());
    }

    #[test]
    fn test_table_spec() {
        let table = TableSpec::new(
            vec!["Name".into(), "Age".into()],
            vec![
                vec!["Alice".into(), "30".into()],
                vec!["Bob".into(), "25".into()],
            ],
        );
        assert_eq!(table.headers.len(), 2);
        assert_eq!(table.rows.len(), 2);
        assert!(!table.sortable);
    }

    #[test]
    fn test_table_spec_serialization() {
        let table = TableSpec {
            headers: vec!["Col1".into()],
            rows: vec![vec!["Val1".into()]],
            sortable: true,
        };
        let json = serde_json::to_string(&table).unwrap();
        let restored: TableSpec = serde_json::from_str(&json).unwrap();
        assert!(restored.sortable);
    }

    #[test]
    fn test_form_spec() {
        let form = FormSpec {
            fields: vec![FormField {
                name: "email".into(),
                label: "Email".into(),
                field_type: "email".into(),
                required: true,
                placeholder: Some("user@example.com".into()),
                options: vec![],
                default_value: None,
            }],
            submit_text: "Send".into(),
            title: Some("Contact".into()),
        };
        assert_eq!(form.fields.len(), 1);
        assert!(form.fields[0].required);
    }

    #[test]
    fn test_form_spec_serialization() {
        let form = FormSpec {
            fields: vec![FormField {
                name: "name".into(),
                label: "Name".into(),
                field_type: "text".into(),
                required: false,
                placeholder: None,
                options: vec![],
                default_value: Some("default".into()),
            }],
            submit_text: "Submit".into(),
            title: None,
        };
        let json = serde_json::to_string(&form).unwrap();
        let restored: FormSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.fields[0].default_value, Some("default".into()));
    }

    #[test]
    fn test_diagram_spec() {
        let diagram = DiagramSpec::new("graph LR; A-->B; B-->C");
        assert!(diagram.source.contains("graph LR"));
        assert!(diagram.title.is_none());
    }

    #[test]
    fn test_diagram_spec_serialization() {
        let diagram = DiagramSpec {
            source: "sequenceDiagram\n  A->>B: Hello".into(),
            title: Some("Sequence".into()),
        };
        let json = serde_json::to_string(&diagram).unwrap();
        let restored: DiagramSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.title, Some("Sequence".into()));
    }
}
