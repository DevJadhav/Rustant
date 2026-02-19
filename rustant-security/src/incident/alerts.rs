//! Alert management — correlation, priority scoring, lifecycle, and triage.
//!
//! Groups related findings, manages alert lifecycle, and provides AI triage support.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Alert lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertStatus {
    New,
    Acknowledged,
    Investigating,
    Resolved,
    Closed,
    FalsePositive,
}

impl std::fmt::Display for AlertStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertStatus::New => write!(f, "New"),
            AlertStatus::Acknowledged => write!(f, "Acknowledged"),
            AlertStatus::Investigating => write!(f, "Investigating"),
            AlertStatus::Resolved => write!(f, "Resolved"),
            AlertStatus::Closed => write!(f, "Closed"),
            AlertStatus::FalsePositive => write!(f, "False Positive"),
        }
    }
}

/// Alert priority level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AlertPriority {
    P4,
    P3,
    P2,
    P1,
    P0,
}

impl std::fmt::Display for AlertPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AlertPriority::P0 => write!(f, "P0 - Immediate"),
            AlertPriority::P1 => write!(f, "P1 - Urgent"),
            AlertPriority::P2 => write!(f, "P2 - High"),
            AlertPriority::P3 => write!(f, "P3 - Medium"),
            AlertPriority::P4 => write!(f, "P4 - Low"),
        }
    }
}

/// A security alert (may group multiple related findings/detections).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Unique alert ID.
    pub id: String,
    /// Alert title.
    pub title: String,
    /// Alert description.
    pub description: String,
    /// Current status.
    pub status: AlertStatus,
    /// Priority level.
    pub priority: AlertPriority,
    /// Source finding or detection IDs.
    pub source_ids: Vec<String>,
    /// When the alert was created.
    pub created_at: DateTime<Utc>,
    /// When the alert was last updated.
    pub updated_at: DateTime<Utc>,
    /// Who acknowledged/is investigating.
    pub assignee: Option<String>,
    /// Status change history.
    pub history: Vec<AlertHistoryEntry>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Additional context.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// An entry in the alert status history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertHistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub from_status: AlertStatus,
    pub to_status: AlertStatus,
    pub actor: String,
    pub note: Option<String>,
}

/// Alert correlation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationGroup {
    /// Group identifier.
    pub group_id: String,
    /// Alert IDs in this group.
    pub alert_ids: Vec<String>,
    /// Why these were correlated.
    pub reason: String,
    /// Common attributes.
    pub common_attributes: HashMap<String, String>,
}

/// Alert manager for lifecycle and correlation.
pub struct AlertManager {
    alerts: HashMap<String, Alert>,
    next_id: usize,
}

impl AlertManager {
    pub fn new() -> Self {
        Self {
            alerts: HashMap::new(),
            next_id: 1,
        }
    }

    /// Create a new alert.
    pub fn create_alert(
        &mut self,
        title: &str,
        description: &str,
        priority: AlertPriority,
        source_ids: Vec<String>,
    ) -> &Alert {
        let id = format!("ALERT-{:04}", self.next_id);
        self.next_id += 1;

        let now = Utc::now();
        let alert = Alert {
            id: id.clone(),
            title: title.to_string(),
            description: description.to_string(),
            status: AlertStatus::New,
            priority,
            source_ids,
            created_at: now,
            updated_at: now,
            assignee: None,
            history: Vec::new(),
            tags: Vec::new(),
            metadata: HashMap::new(),
        };

        self.alerts.insert(id.clone(), alert);
        self.alerts.get(&id).unwrap()
    }

    /// Transition alert status.
    pub fn update_status(
        &mut self,
        alert_id: &str,
        new_status: AlertStatus,
        actor: &str,
        note: Option<&str>,
    ) -> Result<(), AlertError> {
        let alert = self
            .alerts
            .get_mut(alert_id)
            .ok_or_else(|| AlertError::NotFound(alert_id.to_string()))?;

        // Validate transition
        if !is_valid_transition(alert.status, new_status) {
            return Err(AlertError::InvalidTransition {
                from: alert.status,
                to: new_status,
            });
        }

        let entry = AlertHistoryEntry {
            timestamp: Utc::now(),
            from_status: alert.status,
            to_status: new_status,
            actor: actor.to_string(),
            note: note.map(|s| s.to_string()),
        };

        alert.history.push(entry);
        alert.status = new_status;
        alert.updated_at = Utc::now();

        Ok(())
    }

    /// Assign an alert.
    pub fn assign(&mut self, alert_id: &str, assignee: &str) -> Result<(), AlertError> {
        let alert = self
            .alerts
            .get_mut(alert_id)
            .ok_or_else(|| AlertError::NotFound(alert_id.to_string()))?;
        alert.assignee = Some(assignee.to_string());
        alert.updated_at = Utc::now();
        Ok(())
    }

    /// Get all alerts with a given status.
    pub fn by_status(&self, status: AlertStatus) -> Vec<&Alert> {
        self.alerts
            .values()
            .filter(|a| a.status == status)
            .collect()
    }

    /// Get all open alerts (not Resolved/Closed/FalsePositive).
    pub fn open_alerts(&self) -> Vec<&Alert> {
        self.alerts
            .values()
            .filter(|a| {
                !matches!(
                    a.status,
                    AlertStatus::Resolved | AlertStatus::Closed | AlertStatus::FalsePositive
                )
            })
            .collect()
    }

    /// Get alert by ID.
    pub fn get(&self, id: &str) -> Option<&Alert> {
        self.alerts.get(id)
    }

    /// Get total alert count.
    pub fn count(&self) -> usize {
        self.alerts.len()
    }

    /// Acknowledge an alert (shorthand for New -> Acknowledged).
    pub fn acknowledge(&mut self, alert_id: &str, actor: &str) -> Result<(), AlertError> {
        self.update_status(alert_id, AlertStatus::Acknowledged, actor, None)
    }

    /// Begin investigation on an alert (shorthand for Acknowledged -> Investigating).
    pub fn investigate(
        &mut self,
        alert_id: &str,
        actor: &str,
        note: Option<&str>,
    ) -> Result<(), AlertError> {
        self.update_status(alert_id, AlertStatus::Investigating, actor, note)
    }

    /// Resolve an alert (shorthand for Investigating -> Resolved).
    pub fn resolve(
        &mut self,
        alert_id: &str,
        actor: &str,
        note: Option<&str>,
    ) -> Result<(), AlertError> {
        self.update_status(alert_id, AlertStatus::Resolved, actor, note)
    }

    /// Close a resolved alert (shorthand for Resolved -> Closed).
    pub fn close(&mut self, alert_id: &str, actor: &str) -> Result<(), AlertError> {
        self.update_status(alert_id, AlertStatus::Closed, actor, None)
    }

    /// Mark an alert as false positive (from New, Acknowledged, or Investigating).
    pub fn mark_false_positive(
        &mut self,
        alert_id: &str,
        actor: &str,
        note: Option<&str>,
    ) -> Result<(), AlertError> {
        self.update_status(alert_id, AlertStatus::FalsePositive, actor, note)
    }

    /// Add a tag to an alert.
    pub fn add_tag(&mut self, alert_id: &str, tag: &str) -> Result<(), AlertError> {
        let alert = self
            .alerts
            .get_mut(alert_id)
            .ok_or_else(|| AlertError::NotFound(alert_id.to_string()))?;
        if !alert.tags.contains(&tag.to_string()) {
            alert.tags.push(tag.to_string());
        }
        Ok(())
    }

    /// Add metadata to an alert.
    pub fn set_metadata(
        &mut self,
        alert_id: &str,
        key: &str,
        value: &str,
    ) -> Result<(), AlertError> {
        let alert = self
            .alerts
            .get_mut(alert_id)
            .ok_or_else(|| AlertError::NotFound(alert_id.to_string()))?;
        alert.metadata.insert(key.to_string(), value.to_string());
        Ok(())
    }

    /// Correlate alerts by common source IDs.
    pub fn correlate_by_source(&self) -> Vec<CorrelationGroup> {
        let mut source_to_alerts: HashMap<&str, Vec<&str>> = HashMap::new();

        for alert in self.alerts.values() {
            for source_id in &alert.source_ids {
                source_to_alerts
                    .entry(source_id.as_str())
                    .or_default()
                    .push(alert.id.as_str());
            }
        }

        source_to_alerts
            .into_iter()
            .filter(|(_, alerts)| alerts.len() > 1)
            .enumerate()
            .map(|(idx, (source, alert_ids))| {
                let mut attrs = HashMap::new();
                attrs.insert("common_source".into(), source.to_string());

                CorrelationGroup {
                    group_id: format!("CG-{:04}", idx + 1),
                    alert_ids: alert_ids.into_iter().map(|s| s.to_string()).collect(),
                    reason: format!("Alerts share common source: {source}"),
                    common_attributes: attrs,
                }
            })
            .collect()
    }

    /// Get alert summary statistics.
    pub fn summary(&self) -> AlertSummary {
        let mut by_status: HashMap<String, usize> = HashMap::new();
        let mut by_priority: HashMap<String, usize> = HashMap::new();

        for alert in self.alerts.values() {
            *by_status.entry(alert.status.to_string()).or_insert(0) += 1;
            *by_priority
                .entry(format!("{:?}", alert.priority))
                .or_insert(0) += 1;
        }

        AlertSummary {
            total: self.alerts.len(),
            open: self.open_alerts().len(),
            by_status,
            by_priority,
        }
    }
}

impl Default for AlertManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Alert summary statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertSummary {
    pub total: usize,
    pub open: usize,
    pub by_status: HashMap<String, usize>,
    pub by_priority: HashMap<String, usize>,
}

/// Alert correlation engine that groups related alerts.
pub struct AlertCorrelationEngine {
    /// Time window for temporal correlation (in seconds).
    temporal_window_secs: i64,
}

impl AlertCorrelationEngine {
    /// Create a new correlation engine with the given temporal window.
    pub fn new(temporal_window_secs: i64) -> Self {
        Self {
            temporal_window_secs,
        }
    }

    /// Correlate alerts using multiple strategies: temporal proximity,
    /// common source IDs, and common tags.
    pub fn correlate(&self, alerts: &[&Alert]) -> Vec<CorrelationGroup> {
        let mut groups = Vec::new();
        let mut group_counter = 0usize;

        // 1. Temporal proximity: group alerts created within the time window
        let temporal = self.correlate_temporal(alerts, &mut group_counter);
        groups.extend(temporal);

        // 2. Common source IDs
        let source = self.correlate_by_common_source(alerts, &mut group_counter);
        groups.extend(source);

        // 3. Common tags (CWE, rule, category)
        let tag = self.correlate_by_common_tags(alerts, &mut group_counter);
        groups.extend(tag);

        groups
    }

    /// Group alerts that were created within the temporal window of each other.
    fn correlate_temporal(&self, alerts: &[&Alert], counter: &mut usize) -> Vec<CorrelationGroup> {
        let mut groups: Vec<CorrelationGroup> = Vec::new();
        let window = Duration::seconds(self.temporal_window_secs);

        // Sort by creation time
        let mut sorted: Vec<&&Alert> = alerts.iter().collect();
        sorted.sort_by_key(|a| a.created_at);

        let mut used: Vec<bool> = vec![false; sorted.len()];

        for i in 0..sorted.len() {
            if used[i] {
                continue;
            }
            let mut cluster = vec![sorted[i].id.clone()];
            used[i] = true;

            for j in (i + 1)..sorted.len() {
                if used[j] {
                    continue;
                }
                let diff = sorted[j].created_at - sorted[i].created_at;
                if diff <= window {
                    cluster.push(sorted[j].id.clone());
                    used[j] = true;
                }
            }

            if cluster.len() > 1 {
                *counter += 1;
                groups.push(CorrelationGroup {
                    group_id: format!("CG-TEMP-{counter:04}"),
                    alert_ids: cluster,
                    reason: format!(
                        "Alerts created within {}s of each other",
                        self.temporal_window_secs
                    ),
                    common_attributes: HashMap::from([(
                        "correlation_type".into(),
                        "temporal".into(),
                    )]),
                });
            }
        }

        groups
    }

    /// Group alerts that share common source IDs.
    fn correlate_by_common_source(
        &self,
        alerts: &[&Alert],
        counter: &mut usize,
    ) -> Vec<CorrelationGroup> {
        let mut source_to_alerts: HashMap<&str, Vec<&str>> = HashMap::new();

        for alert in alerts {
            for source_id in &alert.source_ids {
                source_to_alerts
                    .entry(source_id.as_str())
                    .or_default()
                    .push(alert.id.as_str());
            }
        }

        source_to_alerts
            .into_iter()
            .filter(|(_, alert_ids)| alert_ids.len() > 1)
            .map(|(source, alert_ids)| {
                *counter += 1;
                CorrelationGroup {
                    group_id: format!("CG-SRC-{counter:04}"),
                    alert_ids: alert_ids.into_iter().map(|s| s.to_string()).collect(),
                    reason: format!("Alerts share common source: {source}"),
                    common_attributes: HashMap::from([
                        ("correlation_type".into(), "source".into()),
                        ("common_source".into(), source.to_string()),
                    ]),
                }
            })
            .collect()
    }

    /// Group alerts that share common tags (e.g., CWE IDs, rule names).
    fn correlate_by_common_tags(
        &self,
        alerts: &[&Alert],
        counter: &mut usize,
    ) -> Vec<CorrelationGroup> {
        let mut tag_to_alerts: HashMap<&str, Vec<&str>> = HashMap::new();

        for alert in alerts {
            for tag in &alert.tags {
                tag_to_alerts
                    .entry(tag.as_str())
                    .or_default()
                    .push(alert.id.as_str());
            }
        }

        tag_to_alerts
            .into_iter()
            .filter(|(_, alert_ids)| alert_ids.len() > 1)
            .map(|(tag, alert_ids)| {
                *counter += 1;
                CorrelationGroup {
                    group_id: format!("CG-TAG-{counter:04}"),
                    alert_ids: alert_ids.into_iter().map(|s| s.to_string()).collect(),
                    reason: format!("Alerts share common tag: {tag}"),
                    common_attributes: HashMap::from([
                        ("correlation_type".into(), "tag".into()),
                        ("common_tag".into(), tag.to_string()),
                    ]),
                }
            })
            .collect()
    }

    /// Prioritize alerts by computing a numeric priority score.
    /// Score = severity_weight * recency_weight * blast_radius_weight.
    pub fn prioritize(&self, alerts: &mut [Alert]) {
        let now = Utc::now();

        alerts.sort_by(|a, b| {
            let score_a = Self::priority_score(a, now);
            let score_b = Self::priority_score(b, now);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Compute a priority score for a single alert.
    /// Higher score = higher priority.
    pub fn priority_score(alert: &Alert, now: DateTime<Utc>) -> f64 {
        let severity_weight = match alert.priority {
            AlertPriority::P0 => 10.0,
            AlertPriority::P1 => 8.0,
            AlertPriority::P2 => 5.0,
            AlertPriority::P3 => 3.0,
            AlertPriority::P4 => 1.0,
        };

        // Recency: alerts created more recently get higher weight
        let age_hours = (now - alert.created_at).num_minutes() as f64 / 60.0;
        let recency_weight = 1.0 / (1.0 + age_hours / 24.0);

        // Blast radius: number of source IDs as a proxy for scope
        let blast_radius = 1.0 + alert.source_ids.len() as f64 * 0.5;

        severity_weight * recency_weight * blast_radius
    }
}

impl Default for AlertCorrelationEngine {
    fn default() -> Self {
        // Default: 10 minute temporal window
        Self::new(600)
    }
}

/// Alert errors.
#[derive(Debug, thiserror::Error)]
pub enum AlertError {
    #[error("Alert not found: {0}")]
    NotFound(String),
    #[error("Invalid status transition from {from} to {to}")]
    InvalidTransition { from: AlertStatus, to: AlertStatus },
}

/// Check if a status transition is valid.
fn is_valid_transition(from: AlertStatus, to: AlertStatus) -> bool {
    matches!(
        (from, to),
        (AlertStatus::New, AlertStatus::Acknowledged)
            | (AlertStatus::New, AlertStatus::FalsePositive)
            | (AlertStatus::Acknowledged, AlertStatus::Investigating)
            | (AlertStatus::Acknowledged, AlertStatus::FalsePositive)
            | (AlertStatus::Investigating, AlertStatus::Resolved)
            | (AlertStatus::Investigating, AlertStatus::FalsePositive)
            | (AlertStatus::Resolved, AlertStatus::Closed)
            | (AlertStatus::Resolved, AlertStatus::Investigating) // reopen
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_alert() {
        let mut mgr = AlertManager::new();
        let alert = mgr.create_alert(
            "Test Alert",
            "Test description",
            AlertPriority::P2,
            vec!["finding-1".into()],
        );

        assert_eq!(alert.id, "ALERT-0001");
        assert_eq!(alert.status, AlertStatus::New);
        assert_eq!(alert.priority, AlertPriority::P2);
    }

    #[test]
    fn test_status_transitions() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("Test", "Desc", AlertPriority::P1, vec![]);

        // New -> Acknowledged
        assert!(
            mgr.update_status("ALERT-0001", AlertStatus::Acknowledged, "user1", None)
                .is_ok()
        );

        // Acknowledged -> Investigating
        assert!(
            mgr.update_status(
                "ALERT-0001",
                AlertStatus::Investigating,
                "user1",
                Some("Starting investigation")
            )
            .is_ok()
        );

        // Investigating -> Resolved
        assert!(
            mgr.update_status("ALERT-0001", AlertStatus::Resolved, "user1", None)
                .is_ok()
        );

        // Resolved -> Closed
        assert!(
            mgr.update_status("ALERT-0001", AlertStatus::Closed, "user1", None)
                .is_ok()
        );
    }

    #[test]
    fn test_invalid_transition() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("Test", "Desc", AlertPriority::P1, vec![]);

        // New -> Resolved is not valid (must go through Acknowledged/Investigating)
        let result = mgr.update_status("ALERT-0001", AlertStatus::Resolved, "user1", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_alerts() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("Alert 1", "Desc", AlertPriority::P1, vec![]);
        mgr.create_alert("Alert 2", "Desc", AlertPriority::P2, vec![]);
        mgr.create_alert("Alert 3", "Desc", AlertPriority::P3, vec![]);

        // Close one
        mgr.update_status("ALERT-0001", AlertStatus::Acknowledged, "user", None)
            .unwrap();
        mgr.update_status("ALERT-0001", AlertStatus::Investigating, "user", None)
            .unwrap();
        mgr.update_status("ALERT-0001", AlertStatus::Resolved, "user", None)
            .unwrap();
        mgr.update_status("ALERT-0001", AlertStatus::Closed, "user", None)
            .unwrap();

        assert_eq!(mgr.open_alerts().len(), 2);
    }

    #[test]
    fn test_alert_assignment() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("Test", "Desc", AlertPriority::P1, vec![]);

        mgr.assign("ALERT-0001", "security-team").unwrap();
        assert_eq!(
            mgr.get("ALERT-0001").unwrap().assignee.as_deref(),
            Some("security-team")
        );
    }

    #[test]
    fn test_alert_history() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("Test", "Desc", AlertPriority::P1, vec![]);

        mgr.update_status("ALERT-0001", AlertStatus::Acknowledged, "user1", None)
            .unwrap();
        mgr.update_status(
            "ALERT-0001",
            AlertStatus::Investigating,
            "user2",
            Some("Checking logs"),
        )
        .unwrap();

        let alert = mgr.get("ALERT-0001").unwrap();
        assert_eq!(alert.history.len(), 2);
        assert_eq!(alert.history[1].actor, "user2");
        assert_eq!(alert.history[1].note.as_deref(), Some("Checking logs"));
    }

    #[test]
    fn test_alert_summary() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("A1", "Desc", AlertPriority::P0, vec![]);
        mgr.create_alert("A2", "Desc", AlertPriority::P1, vec![]);
        mgr.create_alert("A3", "Desc", AlertPriority::P2, vec![]);

        let summary = mgr.summary();
        assert_eq!(summary.total, 3);
        assert_eq!(summary.open, 3);
    }

    #[test]
    fn test_priority_ordering() {
        assert!(AlertPriority::P4 < AlertPriority::P3);
        assert!(AlertPriority::P3 < AlertPriority::P2);
        assert!(AlertPriority::P2 < AlertPriority::P1);
        assert!(AlertPriority::P1 < AlertPriority::P0);
    }

    #[test]
    fn test_alert_status_display() {
        assert_eq!(AlertStatus::New.to_string(), "New");
        assert_eq!(AlertStatus::FalsePositive.to_string(), "False Positive");
    }

    #[test]
    fn test_false_positive_from_new() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("FP", "Not real", AlertPriority::P3, vec![]);

        assert!(
            mgr.update_status("ALERT-0001", AlertStatus::FalsePositive, "analyst", None)
                .is_ok()
        );
        assert!(!mgr.open_alerts().iter().any(|a| a.id == "ALERT-0001"));
    }

    #[test]
    fn test_convenience_lifecycle_methods() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("Test", "Desc", AlertPriority::P1, vec![]);

        mgr.acknowledge("ALERT-0001", "user1").unwrap();
        assert_eq!(
            mgr.get("ALERT-0001").unwrap().status,
            AlertStatus::Acknowledged
        );

        mgr.investigate("ALERT-0001", "user1", Some("Looking into it"))
            .unwrap();
        assert_eq!(
            mgr.get("ALERT-0001").unwrap().status,
            AlertStatus::Investigating
        );

        mgr.resolve("ALERT-0001", "user1", Some("Fixed")).unwrap();
        assert_eq!(mgr.get("ALERT-0001").unwrap().status, AlertStatus::Resolved);

        mgr.close("ALERT-0001", "user1").unwrap();
        assert_eq!(mgr.get("ALERT-0001").unwrap().status, AlertStatus::Closed);
    }

    #[test]
    fn test_mark_false_positive_convenience() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("FP", "Not real", AlertPriority::P4, vec![]);

        mgr.mark_false_positive("ALERT-0001", "analyst", Some("Not a real threat"))
            .unwrap();
        assert_eq!(
            mgr.get("ALERT-0001").unwrap().status,
            AlertStatus::FalsePositive
        );
    }

    #[test]
    fn test_add_tag_and_metadata() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("Test", "Desc", AlertPriority::P2, vec![]);

        mgr.add_tag("ALERT-0001", "cwe-79").unwrap();
        mgr.add_tag("ALERT-0001", "xss").unwrap();
        // Duplicate tag should not be added
        mgr.add_tag("ALERT-0001", "cwe-79").unwrap();
        assert_eq!(mgr.get("ALERT-0001").unwrap().tags.len(), 2);

        mgr.set_metadata("ALERT-0001", "component", "auth-service")
            .unwrap();
        assert_eq!(
            mgr.get("ALERT-0001")
                .unwrap()
                .metadata
                .get("component")
                .map(|s| s.as_str()),
            Some("auth-service")
        );
    }

    #[test]
    fn test_correlation_engine_temporal() {
        let engine = AlertCorrelationEngine::new(300); // 5 min window

        let now = Utc::now();
        let alerts = [
            Alert {
                id: "A1".into(),
                title: "Alert 1".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P1,
                source_ids: vec![],
                created_at: now,
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec![],
                metadata: HashMap::new(),
            },
            Alert {
                id: "A2".into(),
                title: "Alert 2".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P2,
                source_ids: vec![],
                created_at: now + Duration::seconds(60), // 1 min later
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec![],
                metadata: HashMap::new(),
            },
            Alert {
                id: "A3".into(),
                title: "Alert 3".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P3,
                source_ids: vec![],
                created_at: now + Duration::seconds(7200), // 2 hours later
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec![],
                metadata: HashMap::new(),
            },
        ];

        let alert_refs: Vec<&Alert> = alerts.iter().collect();
        let groups = engine.correlate(&alert_refs);

        // A1 and A2 should be temporally correlated, A3 should not
        let temporal: Vec<&CorrelationGroup> = groups
            .iter()
            .filter(|g| g.common_attributes.get("correlation_type") == Some(&"temporal".into()))
            .collect();
        assert_eq!(temporal.len(), 1);
        assert!(temporal[0].alert_ids.contains(&"A1".to_string()));
        assert!(temporal[0].alert_ids.contains(&"A2".to_string()));
        assert!(!temporal[0].alert_ids.contains(&"A3".to_string()));
    }

    #[test]
    fn test_correlation_engine_common_source() {
        let engine = AlertCorrelationEngine::default();
        let now = Utc::now();

        let alerts = [
            Alert {
                id: "A1".into(),
                title: "Alert 1".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P1,
                source_ids: vec!["finding-1".into(), "finding-2".into()],
                created_at: now,
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec![],
                metadata: HashMap::new(),
            },
            Alert {
                id: "A2".into(),
                title: "Alert 2".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P2,
                source_ids: vec!["finding-2".into(), "finding-3".into()],
                created_at: now,
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec![],
                metadata: HashMap::new(),
            },
        ];

        let alert_refs: Vec<&Alert> = alerts.iter().collect();
        let groups = engine.correlate(&alert_refs);

        let source_groups: Vec<&CorrelationGroup> = groups
            .iter()
            .filter(|g| g.common_attributes.get("correlation_type") == Some(&"source".into()))
            .collect();
        assert!(!source_groups.is_empty());
        assert!(
            source_groups[0]
                .common_attributes
                .contains_key("common_source")
        );
    }

    #[test]
    fn test_correlation_engine_common_tags() {
        let engine = AlertCorrelationEngine::default();
        let now = Utc::now();

        let alerts = [
            Alert {
                id: "A1".into(),
                title: "Alert 1".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P1,
                source_ids: vec![],
                created_at: now,
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec!["cwe-79".into(), "xss".into()],
                metadata: HashMap::new(),
            },
            Alert {
                id: "A2".into(),
                title: "Alert 2".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P2,
                source_ids: vec![],
                created_at: now,
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec!["cwe-79".into(), "injection".into()],
                metadata: HashMap::new(),
            },
        ];

        let alert_refs: Vec<&Alert> = alerts.iter().collect();
        let groups = engine.correlate(&alert_refs);

        let tag_groups: Vec<&CorrelationGroup> = groups
            .iter()
            .filter(|g| g.common_attributes.get("correlation_type") == Some(&"tag".into()))
            .collect();
        assert!(!tag_groups.is_empty());
        assert!(
            tag_groups
                .iter()
                .any(|g| g.common_attributes.get("common_tag") == Some(&"cwe-79".into()))
        );
    }

    #[test]
    fn test_prioritize_alerts() {
        let engine = AlertCorrelationEngine::default();
        let now = Utc::now();

        let mut alerts = vec![
            Alert {
                id: "LOW".into(),
                title: "Low".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P4,
                source_ids: vec![],
                created_at: now,
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec![],
                metadata: HashMap::new(),
            },
            Alert {
                id: "HIGH".into(),
                title: "High".into(),
                description: "".into(),
                status: AlertStatus::New,
                priority: AlertPriority::P0,
                source_ids: vec!["s1".into(), "s2".into()],
                created_at: now,
                updated_at: now,
                assignee: None,
                history: vec![],
                tags: vec![],
                metadata: HashMap::new(),
            },
        ];

        engine.prioritize(&mut alerts);

        // HIGH priority should come first
        assert_eq!(alerts[0].id, "HIGH");
        assert_eq!(alerts[1].id, "LOW");
    }

    #[test]
    fn test_priority_score() {
        let now = Utc::now();
        let alert_p0 = Alert {
            id: "P0".into(),
            title: "Critical".into(),
            description: "".into(),
            status: AlertStatus::New,
            priority: AlertPriority::P0,
            source_ids: vec!["s1".into()],
            created_at: now,
            updated_at: now,
            assignee: None,
            history: vec![],
            tags: vec![],
            metadata: HashMap::new(),
        };

        let alert_p4 = Alert {
            id: "P4".into(),
            title: "Low".into(),
            description: "".into(),
            status: AlertStatus::New,
            priority: AlertPriority::P4,
            source_ids: vec![],
            created_at: now,
            updated_at: now,
            assignee: None,
            history: vec![],
            tags: vec![],
            metadata: HashMap::new(),
        };

        let score_p0 = AlertCorrelationEngine::priority_score(&alert_p0, now);
        let score_p4 = AlertCorrelationEngine::priority_score(&alert_p4, now);

        assert!(score_p0 > score_p4);
    }

    #[test]
    fn test_correlate_by_source_on_manager() {
        let mut mgr = AlertManager::new();
        mgr.create_alert("A1", "D1", AlertPriority::P1, vec!["src-1".into()]);
        mgr.create_alert("A2", "D2", AlertPriority::P2, vec!["src-1".into()]);
        mgr.create_alert("A3", "D3", AlertPriority::P3, vec!["src-2".into()]);

        let groups = mgr.correlate_by_source();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].alert_ids.len(), 2);
    }
}
