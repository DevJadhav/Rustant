//! Travel planner tool — itineraries, timezone conversion, packing lists.

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use chrono_tz::Tz;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::PathBuf;

use crate::registry::Tool;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Itinerary {
    id: usize,
    name: String,
    destination: String,
    segments: Vec<TravelSegment>,
    packing_list: Vec<String>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TravelSegment {
    description: String,
    from: String,
    to: String,
    departure: String, // ISO datetime string
    arrival: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct TravelState {
    itineraries: Vec<Itinerary>,
    next_id: usize,
}

pub struct TravelTool {
    workspace: PathBuf,
}

impl TravelTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }

    fn state_path(&self) -> PathBuf {
        self.workspace
            .join(".rustant")
            .join("travel")
            .join("trips.json")
    }

    fn load_state(&self) -> TravelState {
        let path = self.state_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            TravelState {
                itineraries: Vec::new(),
                next_id: 1,
            }
        }
    }

    fn save_state(&self, state: &TravelState) -> Result<(), ToolError> {
        let path = self.state_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| ToolError::ExecutionFailed {
                name: "travel".to_string(),
                message: e.to_string(),
            })?;
        }
        let json = serde_json::to_string_pretty(state).map_err(|e| ToolError::ExecutionFailed {
            name: "travel".to_string(),
            message: e.to_string(),
        })?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, &json).map_err(|e| ToolError::ExecutionFailed {
            name: "travel".to_string(),
            message: e.to_string(),
        })?;
        std::fs::rename(&tmp, &path).map_err(|e| ToolError::ExecutionFailed {
            name: "travel".to_string(),
            message: e.to_string(),
        })?;
        Ok(())
    }
}

#[async_trait]
impl Tool for TravelTool {
    fn name(&self) -> &str {
        "travel"
    }
    fn description(&self) -> &str {
        "Travel itinerary planner. Actions: create_itinerary, add_segment, list, timezone_convert, packing_list."
    }
    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "enum": ["create_itinerary", "add_segment", "list", "timezone_convert", "packing_list"] },
                "name": { "type": "string", "description": "Trip name" },
                "destination": { "type": "string", "description": "Destination" },
                "id": { "type": "integer", "description": "Itinerary ID" },
                "description": { "type": "string", "description": "Segment description" },
                "from": { "type": "string", "description": "Departure location" },
                "to": { "type": "string", "description": "Arrival location" },
                "departure": { "type": "string", "description": "Departure datetime (ISO format)" },
                "arrival": { "type": "string", "description": "Arrival datetime (ISO format)" },
                "time": { "type": "string", "description": "Time to convert (YYYY-MM-DD HH:MM)" },
                "from_tz": { "type": "string", "description": "Source timezone (e.g., America/New_York)" },
                "to_tz": { "type": "string", "description": "Target timezone (e.g., Europe/London)" },
                "items": { "type": "array", "items": { "type": "string" }, "description": "Packing list items to add" }
            },
            "required": ["action"]
        })
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, args: Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let mut state = self.load_state();

        match action {
            "create_itinerary" => {
                let name = args
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unnamed trip");
                let dest = args
                    .get("destination")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let id = state.next_id;
                state.next_id += 1;
                state.itineraries.push(Itinerary {
                    id,
                    name: name.to_string(),
                    destination: dest.to_string(),
                    segments: Vec::new(),
                    packing_list: Vec::new(),
                    created_at: Utc::now(),
                });
                self.save_state(&state)?;
                Ok(ToolOutput::text(format!(
                    "Created itinerary #{id}: '{name}' to {dest}"
                )))
            }
            "add_segment" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let desc = args
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let from = args.get("from").and_then(|v| v.as_str()).unwrap_or("");
                let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("");
                let departure = args.get("departure").and_then(|v| v.as_str()).unwrap_or("");
                let arrival = args.get("arrival").and_then(|v| v.as_str()).unwrap_or("");
                if let Some(itin) = state.itineraries.iter_mut().find(|i| i.id == id) {
                    itin.segments.push(TravelSegment {
                        description: desc.to_string(),
                        from: from.to_string(),
                        to: to.to_string(),
                        departure: departure.to_string(),
                        arrival: arrival.to_string(),
                    });
                    let itin_name = itin.name.clone();
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!(
                        "Added segment to '{itin_name}': {from} -> {to}"
                    )))
                } else {
                    Ok(ToolOutput::text(format!("Itinerary #{id} not found.")))
                }
            }
            "list" => {
                if state.itineraries.is_empty() {
                    return Ok(ToolOutput::text("No itineraries yet."));
                }
                let lines: Vec<String> = state
                    .itineraries
                    .iter()
                    .map(|i| {
                        format!(
                            "  #{} — {} to {} ({} segments)",
                            i.id,
                            i.name,
                            i.destination,
                            i.segments.len()
                        )
                    })
                    .collect();
                Ok(ToolOutput::text(format!(
                    "Itineraries:\n{}",
                    lines.join("\n")
                )))
            }
            "timezone_convert" => {
                let time_str = args.get("time").and_then(|v| v.as_str()).unwrap_or("");
                let from_tz_str = args
                    .get("from_tz")
                    .and_then(|v| v.as_str())
                    .unwrap_or("UTC");
                let to_tz_str = args.get("to_tz").and_then(|v| v.as_str()).unwrap_or("UTC");

                let from_tz: Tz = from_tz_str
                    .parse()
                    .map_err(|_| ToolError::ExecutionFailed {
                        name: "travel".to_string(),
                        message: format!("Invalid timezone: {from_tz_str}"),
                    })?;
                let to_tz: Tz = to_tz_str.parse().map_err(|_| ToolError::ExecutionFailed {
                    name: "travel".to_string(),
                    message: format!("Invalid timezone: {to_tz_str}"),
                })?;

                let naive = NaiveDateTime::parse_from_str(time_str, "%Y-%m-%d %H:%M")
                    .or_else(|_| NaiveDateTime::parse_from_str(time_str, "%Y-%m-%dT%H:%M"))
                    .map_err(|_| ToolError::ExecutionFailed {
                        name: "travel".to_string(),
                        message: format!("Invalid time format: {time_str}. Use YYYY-MM-DD HH:MM"),
                    })?;

                let from_dt = naive.and_local_timezone(from_tz).single().ok_or_else(|| {
                    ToolError::ExecutionFailed {
                        name: "travel".to_string(),
                        message: "Ambiguous timezone conversion".to_string(),
                    }
                })?;
                let to_dt = from_dt.with_timezone(&to_tz);

                Ok(ToolOutput::text(format!(
                    "{} {} -> {} {}",
                    from_dt.format("%Y-%m-%d %H:%M"),
                    from_tz_str,
                    to_dt.format("%Y-%m-%d %H:%M"),
                    to_tz_str
                )))
            }
            "packing_list" => {
                let id = args.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                let items: Vec<String> = args
                    .get("items")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();
                if let Some(itin) = state.itineraries.iter_mut().find(|i| i.id == id) {
                    if items.is_empty() {
                        // Show current list
                        if itin.packing_list.is_empty() {
                            return Ok(ToolOutput::text(format!(
                                "Packing list for '{}' is empty.",
                                itin.name
                            )));
                        }
                        let lines: Vec<String> = itin
                            .packing_list
                            .iter()
                            .enumerate()
                            .map(|(i, item)| format!("  {}. {}", i + 1, item))
                            .collect();
                        return Ok(ToolOutput::text(format!(
                            "Packing list for '{}':\n{}",
                            itin.name,
                            lines.join("\n")
                        )));
                    }
                    itin.packing_list.extend(items.clone());
                    let itin_name = itin.name.clone();
                    self.save_state(&state)?;
                    Ok(ToolOutput::text(format!(
                        "Added {} items to packing list for '{}'.",
                        items.len(),
                        itin_name
                    )))
                } else {
                    Ok(ToolOutput::text(format!("Itinerary #{id} not found.")))
                }
            }
            _ => Ok(ToolOutput::text(format!("Unknown action: {action}."))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_travel_timezone_conversion() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = TravelTool::new(workspace);
        let result = tool
            .execute(json!({
                "action": "timezone_convert",
                "time": "2025-06-15 10:00",
                "from_tz": "America/New_York",
                "to_tz": "Europe/London"
            }))
            .await
            .unwrap();
        assert!(result.content.contains("15:00") || result.content.contains("14:00"));
        // EDT or EST
    }

    #[tokio::test]
    async fn test_travel_create_and_list() {
        let dir = TempDir::new().unwrap();
        let workspace = dir.path().canonicalize().unwrap();
        let tool = TravelTool::new(workspace);
        tool.execute(
            json!({"action": "create_itinerary", "name": "Euro Trip", "destination": "Paris"}),
        )
        .await
        .unwrap();
        let result = tool.execute(json!({"action": "list"})).await.unwrap();
        assert!(result.content.contains("Euro Trip"));
        assert!(result.content.contains("Paris"));
    }

    #[tokio::test]
    async fn test_travel_schema() {
        let dir = TempDir::new().unwrap();
        let tool = TravelTool::new(dir.path().to_path_buf());
        assert_eq!(tool.name(), "travel");
    }
}
