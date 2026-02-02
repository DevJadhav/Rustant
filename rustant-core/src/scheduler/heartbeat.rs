//! Heartbeat manager — periodic task triggers with cooldowns and quiet hours.

use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for the heartbeat system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatConfig {
    /// Interval in seconds between heartbeat ticks.
    pub interval_secs: u64,
    /// Tasks to check on each heartbeat.
    pub tasks: Vec<HeartbeatTask>,
    /// Optional quiet hours during which no tasks run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quiet_hours: Option<QuietHours>,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            tasks: Vec::new(),
            quiet_hours: None,
        }
    }
}

/// A task that runs on heartbeat ticks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatTask {
    /// Unique name for this task.
    pub name: String,
    /// Optional condition (e.g., "file_changed:Cargo.toml").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    /// The action to perform.
    pub action: String,
    /// Minimum seconds between executions.
    pub cooldown_secs: u64,
}

/// Quiet hours configuration — suppress tasks during this window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuietHours {
    /// Start time in HH:MM format.
    pub start: String,
    /// End time in HH:MM format.
    pub end: String,
}

impl QuietHours {
    /// Check if the given time falls within quiet hours.
    pub fn is_active(&self, now: &DateTime<Utc>) -> bool {
        let current_time = now.time();
        let start = match NaiveTime::parse_from_str(&self.start, "%H:%M") {
            Ok(t) => t,
            Err(_) => return false,
        };
        let end = match NaiveTime::parse_from_str(&self.end, "%H:%M") {
            Ok(t) => t,
            Err(_) => return false,
        };

        if start <= end {
            // Normal range: e.g., 22:00 to 23:00
            current_time >= start && current_time < end
        } else {
            // Wraps midnight: e.g., 22:00 to 06:00
            current_time >= start || current_time < end
        }
    }
}

/// Manages heartbeat task execution with cooldowns and quiet hours.
pub struct HeartbeatManager {
    config: HeartbeatConfig,
    /// Last execution time per task name.
    last_executed: HashMap<String, DateTime<Utc>>,
}

impl HeartbeatManager {
    pub fn new(config: HeartbeatConfig) -> Self {
        Self {
            config,
            last_executed: HashMap::new(),
        }
    }

    /// Check if quiet hours are currently active.
    pub fn is_quiet(&self) -> bool {
        if let Some(ref quiet) = self.config.quiet_hours {
            quiet.is_active(&Utc::now())
        } else {
            false
        }
    }

    /// Check if quiet hours are active at a specific time.
    pub fn is_quiet_at(&self, time: &DateTime<Utc>) -> bool {
        if let Some(ref quiet) = self.config.quiet_hours {
            quiet.is_active(time)
        } else {
            false
        }
    }

    /// Get the tasks that are ready to run (not in cooldown, not in quiet hours).
    pub fn ready_tasks(&self) -> Vec<&HeartbeatTask> {
        if self.is_quiet() {
            return Vec::new();
        }
        let now = Utc::now();
        self.config
            .tasks
            .iter()
            .filter(|task| self.is_cooldown_expired(task, &now))
            .collect()
    }

    /// Get the tasks that would be ready at a specific time.
    pub fn ready_tasks_at(&self, time: &DateTime<Utc>) -> Vec<&HeartbeatTask> {
        if self.is_quiet_at(time) {
            return Vec::new();
        }
        self.config
            .tasks
            .iter()
            .filter(|task| self.is_cooldown_expired(task, time))
            .collect()
    }

    /// Check if a task's cooldown has expired.
    fn is_cooldown_expired(&self, task: &HeartbeatTask, now: &DateTime<Utc>) -> bool {
        match self.last_executed.get(&task.name) {
            Some(last) => {
                let elapsed = (*now - *last).num_seconds();
                elapsed >= task.cooldown_secs as i64
            }
            None => true, // Never executed, so cooldown is "expired"
        }
    }

    /// Mark a task as having just been executed.
    pub fn mark_executed(&mut self, task_name: &str) {
        self.last_executed
            .insert(task_name.to_string(), Utc::now());
    }

    /// Mark a task as executed at a specific time (for testing).
    pub fn mark_executed_at(&mut self, task_name: &str, time: DateTime<Utc>) {
        self.last_executed.insert(task_name.to_string(), time);
    }

    /// Get the heartbeat config.
    pub fn config(&self) -> &HeartbeatConfig {
        &self.config
    }

    /// Check if a condition is met.
    /// Currently supports "file_changed:<path>" format.
    pub fn check_condition(condition: &str) -> bool {
        if let Some(path) = condition.strip_prefix("file_changed:") {
            // Simple check: file exists (in real use, would track modification times)
            std::path::Path::new(path).exists()
        } else {
            // Unknown condition format — default to true
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_config(quiet_hours: Option<QuietHours>) -> HeartbeatConfig {
        HeartbeatConfig {
            interval_secs: 60,
            tasks: vec![
                HeartbeatTask {
                    name: "check".to_string(),
                    condition: None,
                    action: "run tests".to_string(),
                    cooldown_secs: 300,
                },
                HeartbeatTask {
                    name: "sync".to_string(),
                    condition: Some("file_changed:Cargo.toml".to_string()),
                    action: "sync deps".to_string(),
                    cooldown_secs: 600,
                },
            ],
            quiet_hours,
        }
    }

    #[test]
    fn test_heartbeat_config_defaults() {
        let config = HeartbeatConfig::default();
        assert_eq!(config.interval_secs, 60);
        assert!(config.tasks.is_empty());
        assert!(config.quiet_hours.is_none());
    }

    #[test]
    fn test_heartbeat_quiet_hours_active() {
        // Set quiet hours from 02:00 to 06:00
        let quiet = QuietHours {
            start: "02:00".to_string(),
            end: "06:00".to_string(),
        };
        // 03:00 UTC should be in quiet hours
        let time_in_quiet = Utc.with_ymd_and_hms(2025, 1, 15, 3, 0, 0).unwrap();
        assert!(quiet.is_active(&time_in_quiet));

        let config = make_config(Some(quiet));
        let manager = HeartbeatManager::new(config);
        let ready = manager.ready_tasks_at(&time_in_quiet);
        assert!(ready.is_empty(), "No tasks should run during quiet hours");
    }

    #[test]
    fn test_heartbeat_quiet_hours_inactive() {
        let quiet = QuietHours {
            start: "02:00".to_string(),
            end: "06:00".to_string(),
        };
        // 10:00 UTC should NOT be in quiet hours
        let time_outside = Utc.with_ymd_and_hms(2025, 1, 15, 10, 0, 0).unwrap();
        assert!(!quiet.is_active(&time_outside));

        let config = make_config(Some(quiet));
        let manager = HeartbeatManager::new(config);
        let ready = manager.ready_tasks_at(&time_outside);
        assert_eq!(ready.len(), 2, "Tasks should run outside quiet hours");
    }

    #[test]
    fn test_heartbeat_cooldown_respected() {
        let config = make_config(None);
        let mut manager = HeartbeatManager::new(config);

        // Mark "check" as executed just now
        manager.mark_executed("check");

        // Cooldown is 300s, so task should NOT be ready
        let now = Utc::now();
        let ready = manager.ready_tasks_at(&now);
        // "check" should not be in ready (just executed), but "sync" should be
        let ready_names: Vec<&str> = ready.iter().map(|t| t.name.as_str()).collect();
        assert!(!ready_names.contains(&"check"));
        assert!(ready_names.contains(&"sync"));
    }

    #[test]
    fn test_heartbeat_cooldown_expired() {
        let config = make_config(None);
        let mut manager = HeartbeatManager::new(config);

        // Mark "check" as executed 600 seconds ago (cooldown is 300s)
        let past = Utc::now() - chrono::Duration::seconds(600);
        manager.mark_executed_at("check", past);

        let now = Utc::now();
        let ready = manager.ready_tasks_at(&now);
        let ready_names: Vec<&str> = ready.iter().map(|t| t.name.as_str()).collect();
        assert!(ready_names.contains(&"check"), "Cooldown should have expired");
    }

    #[test]
    fn test_heartbeat_condition_file_changed() {
        // Existing file should return true
        assert!(HeartbeatManager::check_condition("file_changed:Cargo.toml"));
        // Non-existent file should return false
        assert!(!HeartbeatManager::check_condition(
            "file_changed:/nonexistent/path/file.txt"
        ));
    }

    #[test]
    fn test_heartbeat_quiet_hours_wrapping_midnight() {
        let quiet = QuietHours {
            start: "22:00".to_string(),
            end: "06:00".to_string(),
        };
        // 23:00 should be in quiet hours
        let late_night = Utc.with_ymd_and_hms(2025, 1, 15, 23, 0, 0).unwrap();
        assert!(quiet.is_active(&late_night));
        // 03:00 should be in quiet hours
        let early_morning = Utc.with_ymd_and_hms(2025, 1, 16, 3, 0, 0).unwrap();
        assert!(quiet.is_active(&early_morning));
        // 10:00 should NOT be in quiet hours
        let daytime = Utc.with_ymd_and_hms(2025, 1, 15, 10, 0, 0).unwrap();
        assert!(!quiet.is_active(&daytime));
    }
}
