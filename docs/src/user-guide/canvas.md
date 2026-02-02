# Canvas

The canvas system provides rich content rendering capabilities, allowing the agent to display charts, tables, forms, diagrams, and other visual content.

## CLI Usage

```bash
rustant canvas push html "<h1>Hello Canvas</h1>"
rustant canvas push markdown "# Hello\n\nWorld"
rustant canvas push code "fn main() { println!(\"hello\"); }"
rustant canvas push chart '{"type":"bar","labels":["A","B","C"],"data":[1,2,3]}'
rustant canvas push table '{"headers":["Name","Age"],"rows":[["Alice","30"],["Bob","25"]]}'
rustant canvas push form '{"fields":[{"name":"email","field_type":"email","label":"Email"}]}'
rustant canvas push diagram '{"source":"graph LR; A-->B; B-->C"}'
rustant canvas clear
rustant canvas snapshot
```

## Content Types

| Type | Description | Input Format |
|------|-------------|--------------|
| `html` | Raw HTML | String |
| `markdown` | Markdown text | String |
| `code` | Source code | String |
| `chart` | Chart.js chart | JSON (`ChartSpec`) |
| `table` | Sortable table | JSON (`TableSpec`) |
| `form` | Interactive form | JSON (`FormSpec`) |
| `image` | Image (URL or base64) | String |
| `diagram` | Mermaid diagram | JSON (`DiagramSpec`) |

## Chart Specification

```json
{
  "type": "bar",
  "labels": ["Q1", "Q2", "Q3", "Q4"],
  "data": [100, 200, 150, 300],
  "title": "Quarterly Revenue",
  "options": {}
}
```

Supported chart types: `line`, `bar`, `pie`, `scatter`, `doughnut`, `radar`.

## Table Specification

```json
{
  "headers": ["Name", "Role", "Status"],
  "rows": [
    ["Alice", "Engineer", "Active"],
    ["Bob", "Designer", "On Leave"]
  ],
  "sortable": true,
  "caption": "Team Members"
}
```

## Form Specification

```json
{
  "fields": [
    {"name": "name", "field_type": "text", "label": "Name", "required": true},
    {"name": "email", "field_type": "email", "label": "Email"},
    {"name": "role", "field_type": "select", "label": "Role", "options": ["Admin", "User"]}
  ],
  "submit_label": "Save"
}
```

## Diagram Specification

```json
{
  "source": "graph TD; A[Start]-->B{Decision}; B-->|Yes|C[Action]; B-->|No|D[End]"
}
```

Uses Mermaid syntax for diagram rendering.

## Canvas Protocol

The canvas system uses a message-based protocol routed through the gateway:

- `Push` — Add content to the canvas
- `Clear` — Remove all content
- `Update` — Modify existing content
- `Snapshot` — Get current canvas state
- `Interact` — User interaction events (form submissions, clicks)
