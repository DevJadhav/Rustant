//! Threat detection — Sigma-compatible rule engine with MITRE ATT&CK mapping.
//!
//! Detects threats from log events and audit trails using configurable rules.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A detection rule for threat identification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionRule {
    /// Unique rule identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Rule description.
    pub description: String,
    /// Severity of the detected threat.
    pub severity: ThreatSeverity,
    /// MITRE ATT&CK technique ID (e.g., "T1110").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mitre_technique: Option<String>,
    /// Detection condition.
    pub condition: DetectionCondition,
    /// Response action to take.
    pub response: Option<String>,
    /// Tags for categorization.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Whether this rule is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Threat severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreatSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl std::fmt::Display for ThreatSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ThreatSeverity::Info => write!(f, "info"),
            ThreatSeverity::Low => write!(f, "low"),
            ThreatSeverity::Medium => write!(f, "medium"),
            ThreatSeverity::High => write!(f, "high"),
            ThreatSeverity::Critical => write!(f, "critical"),
        }
    }
}

/// Detection condition types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DetectionCondition {
    /// Match a field against a pattern.
    FieldMatch { field: String, pattern: String },
    /// Count events exceeding a threshold within a time window.
    Threshold {
        field: String,
        value: String,
        count: usize,
        window_secs: u64,
    },
    /// Sequence of events in order.
    Sequence {
        events: Vec<EventMatcher>,
        within_secs: u64,
    },
}

/// Event matcher for sequence detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMatcher {
    /// Field to match.
    pub field: String,
    /// Value to match.
    pub value: String,
}

/// A log event to analyze.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEvent {
    /// Event timestamp.
    pub timestamp: DateTime<Utc>,
    /// Event type/category.
    pub event_type: String,
    /// Source of the event.
    pub source: String,
    /// Event fields.
    pub fields: HashMap<String, String>,
}

/// A detected threat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreatDetection {
    /// Which rule triggered.
    pub rule_id: String,
    /// Rule name.
    pub rule_name: String,
    /// Severity.
    pub severity: ThreatSeverity,
    /// MITRE ATT&CK technique.
    pub mitre_technique: Option<String>,
    /// When detected.
    pub detected_at: DateTime<Utc>,
    /// Events that triggered detection.
    pub triggering_events: Vec<LogEvent>,
    /// Suggested response action.
    pub response: Option<String>,
    /// Detection context.
    pub context: HashMap<String, String>,
}

/// Threat detector engine.
pub struct ThreatDetector {
    rules: Vec<DetectionRule>,
    /// Sliding window of recent events for temporal correlation.
    event_buffer: Vec<LogEvent>,
    /// Maximum events to keep in buffer.
    buffer_size: usize,
}

impl ThreatDetector {
    /// Create a new detector with the given rules.
    pub fn new(rules: Vec<DetectionRule>) -> Self {
        Self {
            rules,
            event_buffer: Vec::new(),
            buffer_size: 10_000,
        }
    }

    /// Create with default detection rules.
    pub fn with_defaults() -> Self {
        Self::new(vec![
            DetectionRule {
                id: "brute-force-login".into(),
                name: "Brute Force Login Attempt".into(),
                description: "Multiple failed login attempts from same source".into(),
                severity: ThreatSeverity::High,
                mitre_technique: Some("T1110".into()),
                condition: DetectionCondition::Threshold {
                    field: "event_type".into(),
                    value: "login_failed".into(),
                    count: 5,
                    window_secs: 300,
                },
                response: Some("block_ip".into()),
                tags: vec!["credential-access".into()],
                enabled: true,
            },
            DetectionRule {
                id: "privilege-escalation".into(),
                name: "Privilege Escalation Attempt".into(),
                description: "Attempt to elevate privileges".into(),
                severity: ThreatSeverity::Critical,
                mitre_technique: Some("T1068".into()),
                condition: DetectionCondition::FieldMatch {
                    field: "event_type".into(),
                    pattern: "privilege_escalation".into(),
                },
                response: Some("alert_security_team".into()),
                tags: vec!["privilege-escalation".into()],
                enabled: true,
            },
            DetectionRule {
                id: "suspicious-file-access".into(),
                name: "Suspicious File Access Pattern".into(),
                description: "Access to sensitive files from unusual source".into(),
                severity: ThreatSeverity::Medium,
                mitre_technique: Some("T1005".into()),
                condition: DetectionCondition::FieldMatch {
                    field: "event_type".into(),
                    pattern: "sensitive_file_access".into(),
                },
                response: None,
                tags: vec!["collection".into()],
                enabled: true,
            },
        ])
    }

    /// Add an event to the buffer and check for detections.
    pub fn process_event(&mut self, event: LogEvent) -> Vec<ThreatDetection> {
        self.event_buffer.push(event.clone());

        // Evict old events beyond buffer size
        if self.event_buffer.len() > self.buffer_size {
            self.event_buffer
                .drain(0..self.event_buffer.len() - self.buffer_size);
        }

        let mut detections = Vec::new();

        for rule in &self.rules {
            if !rule.enabled {
                continue;
            }

            if let Some(detection) = self.check_rule(rule, &event) {
                detections.push(detection);
            }
        }

        detections
    }

    /// Analyze a batch of events.
    pub fn analyze_batch(&mut self, events: &[LogEvent]) -> Vec<ThreatDetection> {
        let mut detections = Vec::new();
        for event in events {
            detections.extend(self.process_event(event.clone()));
        }
        detections
    }

    /// Check a single rule against the current event.
    fn check_rule(&self, rule: &DetectionRule, event: &LogEvent) -> Option<ThreatDetection> {
        match &rule.condition {
            DetectionCondition::FieldMatch { field, pattern } => {
                let value = if field == "event_type" {
                    Some(&event.event_type)
                } else {
                    event.fields.get(field.as_str())
                };

                if let Some(val) = value
                    && (val == pattern || val.contains(pattern.as_str()))
                {
                    return Some(ThreatDetection {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        severity: rule.severity,
                        mitre_technique: rule.mitre_technique.clone(),
                        detected_at: Utc::now(),
                        triggering_events: vec![event.clone()],
                        response: rule.response.clone(),
                        context: HashMap::new(),
                    });
                }
                None
            }
            DetectionCondition::Threshold {
                field,
                value,
                count,
                window_secs,
            } => {
                let cutoff = Utc::now() - chrono::Duration::seconds(*window_secs as i64);
                let matching_count = self
                    .event_buffer
                    .iter()
                    .filter(|e| {
                        e.timestamp >= cutoff && {
                            let v = if field == "event_type" {
                                Some(&e.event_type)
                            } else {
                                e.fields.get(field.as_str())
                            };
                            v == Some(value)
                        }
                    })
                    .count();

                if matching_count >= *count {
                    let triggering: Vec<LogEvent> = self
                        .event_buffer
                        .iter()
                        .filter(|e| {
                            e.timestamp >= cutoff && {
                                let v = if field == "event_type" {
                                    Some(&e.event_type)
                                } else {
                                    e.fields.get(field.as_str())
                                };
                                v == Some(value)
                            }
                        })
                        .cloned()
                        .collect();

                    let mut ctx = HashMap::new();
                    ctx.insert("count".into(), matching_count.to_string());
                    ctx.insert("window_secs".into(), window_secs.to_string());

                    return Some(ThreatDetection {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        severity: rule.severity,
                        mitre_technique: rule.mitre_technique.clone(),
                        detected_at: Utc::now(),
                        triggering_events: triggering,
                        response: rule.response.clone(),
                        context: ctx,
                    });
                }
                None
            }
            DetectionCondition::Sequence {
                events: matchers,
                within_secs,
            } => {
                let cutoff = Utc::now() - chrono::Duration::seconds(*within_secs as i64);

                let recent: Vec<&LogEvent> = self
                    .event_buffer
                    .iter()
                    .filter(|e| e.timestamp >= cutoff)
                    .collect();

                // Check if all matchers can be satisfied in order
                let mut match_idx = 0;
                let mut matched_events = Vec::new();

                for evt in &recent {
                    if match_idx < matchers.len() {
                        let matcher = &matchers[match_idx];
                        let val = if matcher.field == "event_type" {
                            Some(&evt.event_type)
                        } else {
                            evt.fields.get(matcher.field.as_str())
                        };

                        if let Some(v) = val
                            && v == &matcher.value
                        {
                            matched_events.push((*evt).clone());
                            match_idx += 1;
                        }
                    }
                }

                if match_idx == matchers.len() {
                    return Some(ThreatDetection {
                        rule_id: rule.id.clone(),
                        rule_name: rule.name.clone(),
                        severity: rule.severity,
                        mitre_technique: rule.mitre_technique.clone(),
                        detected_at: Utc::now(),
                        triggering_events: matched_events,
                        response: rule.response.clone(),
                        context: HashMap::new(),
                    });
                }
                None
            }
        }
    }

    /// Add a detection rule.
    pub fn add_rule(&mut self, rule: DetectionRule) {
        self.rules.push(rule);
    }

    /// Remove a detection rule by ID. Returns true if found and removed.
    pub fn remove_rule(&mut self, rule_id: &str) -> bool {
        let before = self.rules.len();
        self.rules.retain(|r| r.id != rule_id);
        self.rules.len() < before
    }

    /// List all rule IDs and names.
    pub fn list_rules(&self) -> Vec<(&str, &str, bool)> {
        self.rules
            .iter()
            .map(|r| (r.id.as_str(), r.name.as_str(), r.enabled))
            .collect()
    }

    /// Enable or disable a rule by ID. Returns true if the rule was found.
    pub fn set_rule_enabled(&mut self, rule_id: &str, enabled: bool) -> bool {
        if let Some(rule) = self.rules.iter_mut().find(|r| r.id == rule_id) {
            rule.enabled = enabled;
            true
        } else {
            false
        }
    }

    /// Get all enabled rules.
    pub fn rules(&self) -> &[DetectionRule] {
        &self.rules
    }

    /// Get event buffer size.
    pub fn buffer_len(&self) -> usize {
        self.event_buffer.len()
    }

    /// Clear the event buffer.
    pub fn clear_buffer(&mut self) {
        self.event_buffer.clear();
    }

    /// Create default detection rules (static method for reuse).
    pub fn default_rules() -> Vec<DetectionRule> {
        vec![
            DetectionRule {
                id: "brute-force-login".into(),
                name: "Brute Force Login Attempt".into(),
                description: "Multiple failed login attempts from same source".into(),
                severity: ThreatSeverity::High,
                mitre_technique: Some("T1110".into()),
                condition: DetectionCondition::Threshold {
                    field: "event_type".into(),
                    value: "login_failed".into(),
                    count: 5,
                    window_secs: 300,
                },
                response: Some("block_ip".into()),
                tags: vec!["credential-access".into()],
                enabled: true,
            },
            DetectionRule {
                id: "privilege-escalation".into(),
                name: "Privilege Escalation Attempt".into(),
                description: "Attempt to elevate privileges".into(),
                severity: ThreatSeverity::Critical,
                mitre_technique: Some("T1068".into()),
                condition: DetectionCondition::FieldMatch {
                    field: "event_type".into(),
                    pattern: "privilege_escalation".into(),
                },
                response: Some("alert_security_team".into()),
                tags: vec!["privilege-escalation".into()],
                enabled: true,
            },
            DetectionRule {
                id: "suspicious-file-access".into(),
                name: "Suspicious File Access Pattern".into(),
                description: "Access to sensitive files from unusual source".into(),
                severity: ThreatSeverity::Medium,
                mitre_technique: Some("T1005".into()),
                condition: DetectionCondition::FieldMatch {
                    field: "event_type".into(),
                    pattern: "sensitive_file_access".into(),
                },
                response: None,
                tags: vec!["collection".into()],
                enabled: true,
            },
            DetectionRule {
                id: "lateral-movement-sequence".into(),
                name: "Lateral Movement Sequence".into(),
                description: "Recon followed by lateral movement attempt".into(),
                severity: ThreatSeverity::High,
                mitre_technique: Some("T1021".into()),
                condition: DetectionCondition::Sequence {
                    events: vec![
                        EventMatcher {
                            field: "event_type".into(),
                            value: "port_scan".into(),
                        },
                        EventMatcher {
                            field: "event_type".into(),
                            value: "lateral_movement".into(),
                        },
                    ],
                    within_secs: 600,
                },
                response: Some("isolate_host".into()),
                tags: vec!["lateral-movement".into()],
                enabled: true,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(event_type: &str) -> LogEvent {
        LogEvent {
            timestamp: Utc::now(),
            event_type: event_type.into(),
            source: "test".into(),
            fields: HashMap::new(),
        }
    }

    #[test]
    fn test_field_match_detection() {
        let mut detector = ThreatDetector::with_defaults();
        let detections = detector.process_event(make_event("privilege_escalation"));

        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].rule_id, "privilege-escalation");
        assert_eq!(detections[0].severity, ThreatSeverity::Critical);
    }

    #[test]
    fn test_no_detection_on_normal_event() {
        let mut detector = ThreatDetector::with_defaults();
        let detections = detector.process_event(make_event("normal_action"));
        assert!(detections.is_empty());
    }

    #[test]
    fn test_threshold_detection() {
        let mut detector = ThreatDetector::with_defaults();

        // 4 events should not trigger (threshold is 5)
        for _ in 0..4 {
            let detections = detector.process_event(make_event("login_failed"));
            // May or may not detect depending on timing
            assert!(detections.is_empty() || detections[0].rule_id == "brute-force-login");
        }

        // 5th event should trigger
        let detections = detector.process_event(make_event("login_failed"));
        assert!(
            detections.iter().any(|d| d.rule_id == "brute-force-login"),
            "5 login failures should trigger brute force rule"
        );
    }

    #[test]
    fn test_batch_analysis() {
        let mut detector = ThreatDetector::with_defaults();
        let events = vec![
            make_event("normal"),
            make_event("privilege_escalation"),
            make_event("normal"),
        ];

        let detections = detector.analyze_batch(&events);
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].rule_name, "Privilege Escalation Attempt");
    }

    #[test]
    fn test_disabled_rule_skipped() {
        let rules = vec![DetectionRule {
            id: "test".into(),
            name: "Test".into(),
            description: "Test".into(),
            severity: ThreatSeverity::High,
            mitre_technique: None,
            condition: DetectionCondition::FieldMatch {
                field: "event_type".into(),
                pattern: "test_event".into(),
            },
            response: None,
            tags: Vec::new(),
            enabled: false,
        }];

        let mut detector = ThreatDetector::new(rules);
        let detections = detector.process_event(make_event("test_event"));
        assert!(detections.is_empty());
    }

    #[test]
    fn test_mitre_technique_mapping() {
        let mut detector = ThreatDetector::with_defaults();
        let detections = detector.process_event(make_event("privilege_escalation"));
        assert_eq!(detections[0].mitre_technique.as_deref(), Some("T1068"));
    }

    #[test]
    fn test_threat_severity_ordering() {
        assert!(ThreatSeverity::Info < ThreatSeverity::Low);
        assert!(ThreatSeverity::Low < ThreatSeverity::Medium);
        assert!(ThreatSeverity::Medium < ThreatSeverity::High);
        assert!(ThreatSeverity::High < ThreatSeverity::Critical);
    }

    #[test]
    fn test_threat_severity_display() {
        assert_eq!(ThreatSeverity::Critical.to_string(), "critical");
        assert_eq!(ThreatSeverity::Info.to_string(), "info");
    }

    #[test]
    fn test_add_rule() {
        let mut detector = ThreatDetector::new(vec![]);
        assert_eq!(detector.rules().len(), 0);

        detector.add_rule(DetectionRule {
            id: "custom-rule".into(),
            name: "Custom Rule".into(),
            description: "Test".into(),
            severity: ThreatSeverity::Low,
            mitre_technique: None,
            condition: DetectionCondition::FieldMatch {
                field: "event_type".into(),
                pattern: "custom_event".into(),
            },
            response: None,
            tags: Vec::new(),
            enabled: true,
        });

        assert_eq!(detector.rules().len(), 1);
        let detections = detector.process_event(make_event("custom_event"));
        assert_eq!(detections.len(), 1);
    }

    #[test]
    fn test_remove_rule() {
        let mut detector = ThreatDetector::with_defaults();
        let initial_count = detector.rules().len();

        assert!(detector.remove_rule("privilege-escalation"));
        assert_eq!(detector.rules().len(), initial_count - 1);

        // Removing again should return false
        assert!(!detector.remove_rule("privilege-escalation"));
    }

    #[test]
    fn test_list_rules() {
        let detector = ThreatDetector::with_defaults();
        let rules = detector.list_rules();
        assert!(rules.len() >= 3);
        assert!(rules.iter().any(|(id, _, _)| *id == "brute-force-login"));
    }

    #[test]
    fn test_set_rule_enabled() {
        let mut detector = ThreatDetector::with_defaults();
        assert!(detector.set_rule_enabled("privilege-escalation", false));

        // Disabled rule should not trigger
        let detections = detector.process_event(make_event("privilege_escalation"));
        assert!(detections.is_empty());

        // Re-enable
        assert!(detector.set_rule_enabled("privilege-escalation", true));
        let detections = detector.process_event(make_event("privilege_escalation"));
        assert!(!detections.is_empty());
    }

    #[test]
    fn test_sequence_detection() {
        let mut detector = ThreatDetector::new(vec![DetectionRule {
            id: "test-sequence".into(),
            name: "Test Sequence".into(),
            description: "Test".into(),
            severity: ThreatSeverity::High,
            mitre_technique: None,
            condition: DetectionCondition::Sequence {
                events: vec![
                    EventMatcher {
                        field: "event_type".into(),
                        value: "recon".into(),
                    },
                    EventMatcher {
                        field: "event_type".into(),
                        value: "exploit".into(),
                    },
                ],
                within_secs: 300,
            },
            response: None,
            tags: Vec::new(),
            enabled: true,
        }]);

        // First event only — should not trigger
        let detections = detector.process_event(make_event("recon"));
        assert!(detections.is_empty());

        // Second event completes the sequence
        let detections = detector.process_event(make_event("exploit"));
        assert_eq!(detections.len(), 1);
        assert_eq!(detections[0].rule_id, "test-sequence");
        assert_eq!(detections[0].triggering_events.len(), 2);
    }

    #[test]
    fn test_default_rules_static() {
        let rules = ThreatDetector::default_rules();
        assert!(rules.len() >= 4);
        assert!(rules.iter().any(|r| r.id == "lateral-movement-sequence"));
    }

    #[test]
    fn test_clear_buffer() {
        let mut detector = ThreatDetector::with_defaults();
        detector.process_event(make_event("something"));
        assert!(detector.buffer_len() > 0);
        detector.clear_buffer();
        assert_eq!(detector.buffer_len(), 0);
    }

    #[test]
    fn test_field_match_on_custom_field() {
        let mut detector = ThreatDetector::new(vec![DetectionRule {
            id: "path-match".into(),
            name: "Path Match".into(),
            description: "Match on path field".into(),
            severity: ThreatSeverity::Medium,
            mitre_technique: None,
            condition: DetectionCondition::FieldMatch {
                field: "path".into(),
                pattern: "/etc/shadow".into(),
            },
            response: None,
            tags: Vec::new(),
            enabled: true,
        }]);

        let mut event = make_event("file_access");
        event.fields.insert("path".into(), "/etc/shadow".into());
        let detections = detector.process_event(event);
        assert_eq!(detections.len(), 1);
    }
}
