//! Log parsers — structured parsing for common log formats.
//!
//! Parses syslog, JSON structured logs, Apache/Nginx access logs,
//! AWS CloudTrail, and Kubernetes audit logs into unified LogEvent format.

use crate::incident::detector::LogEvent;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Supported log format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogFormat {
    /// Syslog (RFC 3164/5424).
    Syslog,
    /// JSON structured logs.
    JsonStructured,
    /// Apache/Nginx combined access log.
    HttpAccess,
    /// AWS CloudTrail events.
    CloudTrail,
    /// Kubernetes audit log.
    KubernetesAudit,
    /// Generic key=value format.
    KeyValue,
}

impl std::fmt::Display for LogFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogFormat::Syslog => write!(f, "Syslog"),
            LogFormat::JsonStructured => write!(f, "JSON"),
            LogFormat::HttpAccess => write!(f, "HTTP Access"),
            LogFormat::CloudTrail => write!(f, "CloudTrail"),
            LogFormat::KubernetesAudit => write!(f, "K8s Audit"),
            LogFormat::KeyValue => write!(f, "Key-Value"),
        }
    }
}

/// Log parser that converts raw log lines into LogEvent structs.
pub struct LogParser;

impl LogParser {
    /// Auto-detect format and parse a log line.
    pub fn parse_line(line: &str) -> Option<LogEvent> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Try JSON first (including K8s audit)
        if trimmed.starts_with('{') {
            // Check for K8s audit format
            if (trimmed.contains("\"apiVersion\"") || trimmed.contains("\"auditID\""))
                && let Some(event) = Self::parse_k8s_audit(trimmed)
            {
                return Some(event);
            }

            if let Some(event) = Self::parse_json(trimmed) {
                return Some(event);
            }
        }

        // Try syslog
        if let Some(event) = Self::parse_syslog(trimmed) {
            return Some(event);
        }

        // Try HTTP access log
        if let Some(event) = Self::parse_http_access(trimmed) {
            return Some(event);
        }

        // Try key=value
        if trimmed.contains('=')
            && let Some(event) = Self::parse_key_value(trimmed)
        {
            return Some(event);
        }

        // Fallback: raw log line
        Some(LogEvent {
            timestamp: Utc::now(),
            event_type: "raw".to_string(),
            source: "unknown".to_string(),
            fields: HashMap::from([("message".to_string(), trimmed.to_string())]),
        })
    }

    /// Parse a batch of log lines.
    pub fn parse_batch(content: &str) -> Vec<LogEvent> {
        content.lines().filter_map(Self::parse_line).collect()
    }

    /// Parse JSON structured log.
    pub fn parse_json(line: &str) -> Option<LogEvent> {
        let value: serde_json::Value = serde_json::from_str(line).ok()?;
        let obj = value.as_object()?;

        let timestamp = obj
            .get("timestamp")
            .or_else(|| obj.get("time"))
            .or_else(|| obj.get("@timestamp"))
            .or_else(|| obj.get("ts"))
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let event_type = obj
            .get("event_type")
            .or_else(|| obj.get("type"))
            .or_else(|| obj.get("eventType"))
            .or_else(|| obj.get("level"))
            .and_then(|v| v.as_str())
            .unwrap_or("log")
            .to_string();

        let source = obj
            .get("source")
            .or_else(|| obj.get("service"))
            .or_else(|| obj.get("logger"))
            .and_then(|v| v.as_str())
            .unwrap_or("json")
            .to_string();

        let mut fields = HashMap::new();
        for (key, val) in obj {
            if !matches!(key.as_str(), "timestamp" | "time" | "@timestamp" | "ts") {
                fields.insert(
                    key.clone(),
                    match val {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    },
                );
            }
        }

        Some(LogEvent {
            timestamp,
            event_type,
            source,
            fields,
        })
    }

    /// Parse syslog format (RFC 3164 simplified).
    /// Format: <priority>Mon DD HH:MM:SS hostname process[pid]: message
    pub fn parse_syslog(line: &str) -> Option<LogEvent> {
        // Match pattern: "Mon DD HH:MM:SS" at start (optionally after <priority>)
        let content = if line.starts_with('<') {
            line.find('>').map(|i| &line[i + 1..]).unwrap_or(line)
        } else {
            line
        };

        // Try to parse syslog timestamp: "Jan  1 12:00:00" or "Jan 01 12:00:00"
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];

        let first_word = content.split_whitespace().next()?;
        if !months.contains(&first_word) {
            return None;
        }

        // Use split_whitespace to handle multiple spaces (e.g., "Jan  5")
        let tokens: Vec<&str> = content.split_whitespace().collect();
        if tokens.len() < 4 {
            return None;
        }

        // Parse timestamp (month day time)
        let timestamp_str = format!("{} {} {}", tokens[0], tokens[1], tokens[2]);
        let timestamp =
            NaiveDateTime::parse_from_str(&format!("2024 {timestamp_str}"), "%Y %b %d %H:%M:%S")
                .ok()
                .map(|ndt| ndt.and_utc())
                .unwrap_or_else(Utc::now);

        // Remainder is everything after the time token
        let time_token = tokens[2];
        let time_pos = content.find(time_token).unwrap_or(0) + time_token.len();
        let remainder = content[time_pos..].trim_start();

        // Extract hostname and process
        let (source, message) = if let Some(colon_pos) = remainder.find(": ") {
            let header = &remainder[..colon_pos];
            let msg = &remainder[colon_pos + 2..];
            let hostname = header.split_whitespace().next().unwrap_or("unknown");
            (hostname.to_string(), msg.to_string())
        } else {
            ("syslog".to_string(), remainder.to_string())
        };

        let msg_lower = message.to_lowercase();
        let event_type = if msg_lower.contains("error") {
            "error"
        } else if msg_lower.contains("warn") {
            "warning"
        } else if msg_lower.contains("fail") || msg_lower.contains("denied") {
            "auth_failure"
        } else {
            "syslog"
        };

        let mut fields = HashMap::new();
        fields.insert("message".to_string(), message);

        Some(LogEvent {
            timestamp,
            event_type: event_type.to_string(),
            source,
            fields,
        })
    }

    /// Parse HTTP access log (combined format).
    /// Format: IP - - [DD/Mon/YYYY:HH:MM:SS +0000] "METHOD /path HTTP/1.1" status size
    pub fn parse_http_access(line: &str) -> Option<LogEvent> {
        // Look for the characteristic [timestamp] pattern
        let bracket_start = line.find('[')?;
        let bracket_end = line.find(']')?;

        if bracket_start >= bracket_end {
            return None;
        }

        let ip = line[..bracket_start].split_whitespace().next()?;
        let timestamp_str = &line[bracket_start + 1..bracket_end];

        let timestamp = NaiveDateTime::parse_from_str(timestamp_str, "%d/%b/%Y:%H:%M:%S %z")
            .ok()
            .map(|ndt| ndt.and_utc())
            .unwrap_or_else(Utc::now);

        let after_bracket = &line[bracket_end + 1..].trim();

        // Parse request line
        let (method, path, status) = if let Some(stripped) = after_bracket.strip_prefix('"') {
            let end_quote = stripped.find('"')?;
            let request = &stripped[..end_quote];
            let rest = stripped[end_quote + 1..].trim();
            let status = rest.split_whitespace().next().unwrap_or("0");

            let parts: Vec<&str> = request.splitn(3, ' ').collect();
            (
                parts.first().unwrap_or(&"GET").to_string(),
                parts.get(1).unwrap_or(&"/").to_string(),
                status.to_string(),
            )
        } else {
            return None;
        };

        let status_code: u16 = status.parse().unwrap_or(0);
        let event_type = if status_code >= 500 {
            "server_error"
        } else if status_code == 401 || status_code == 403 {
            "auth_failure"
        } else if status_code >= 400 {
            "client_error"
        } else {
            "http_request"
        };

        let mut fields = HashMap::new();
        fields.insert("source_ip".to_string(), ip.to_string());
        fields.insert("method".to_string(), method);
        fields.insert("path".to_string(), path);
        fields.insert("status".to_string(), status);

        Some(LogEvent {
            timestamp,
            event_type: event_type.to_string(),
            source: "http".to_string(),
            fields,
        })
    }

    /// Parse CloudTrail JSON event.
    pub fn parse_cloudtrail(json_str: &str) -> Vec<LogEvent> {
        let value: serde_json::Value = match serde_json::from_str(json_str) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };

        let records = value
            .get("Records")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        records
            .iter()
            .filter_map(|record| {
                let event_name = record.get("eventName")?.as_str()?;
                let event_source = record.get("eventSource")?.as_str()?;
                let event_time = record
                    .get("eventTime")
                    .and_then(|v| v.as_str())
                    .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(Utc::now);

                let mut fields = HashMap::new();
                fields.insert("event_name".to_string(), event_name.to_string());
                if let Some(region) = record.get("awsRegion").and_then(|v| v.as_str()) {
                    fields.insert("region".to_string(), region.to_string());
                }
                if let Some(source_ip) = record.get("sourceIPAddress").and_then(|v| v.as_str()) {
                    fields.insert("source_ip".to_string(), source_ip.to_string());
                }
                if let Some(user_agent) = record.get("userAgent").and_then(|v| v.as_str()) {
                    fields.insert("user_agent".to_string(), user_agent.to_string());
                }

                let event_type = if event_name.starts_with("Unauthorized")
                    || event_name.contains("AccessDenied")
                {
                    "auth_failure"
                } else if event_name.contains("Delete") {
                    "destructive_action"
                } else {
                    "api_call"
                };

                Some(LogEvent {
                    timestamp: event_time,
                    event_type: event_type.to_string(),
                    source: event_source.to_string(),
                    fields,
                })
            })
            .collect()
    }

    /// Parse Kubernetes audit log JSON.
    pub fn parse_k8s_audit(json_str: &str) -> Option<LogEvent> {
        let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
        let obj = value.as_object()?;

        // K8s audit logs have "apiVersion" and/or "auditID"
        if obj.get("apiVersion").is_none() && obj.get("auditID").is_none() {
            return None;
        }

        let timestamp = obj
            .get("requestReceivedTimestamp")
            .or_else(|| obj.get("stageTimestamp"))
            .or_else(|| obj.get("timestamp"))
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|dt| dt.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);

        let verb = obj
            .get("verb")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let mut fields = HashMap::new();
        fields.insert("verb".to_string(), verb.to_string());

        if let Some(audit_id) = obj.get("auditID").and_then(|v| v.as_str()) {
            fields.insert("audit_id".to_string(), audit_id.to_string());
        }
        if let Some(stage) = obj.get("stage").and_then(|v| v.as_str()) {
            fields.insert("stage".to_string(), stage.to_string());
        }

        // Extract object reference (resource info)
        if let Some(object_ref) = obj.get("objectRef").and_then(|v| v.as_object()) {
            if let Some(resource) = object_ref.get("resource").and_then(|v| v.as_str()) {
                fields.insert("resource".to_string(), resource.to_string());
            }
            if let Some(namespace) = object_ref.get("namespace").and_then(|v| v.as_str()) {
                fields.insert("namespace".to_string(), namespace.to_string());
            }
            if let Some(name) = object_ref.get("name").and_then(|v| v.as_str()) {
                fields.insert("resource_name".to_string(), name.to_string());
            }
        }

        // Extract user info
        if let Some(user) = obj.get("user").and_then(|v| v.as_object()) {
            if let Some(username) = user.get("username").and_then(|v| v.as_str()) {
                fields.insert("username".to_string(), username.to_string());
            }
            if let Some(groups) = user.get("groups").and_then(|v| v.as_array()) {
                let group_names: Vec<String> = groups
                    .iter()
                    .filter_map(|g| g.as_str().map(|s| s.to_string()))
                    .collect();
                fields.insert("groups".to_string(), group_names.join(","));
            }
        }

        // Extract source IPs
        if let Some(source_ips) = obj.get("sourceIPs").and_then(|v| v.as_array()) {
            let ips: Vec<String> = source_ips
                .iter()
                .filter_map(|ip| ip.as_str().map(|s| s.to_string()))
                .collect();
            if !ips.is_empty() {
                fields.insert("source_ip".to_string(), ips.join(","));
            }
        }

        // Classify event type based on verb and response code
        let response_code = obj
            .get("responseStatus")
            .and_then(|r| r.get("code"))
            .and_then(|c| c.as_u64())
            .unwrap_or(200);

        let event_type = if response_code == 401 || response_code == 403 {
            "auth_failure"
        } else if verb == "delete" {
            "destructive_action"
        } else if verb == "create" || verb == "update" || verb == "patch" {
            "k8s_mutation"
        } else {
            "k8s_audit"
        };

        let source = fields
            .get("resource")
            .cloned()
            .unwrap_or_else(|| "k8s".to_string());

        Some(LogEvent {
            timestamp,
            event_type: event_type.to_string(),
            source,
            fields,
        })
    }

    /// Parse key=value format logs.
    pub fn parse_key_value(line: &str) -> Option<LogEvent> {
        let mut fields = HashMap::new();

        for part in line.split_whitespace() {
            if let Some(eq_pos) = part.find('=') {
                let key = &part[..eq_pos];
                let value = part[eq_pos + 1..].trim_matches('"');
                fields.insert(key.to_string(), value.to_string());
            }
        }

        if fields.is_empty() {
            return None;
        }

        let event_type = fields
            .get("event")
            .or_else(|| fields.get("type"))
            .or_else(|| fields.get("level"))
            .cloned()
            .unwrap_or_else(|| "log".to_string());

        let source = fields
            .get("source")
            .or_else(|| fields.get("service"))
            .cloned()
            .unwrap_or_else(|| "kv".to_string());

        Some(LogEvent {
            timestamp: Utc::now(),
            event_type,
            source,
            fields,
        })
    }

    /// Detect the format of a log sample.
    pub fn detect_format(sample: &str) -> LogFormat {
        let first_line = sample.lines().next().unwrap_or("");
        let trimmed = first_line.trim();

        if trimmed.starts_with('{') {
            if trimmed.contains("\"Records\"") {
                return LogFormat::CloudTrail;
            }
            if trimmed.contains("\"apiVersion\"") || trimmed.contains("\"auditID\"") {
                return LogFormat::KubernetesAudit;
            }
            return LogFormat::JsonStructured;
        }

        if trimmed.contains('[') && trimmed.contains(']') && trimmed.contains('"') {
            return LogFormat::HttpAccess;
        }

        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        if months.iter().any(|m| trimmed.starts_with(m)) || trimmed.starts_with('<') {
            return LogFormat::Syslog;
        }

        if trimmed.contains('=') {
            return LogFormat::KeyValue;
        }

        LogFormat::JsonStructured // Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_log() {
        let line = r#"{"timestamp":"2024-01-15T10:30:00Z","event_type":"login_failed","source":"auth","user":"admin","source_ip":"10.0.0.1"}"#;
        let event = LogParser::parse_json(line).unwrap();
        assert_eq!(event.event_type, "login_failed");
        assert_eq!(event.source, "auth");
        assert_eq!(event.fields.get("user"), Some(&"admin".to_string()));
    }

    #[test]
    fn test_parse_syslog() {
        let line = "Jan  5 12:00:00 myhost sshd[1234]: Failed password for root from 10.0.0.1";
        let event = LogParser::parse_syslog(line).unwrap();
        assert_eq!(event.event_type, "auth_failure");
        assert_eq!(event.source, "myhost");
    }

    #[test]
    fn test_parse_syslog_with_priority() {
        let line = "<34>Jan  5 12:00:00 myhost sshd[1234]: Connection accepted";
        let event = LogParser::parse_syslog(line).unwrap();
        assert_eq!(event.source, "myhost");
    }

    #[test]
    fn test_parse_http_access() {
        let line = r#"192.168.1.1 - - [15/Jan/2024:10:30:00 +0000] "GET /admin HTTP/1.1" 403 512"#;
        let event = LogParser::parse_http_access(line).unwrap();
        assert_eq!(event.event_type, "auth_failure");
        assert_eq!(
            event.fields.get("source_ip"),
            Some(&"192.168.1.1".to_string())
        );
        assert_eq!(event.fields.get("status"), Some(&"403".to_string()));
    }

    #[test]
    fn test_parse_http_access_200() {
        let line = r#"10.0.0.1 - user [15/Jan/2024:10:30:00 +0000] "GET / HTTP/1.1" 200 1024"#;
        let event = LogParser::parse_http_access(line).unwrap();
        assert_eq!(event.event_type, "http_request");
    }

    #[test]
    fn test_parse_cloudtrail() {
        let json = r#"{
            "Records": [{
                "eventName": "ConsoleLogin",
                "eventSource": "signin.amazonaws.com",
                "eventTime": "2024-01-15T10:30:00Z",
                "awsRegion": "us-east-1",
                "sourceIPAddress": "1.2.3.4"
            }]
        }"#;

        let events = LogParser::parse_cloudtrail(json);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "api_call");
        assert_eq!(
            events[0].fields.get("region"),
            Some(&"us-east-1".to_string())
        );
    }

    #[test]
    fn test_parse_key_value() {
        let line = "time=2024-01-15 event=login_failed user=admin source_ip=10.0.0.1";
        let event = LogParser::parse_key_value(line).unwrap();
        assert_eq!(event.event_type, "login_failed");
        assert_eq!(event.fields.get("user"), Some(&"admin".to_string()));
    }

    #[test]
    fn test_detect_format() {
        assert_eq!(
            LogParser::detect_format("{\"key\": \"value\"}"),
            LogFormat::JsonStructured
        );
        assert_eq!(
            LogParser::detect_format("Jan  1 12:00:00 host msg"),
            LogFormat::Syslog
        );
        assert_eq!(
            LogParser::detect_format(
                r#"1.2.3.4 - - [01/Jan/2024:00:00:00 +0000] "GET / HTTP/1.1" 200 0"#
            ),
            LogFormat::HttpAccess
        );
        assert_eq!(
            LogParser::detect_format("key=value foo=bar"),
            LogFormat::KeyValue
        );
    }

    #[test]
    fn test_parse_batch() {
        let content = r#"{"event_type":"test1","source":"app"}
{"event_type":"test2","source":"app"}"#;
        let events = LogParser::parse_batch(content);
        assert_eq!(events.len(), 2);
    }

    #[test]
    fn test_parse_line_auto_detect() {
        let json_line = r#"{"event_type":"test","source":"app"}"#;
        let event = LogParser::parse_line(json_line).unwrap();
        assert_eq!(event.event_type, "test");

        let empty = LogParser::parse_line("");
        assert!(empty.is_none());
    }

    #[test]
    fn test_parse_k8s_audit() {
        let json = r#"{
            "apiVersion": "audit.k8s.io/v1",
            "auditID": "abc-123",
            "verb": "delete",
            "requestReceivedTimestamp": "2024-01-15T10:30:00Z",
            "user": {
                "username": "admin",
                "groups": ["system:masters"]
            },
            "objectRef": {
                "resource": "pods",
                "namespace": "default",
                "name": "my-pod"
            },
            "sourceIPs": ["10.0.0.1"],
            "responseStatus": {
                "code": 200
            }
        }"#;

        let event = LogParser::parse_k8s_audit(json).unwrap();
        assert_eq!(event.event_type, "destructive_action");
        assert_eq!(event.source, "pods");
        assert_eq!(event.fields.get("username"), Some(&"admin".to_string()));
        assert_eq!(event.fields.get("namespace"), Some(&"default".to_string()));
        assert_eq!(
            event.fields.get("resource_name"),
            Some(&"my-pod".to_string())
        );
        assert_eq!(event.fields.get("verb"), Some(&"delete".to_string()));
    }

    #[test]
    fn test_parse_k8s_audit_auth_failure() {
        let json = r#"{
            "apiVersion": "audit.k8s.io/v1",
            "auditID": "def-456",
            "verb": "get",
            "user": {"username": "unknown-user"},
            "objectRef": {"resource": "secrets", "namespace": "kube-system"},
            "responseStatus": {"code": 403}
        }"#;

        let event = LogParser::parse_k8s_audit(json).unwrap();
        assert_eq!(event.event_type, "auth_failure");
    }

    #[test]
    fn test_parse_k8s_audit_mutation() {
        let json = r#"{
            "apiVersion": "audit.k8s.io/v1",
            "verb": "create",
            "objectRef": {"resource": "deployments", "namespace": "prod"},
            "responseStatus": {"code": 201}
        }"#;

        let event = LogParser::parse_k8s_audit(json).unwrap();
        assert_eq!(event.event_type, "k8s_mutation");
    }

    #[test]
    fn test_detect_k8s_audit_format() {
        let sample = r#"{"apiVersion": "audit.k8s.io/v1", "verb": "get"}"#;
        assert_eq!(LogParser::detect_format(sample), LogFormat::KubernetesAudit);

        let sample2 = r#"{"auditID": "abc-123", "verb": "list"}"#;
        assert_eq!(
            LogParser::detect_format(sample2),
            LogFormat::KubernetesAudit
        );
    }

    #[test]
    fn test_detect_cloudtrail_format() {
        let sample = r#"{"Records": [{"eventName": "ConsoleLogin"}]}"#;
        assert_eq!(LogParser::detect_format(sample), LogFormat::CloudTrail);
    }

    #[test]
    fn test_parse_line_k8s_auto_detect() {
        let json = r#"{"apiVersion":"audit.k8s.io/v1","auditID":"x","verb":"get","objectRef":{"resource":"pods"},"responseStatus":{"code":200}}"#;
        let event = LogParser::parse_line(json).unwrap();
        assert_eq!(event.event_type, "k8s_audit");
    }

    #[test]
    fn test_parse_cloudtrail_delete_event() {
        let json = r#"{
            "Records": [{
                "eventName": "DeleteBucket",
                "eventSource": "s3.amazonaws.com",
                "eventTime": "2024-01-15T10:30:00Z",
                "awsRegion": "us-west-2",
                "sourceIPAddress": "203.0.113.1"
            }]
        }"#;

        let events = LogParser::parse_cloudtrail(json);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, "destructive_action");
    }

    #[test]
    fn test_parse_http_server_error() {
        let line =
            r#"10.0.0.1 - - [15/Jan/2024:10:30:00 +0000] "POST /api/data HTTP/1.1" 500 1234"#;
        let event = LogParser::parse_http_access(line).unwrap();
        assert_eq!(event.event_type, "server_error");
        assert_eq!(event.fields.get("method"), Some(&"POST".to_string()));
    }

    #[test]
    fn test_parse_line_fallback_raw() {
        let line = "this is just some random log text without any known format";
        let event = LogParser::parse_line(line).unwrap();
        assert_eq!(event.event_type, "raw");
        assert_eq!(event.source, "unknown");
        assert!(event.fields.contains_key("message"));
    }

    #[test]
    fn test_log_format_display() {
        assert_eq!(LogFormat::Syslog.to_string(), "Syslog");
        assert_eq!(LogFormat::KubernetesAudit.to_string(), "K8s Audit");
        assert_eq!(LogFormat::CloudTrail.to_string(), "CloudTrail");
    }
}
