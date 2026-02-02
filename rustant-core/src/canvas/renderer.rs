//! Canvas server-side renderer.
//!
//! Converts component specs (ChartSpec, TableSpec, FormSpec, DiagramSpec)
//! into HTML/JS strings suitable for embedding in the dashboard or canvas UI.

use super::components::{ChartSpec, DiagramSpec, FormSpec, TableSpec};

/// Render a ChartSpec to a Chart.js config JSON string.
pub fn render_chart_config(spec: &ChartSpec) -> String {
    let datasets: Vec<serde_json::Value> = spec
        .datasets
        .iter()
        .map(|ds| {
            let mut obj = serde_json::json!({
                "label": ds.label,
                "data": ds.data,
            });
            if let Some(color) = &ds.color {
                obj["borderColor"] = serde_json::json!(color);
                obj["backgroundColor"] = serde_json::json!(color);
            }
            obj
        })
        .collect();

    let config = serde_json::json!({
        "type": spec.chart_type,
        "data": {
            "labels": spec.labels,
            "datasets": datasets,
        },
        "options": {
            "responsive": true,
            "plugins": {
                "title": {
                    "display": spec.title.is_some(),
                    "text": spec.title.as_deref().unwrap_or(""),
                }
            }
        }
    });

    serde_json::to_string_pretty(&config).unwrap_or_else(|_| "{}".into())
}

/// Render a TableSpec to an HTML table string.
pub fn render_table_html(spec: &TableSpec) -> String {
    let mut html = String::from("<table class=\"canvas-table\">\n<thead><tr>\n");
    for header in &spec.headers {
        if spec.sortable {
            html.push_str(&format!(
                "  <th class=\"sortable\" onclick=\"sortTable(this)\">{}</th>\n",
                escape_html(header)
            ));
        } else {
            html.push_str(&format!("  <th>{}</th>\n", escape_html(header)));
        }
    }
    html.push_str("</tr></thead>\n<tbody>\n");
    for row in &spec.rows {
        html.push_str("<tr>\n");
        for cell in row {
            html.push_str(&format!("  <td>{}</td>\n", escape_html(cell)));
        }
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody>\n</table>");
    html
}

/// Render a FormSpec to an HTML form string.
pub fn render_form_html(spec: &FormSpec) -> String {
    let mut html = String::from("<form class=\"canvas-form\">\n");
    if let Some(title) = &spec.title {
        html.push_str(&format!("<h3>{}</h3>\n", escape_html(title)));
    }
    for field in &spec.fields {
        html.push_str("<div class=\"form-group\">\n");
        html.push_str(&format!(
            "  <label for=\"{}\">{}</label>\n",
            escape_html(&field.name),
            escape_html(&field.label)
        ));
        let required = if field.required { " required" } else { "" };
        let placeholder = field
            .placeholder
            .as_deref()
            .map(|p| format!(" placeholder=\"{}\"", escape_html(p)))
            .unwrap_or_default();
        let default_val = field
            .default_value
            .as_deref()
            .map(|v| format!(" value=\"{}\"", escape_html(v)))
            .unwrap_or_default();

        match field.field_type.as_str() {
            "textarea" => {
                let val = field.default_value.as_deref().unwrap_or("");
                html.push_str(&format!(
                    "  <textarea name=\"{}\" id=\"{}\"{}{}>{}</textarea>\n",
                    escape_html(&field.name),
                    escape_html(&field.name),
                    placeholder,
                    required,
                    escape_html(val)
                ));
            }
            "select" => {
                html.push_str(&format!(
                    "  <select name=\"{}\" id=\"{}\"{}>\n",
                    escape_html(&field.name),
                    escape_html(&field.name),
                    required,
                ));
                for opt in &field.options {
                    let selected = field
                        .default_value
                        .as_deref()
                        .map(|v| if v == opt { " selected" } else { "" })
                        .unwrap_or("");
                    html.push_str(&format!(
                        "    <option value=\"{}\"{}>{}</option>\n",
                        escape_html(opt),
                        selected,
                        escape_html(opt)
                    ));
                }
                html.push_str("  </select>\n");
            }
            "checkbox" => {
                let checked = field
                    .default_value
                    .as_deref()
                    .map(|v| {
                        if v == "true" || v == "1" {
                            " checked"
                        } else {
                            ""
                        }
                    })
                    .unwrap_or("");
                html.push_str(&format!(
                    "  <input type=\"checkbox\" name=\"{}\" id=\"{}\"{}{}>\n",
                    escape_html(&field.name),
                    escape_html(&field.name),
                    checked,
                    required,
                ));
            }
            _ => {
                html.push_str(&format!(
                    "  <input type=\"{}\" name=\"{}\" id=\"{}\"{}{}{}>\n",
                    escape_html(&field.field_type),
                    escape_html(&field.name),
                    escape_html(&field.name),
                    placeholder,
                    default_val,
                    required,
                ));
            }
        }
        html.push_str("</div>\n");
    }
    html.push_str(&format!(
        "<button type=\"submit\">{}</button>\n</form>",
        escape_html(&spec.submit_text)
    ));
    html
}

/// Render a DiagramSpec to Mermaid markup.
pub fn render_diagram_mermaid(spec: &DiagramSpec) -> String {
    let mut output = String::new();
    if let Some(title) = &spec.title {
        output.push_str(&format!("---\ntitle: {}\n---\n", title));
    }
    output.push_str(&spec.source);
    output
}

/// Escape HTML special characters.
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::components::*;

    #[test]
    fn test_render_chart_config_bar() {
        let spec = ChartSpec::simple("bar", vec!["A".into(), "B".into()], vec![1.0, 2.0]);
        let json_str = render_chart_config(&spec);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["type"], "bar");
        assert_eq!(parsed["data"]["labels"][0], "A");
        assert_eq!(parsed["data"]["datasets"][0]["data"][0], 1.0);
    }

    #[test]
    fn test_render_chart_config_line_with_title() {
        let mut spec = ChartSpec::simple("line", vec!["Jan".into()], vec![10.0]);
        spec.title = Some("Monthly".into());
        let json_str = render_chart_config(&spec);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["type"], "line");
        assert_eq!(parsed["options"]["plugins"]["title"]["display"], true);
        assert_eq!(parsed["options"]["plugins"]["title"]["text"], "Monthly");
    }

    #[test]
    fn test_render_chart_config_pie() {
        let spec = ChartSpec::simple(
            "pie",
            vec!["Red".into(), "Blue".into(), "Green".into()],
            vec![30.0, 50.0, 20.0],
        );
        let json_str = render_chart_config(&spec);
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["type"], "pie");
    }

    #[test]
    fn test_render_chart_config_scatter() {
        let spec = ChartSpec::simple("scatter", vec!["1".into(), "2".into()], vec![5.0, 10.0]);
        let json_str = render_chart_config(&spec);
        assert!(json_str.contains("scatter"));
    }

    #[test]
    fn test_render_table_html() {
        let spec = TableSpec::new(
            vec!["Name".into(), "Age".into()],
            vec![
                vec!["Alice".into(), "30".into()],
                vec!["Bob".into(), "25".into()],
            ],
        );
        let html = render_table_html(&spec);
        assert!(html.contains("<table"));
        assert!(html.contains("<th>Name</th>"));
        assert!(html.contains("<td>Alice</td>"));
        assert!(html.contains("<td>30</td>"));
    }

    #[test]
    fn test_render_table_html_sortable() {
        let spec = TableSpec {
            headers: vec!["Col".into()],
            rows: vec![vec!["Val".into()]],
            sortable: true,
        };
        let html = render_table_html(&spec);
        assert!(html.contains("sortable"));
        assert!(html.contains("onclick"));
    }

    #[test]
    fn test_render_form_html() {
        let spec = FormSpec {
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
        let html = render_form_html(&spec);
        assert!(html.contains("<form"));
        assert!(html.contains("<h3>Contact</h3>"));
        assert!(html.contains("type=\"email\""));
        assert!(html.contains("required"));
        assert!(html.contains("placeholder=\"user@example.com\""));
        assert!(html.contains("Send"));
    }

    #[test]
    fn test_render_form_html_select() {
        let spec = FormSpec {
            fields: vec![FormField {
                name: "color".into(),
                label: "Color".into(),
                field_type: "select".into(),
                required: false,
                placeholder: None,
                options: vec!["red".into(), "blue".into(), "green".into()],
                default_value: Some("blue".into()),
            }],
            submit_text: "Submit".into(),
            title: None,
        };
        let html = render_form_html(&spec);
        assert!(html.contains("<select"));
        assert!(html.contains("<option value=\"blue\" selected>blue</option>"));
    }

    #[test]
    fn test_render_form_html_textarea() {
        let spec = FormSpec {
            fields: vec![FormField {
                name: "notes".into(),
                label: "Notes".into(),
                field_type: "textarea".into(),
                required: false,
                placeholder: Some("Enter notes...".into()),
                options: vec![],
                default_value: Some("Default notes".into()),
            }],
            submit_text: "Save".into(),
            title: None,
        };
        let html = render_form_html(&spec);
        assert!(html.contains("<textarea"));
        assert!(html.contains("Default notes"));
    }

    #[test]
    fn test_render_diagram_mermaid() {
        let spec = DiagramSpec::new("graph LR; A-->B; B-->C");
        let output = render_diagram_mermaid(&spec);
        assert_eq!(output, "graph LR; A-->B; B-->C");
    }

    #[test]
    fn test_render_diagram_mermaid_with_title() {
        let spec = DiagramSpec {
            source: "graph TD; X-->Y".into(),
            title: Some("Flow".into()),
        };
        let output = render_diagram_mermaid(&spec);
        assert!(output.starts_with("---\ntitle: Flow\n---\n"));
        assert!(output.contains("graph TD; X-->Y"));
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(
            escape_html("<script>alert('xss')</script>"),
            "&lt;script&gt;alert(&#39;xss&#39;)&lt;/script&gt;"
        );
        assert_eq!(escape_html("a & b"), "a &amp; b");
        assert_eq!(escape_html("\"quoted\""), "&quot;quoted&quot;");
    }
}
