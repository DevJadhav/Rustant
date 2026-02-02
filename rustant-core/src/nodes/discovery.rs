//! Node discovery — finds nodes on the local network or locally.
//!
//! Supports local-only discovery (this machine) and mDNS-based LAN
//! peer discovery using the `_rustant._tcp.local.` service name.
//! The mDNS layer is trait-abstracted for testability.

use super::types::{Capability, NodeId, NodeInfo, Platform};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── mDNS constants ──────────────────────────────────────────────────

/// mDNS multicast group (IPv4).
pub const MDNS_MULTICAST_ADDR: &str = "224.0.0.251";
/// mDNS port.
pub const MDNS_PORT: u16 = 5353;
/// Service name used for Rustant node discovery.
pub const RUSTANT_SERVICE_NAME: &str = "_rustant._tcp.local.";

// ── mDNS service record ─────────────────────────────────────────────

/// An mDNS service record describing a Rustant node on the LAN.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdnsServiceRecord {
    /// The service name (always `_rustant._tcp.local.`).
    pub service_name: String,
    /// Human-readable instance name (e.g. "DevMac-Rustant").
    pub instance_name: String,
    /// IP address of the advertising node.
    pub address: String,
    /// Gateway port the node listens on.
    pub port: u16,
    /// Platform of the advertising node.
    pub platform: Platform,
    /// Node id.
    pub node_id: String,
    /// Comma-separated capability list.
    pub capabilities_csv: String,
}

impl MdnsServiceRecord {
    /// Parse capabilities from the CSV field.
    pub fn parse_capabilities(&self) -> Vec<Capability> {
        if self.capabilities_csv.is_empty() {
            return Vec::new();
        }
        self.capabilities_csv
            .split(',')
            .filter_map(|s| match s.trim() {
                "shell" => Some(Capability::Shell),
                "filesystem" => Some(Capability::FileSystem),
                "applescript" => Some(Capability::AppleScript),
                "automator" => Some(Capability::Automator),
                "screenshot" => Some(Capability::Screenshot),
                "clipboard" => Some(Capability::Clipboard),
                "notifications" => Some(Capability::Notifications),
                "browser" => Some(Capability::Browser),
                "camera" => Some(Capability::Camera),
                "screen_record" => Some(Capability::ScreenRecord),
                "location" => Some(Capability::Location),
                other if other.starts_with("app_control:") => {
                    Some(Capability::AppControl(other[12..].to_string()))
                }
                other if other.starts_with("custom:") => {
                    Some(Capability::Custom(other[7..].to_string()))
                }
                _ => None,
            })
            .collect()
    }

    /// Build capability CSV from a slice of capabilities.
    pub fn capabilities_to_csv(caps: &[Capability]) -> String {
        caps.iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Convert to a `DiscoveredNode`.
    pub fn to_discovered_node(&self) -> DiscoveredNode {
        DiscoveredNode {
            node_id: NodeId::new(&self.node_id),
            address: self.address.clone(),
            port: self.port,
            platform: self.platform,
            capabilities: self.parse_capabilities(),
            discovered_at: Utc::now(),
        }
    }
}

// ── mDNS service trait ──────────────────────────────────────────────

/// Trait abstracting mDNS network operations for testability.
#[async_trait]
pub trait MdnsTransport: Send + Sync {
    /// Register (advertise) this node on the local network.
    async fn register(&self, record: &MdnsServiceRecord) -> Result<(), String>;

    /// Unregister (stop advertising) this node.
    async fn unregister(&self) -> Result<(), String>;

    /// Perform a single discovery scan and return found service records.
    async fn discover(&self, timeout_ms: u64) -> Result<Vec<MdnsServiceRecord>, String>;
}

// ── mDNS discovery coordinator ──────────────────────────────────────

/// Configuration for mDNS-based node discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdnsConfig {
    /// Whether mDNS discovery is enabled.
    pub enabled: bool,
    /// Scan interval in seconds for background discovery.
    pub scan_interval_secs: u64,
    /// Timeout in milliseconds for each discovery scan.
    pub scan_timeout_ms: u64,
    /// Maximum age in seconds before a node is considered stale.
    pub stale_threshold_secs: u64,
}

impl Default for MdnsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            scan_interval_secs: 30,
            scan_timeout_ms: 3000,
            stale_threshold_secs: 120,
        }
    }
}

/// mDNS-based discovery coordinator.
///
/// Wraps an `MdnsTransport` implementation and manages registration,
/// scanning, and stale-node pruning.
pub struct MdnsDiscovery {
    transport: Box<dyn MdnsTransport>,
    config: MdnsConfig,
    /// The record this node advertises (set after `register()`).
    local_record: Option<MdnsServiceRecord>,
    /// Nodes found via mDNS scans.
    found: Vec<DiscoveredNode>,
}

impl MdnsDiscovery {
    pub fn new(transport: Box<dyn MdnsTransport>, config: MdnsConfig) -> Self {
        Self {
            transport,
            config,
            local_record: None,
            found: Vec::new(),
        }
    }

    /// Register this node on the network.
    pub async fn register(&mut self, record: MdnsServiceRecord) -> Result<(), String> {
        self.transport.register(&record).await?;
        self.local_record = Some(record);
        Ok(())
    }

    /// Unregister this node.
    pub async fn unregister(&mut self) -> Result<(), String> {
        self.transport.unregister().await?;
        self.local_record = None;
        Ok(())
    }

    /// Whether this node is currently registered/advertising.
    pub fn is_registered(&self) -> bool {
        self.local_record.is_some()
    }

    /// Perform a single discovery scan. Returns newly found nodes.
    pub async fn scan(&mut self) -> Result<Vec<DiscoveredNode>, String> {
        let records = self.transport.discover(self.config.scan_timeout_ms).await?;
        let local_id = self.local_record.as_ref().map(|r| r.node_id.as_str());

        let mut new_nodes = Vec::new();
        for record in records {
            // Skip our own advertisement.
            if let Some(lid) = local_id {
                if record.node_id == lid {
                    continue;
                }
            }

            let already_known = self.found.iter().any(|n| n.node_id.0 == record.node_id);
            let discovered = record.to_discovered_node();

            if already_known {
                // Refresh timestamp for existing node.
                if let Some(existing) = self
                    .found
                    .iter_mut()
                    .find(|n| n.node_id.0 == record.node_id)
                {
                    existing.discovered_at = Utc::now();
                    existing.capabilities = discovered.capabilities;
                }
            } else {
                new_nodes.push(discovered.clone());
                self.found.push(discovered);
            }
        }

        Ok(new_nodes)
    }

    /// All currently known remote nodes from mDNS.
    pub fn found_nodes(&self) -> &[DiscoveredNode] {
        &self.found
    }

    /// Remove stale nodes that haven't been refreshed within the threshold.
    pub fn prune_stale(&mut self) -> usize {
        let now = Utc::now();
        let threshold = self.config.stale_threshold_secs as i64;
        let before = self.found.len();
        self.found.retain(|node| {
            let age = now.signed_duration_since(node.discovered_at);
            age.num_seconds() < threshold
        });
        before - self.found.len()
    }

    /// Clear all found nodes.
    pub fn clear(&mut self) {
        self.found.clear();
    }

    /// Access the config.
    pub fn config(&self) -> &MdnsConfig {
        &self.config
    }
}

// ── Real UDP mDNS transport ─────────────────────────────────────────

/// A real mDNS transport that uses UDP multicast.
///
/// Sends and receives mDNS-like JSON packets on `224.0.0.251:5353`.
/// This is a simplified Rustant-specific protocol — not full RFC 6762 —
/// but uses the standard mDNS multicast group for LAN discovery.
pub struct UdpMdnsTransport {
    bind_addr: String,
}

impl UdpMdnsTransport {
    pub fn new() -> Self {
        Self {
            bind_addr: format!("0.0.0.0:{}", MDNS_PORT),
        }
    }

    pub fn with_bind_addr(addr: impl Into<String>) -> Self {
        Self {
            bind_addr: addr.into(),
        }
    }
}

impl Default for UdpMdnsTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MdnsTransport for UdpMdnsTransport {
    async fn register(&self, record: &MdnsServiceRecord) -> Result<(), String> {
        let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| format!("Failed to bind UDP socket: {e}"))?;

        let payload =
            serde_json::to_vec(record).map_err(|e| format!("Failed to serialize record: {e}"))?;

        let dest = format!("{}:{}", MDNS_MULTICAST_ADDR, MDNS_PORT);
        socket
            .send_to(&payload, &dest)
            .await
            .map_err(|e| format!("Failed to send mDNS announcement: {e}"))?;

        Ok(())
    }

    async fn unregister(&self) -> Result<(), String> {
        // In a full implementation, send a "goodbye" packet.
        // For now, simply stop advertising.
        Ok(())
    }

    async fn discover(&self, timeout_ms: u64) -> Result<Vec<MdnsServiceRecord>, String> {
        use std::net::Ipv4Addr;

        let socket = tokio::net::UdpSocket::bind(&self.bind_addr)
            .await
            .map_err(|e| format!("Failed to bind mDNS socket: {e}"))?;

        let multicast_addr: Ipv4Addr = MDNS_MULTICAST_ADDR
            .parse()
            .map_err(|e| format!("Invalid multicast addr: {e}"))?;

        socket
            .join_multicast_v4(multicast_addr, Ipv4Addr::UNSPECIFIED)
            .map_err(|e| format!("Failed to join multicast group: {e}"))?;

        let mut buf = vec![0u8; 4096];
        let mut records = Vec::new();
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_millis(timeout_ms);

        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                break;
            }

            match tokio::time::timeout(remaining, socket.recv_from(&mut buf)).await {
                Ok(Ok((len, _addr))) => {
                    if let Ok(record) = serde_json::from_slice::<MdnsServiceRecord>(&buf[..len]) {
                        if record.service_name == RUSTANT_SERVICE_NAME {
                            records.push(record);
                        }
                    }
                }
                Ok(Err(_)) => break,
                Err(_) => break, // timeout
            }
        }

        Ok(records)
    }
}

// ── Original local discovery ────────────────────────────────────────

/// A node discovered on the network with connection metadata.
#[derive(Debug, Clone)]
pub struct DiscoveredNode {
    pub node_id: NodeId,
    pub address: String,
    pub port: u16,
    pub platform: Platform,
    pub capabilities: Vec<Capability>,
    pub discovered_at: DateTime<Utc>,
}

/// Discovers nodes available for task execution (local + mDNS).
#[derive(Debug, Clone, Default)]
pub struct NodeDiscovery {
    discovered: Vec<NodeInfo>,
    network_discovered: Vec<DiscoveredNode>,
}

impl NodeDiscovery {
    pub fn new() -> Self {
        Self::default()
    }

    /// Discover the local machine as a node.
    pub fn discover_local(&mut self) -> NodeInfo {
        let platform = Self::detect_platform();
        let hostname = Self::get_hostname();
        let info = NodeInfo {
            node_id: NodeId::new(format!("local-{}", hostname)),
            name: format!("Local ({})", hostname),
            platform,
            hostname,
            registered_at: Utc::now(),
            os_version: None,
            agent_version: env!("CARGO_PKG_VERSION").to_string(),
            uptime_secs: 0,
        };
        self.discovered.push(info.clone());
        info
    }

    /// Number of discovered nodes.
    pub fn discovered_count(&self) -> usize {
        self.discovered.len()
    }

    /// All discovered nodes.
    pub fn discovered_nodes(&self) -> &[NodeInfo] {
        &self.discovered
    }

    /// Clear all discovered nodes.
    pub fn clear(&mut self) {
        self.discovered.clear();
        self.network_discovered.clear();
    }

    /// Add a network-discovered node.
    pub fn add_network_node(&mut self, node: DiscoveredNode) {
        self.network_discovered.push(node);
    }

    /// List all network-discovered nodes.
    pub fn network_nodes(&self) -> &[DiscoveredNode] {
        &self.network_discovered
    }

    /// Remove stale network-discovered nodes older than `max_age_secs`.
    /// Returns the number of removed entries.
    pub fn remove_stale(&mut self, max_age_secs: u64) -> usize {
        let now = Utc::now();
        let before = self.network_discovered.len();
        self.network_discovered.retain(|node| {
            let age = now.signed_duration_since(node.discovered_at);
            age.num_seconds() < max_age_secs as i64
        });
        before - self.network_discovered.len()
    }

    /// Detect the current platform.
    fn detect_platform() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::MacOS
        } else if cfg!(target_os = "linux") {
            Platform::Linux
        } else if cfg!(target_os = "windows") {
            Platform::Windows
        } else {
            Platform::Unknown
        }
    }

    /// Get the hostname.
    fn get_hostname() -> String {
        std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "unknown".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    // ── Mock mDNS transport ─────────────────────────────────────────

    struct MockMdnsTransport {
        registered: Arc<Mutex<Option<MdnsServiceRecord>>>,
        scan_results: Arc<Mutex<Vec<MdnsServiceRecord>>>,
    }

    impl MockMdnsTransport {
        fn new() -> Self {
            Self {
                registered: Arc::new(Mutex::new(None)),
                scan_results: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_scan_results(results: Vec<MdnsServiceRecord>) -> Self {
            Self {
                registered: Arc::new(Mutex::new(None)),
                scan_results: Arc::new(Mutex::new(results)),
            }
        }

        #[allow(dead_code)]
        fn registered_record(&self) -> Option<MdnsServiceRecord> {
            self.registered.lock().unwrap().clone()
        }
    }

    #[async_trait]
    impl MdnsTransport for MockMdnsTransport {
        async fn register(&self, record: &MdnsServiceRecord) -> Result<(), String> {
            *self.registered.lock().unwrap() = Some(record.clone());
            Ok(())
        }

        async fn unregister(&self) -> Result<(), String> {
            *self.registered.lock().unwrap() = None;
            Ok(())
        }

        async fn discover(&self, _timeout_ms: u64) -> Result<Vec<MdnsServiceRecord>, String> {
            Ok(self.scan_results.lock().unwrap().clone())
        }
    }

    fn sample_record(node_id: &str, addr: &str, port: u16) -> MdnsServiceRecord {
        MdnsServiceRecord {
            service_name: RUSTANT_SERVICE_NAME.to_string(),
            instance_name: format!("{}-Rustant", node_id),
            address: addr.to_string(),
            port,
            platform: Platform::Linux,
            node_id: node_id.to_string(),
            capabilities_csv: "shell,filesystem".to_string(),
        }
    }

    // ── Original local discovery tests ──────────────────────────────

    #[test]
    fn test_discovery_new() {
        let disc = NodeDiscovery::new();
        assert_eq!(disc.discovered_count(), 0);
    }

    #[test]
    fn test_discover_local() {
        let mut disc = NodeDiscovery::new();
        let info = disc.discover_local();
        assert!(info.node_id.0.starts_with("local-"));
        assert_eq!(disc.discovered_count(), 1);
    }

    #[test]
    fn test_discovery_clear() {
        let mut disc = NodeDiscovery::new();
        disc.discover_local();
        assert_eq!(disc.discovered_count(), 1);
        disc.clear();
        assert_eq!(disc.discovered_count(), 0);
    }

    #[test]
    fn test_discovered_node_creation() {
        let node = DiscoveredNode {
            node_id: NodeId::new("remote-1"),
            address: "192.168.1.10".into(),
            port: 8080,
            platform: Platform::Linux,
            capabilities: vec![Capability::Shell, Capability::FileSystem],
            discovered_at: Utc::now(),
        };
        assert_eq!(node.address, "192.168.1.10");
        assert_eq!(node.port, 8080);
        assert_eq!(node.capabilities.len(), 2);
    }

    #[test]
    fn test_discovery_add_and_list() {
        let mut disc = NodeDiscovery::new();
        disc.add_network_node(DiscoveredNode {
            node_id: NodeId::new("remote-1"),
            address: "10.0.0.1".into(),
            port: 9000,
            platform: Platform::MacOS,
            capabilities: vec![Capability::Shell],
            discovered_at: Utc::now(),
        });
        disc.add_network_node(DiscoveredNode {
            node_id: NodeId::new("remote-2"),
            address: "10.0.0.2".into(),
            port: 9000,
            platform: Platform::Linux,
            capabilities: vec![],
            discovered_at: Utc::now(),
        });

        assert_eq!(disc.network_nodes().len(), 2);
    }

    #[test]
    fn test_discovery_remove_stale() {
        let mut disc = NodeDiscovery::new();
        // Add a stale node (discovered 1000 seconds ago)
        disc.add_network_node(DiscoveredNode {
            node_id: NodeId::new("old"),
            address: "10.0.0.1".into(),
            port: 9000,
            platform: Platform::MacOS,
            capabilities: vec![],
            discovered_at: Utc::now() - chrono::Duration::seconds(1000),
        });
        // Add a fresh node
        disc.add_network_node(DiscoveredNode {
            node_id: NodeId::new("new"),
            address: "10.0.0.2".into(),
            port: 9000,
            platform: Platform::Linux,
            capabilities: vec![],
            discovered_at: Utc::now(),
        });

        let removed = disc.remove_stale(600); // max age 600s
        assert_eq!(removed, 1);
        assert_eq!(disc.network_nodes().len(), 1);
        assert_eq!(disc.network_nodes()[0].node_id, NodeId::new("new"));
    }

    #[test]
    fn test_discovery_no_stale() {
        let mut disc = NodeDiscovery::new();
        disc.add_network_node(DiscoveredNode {
            node_id: NodeId::new("fresh"),
            address: "10.0.0.1".into(),
            port: 9000,
            platform: Platform::MacOS,
            capabilities: vec![],
            discovered_at: Utc::now(),
        });

        let removed = disc.remove_stale(600);
        assert_eq!(removed, 0);
        assert_eq!(disc.network_nodes().len(), 1);
    }

    // ── mDNS service record tests ───────────────────────────────────

    #[test]
    fn test_mdns_constants() {
        assert_eq!(MDNS_MULTICAST_ADDR, "224.0.0.251");
        assert_eq!(MDNS_PORT, 5353);
        assert_eq!(RUSTANT_SERVICE_NAME, "_rustant._tcp.local.");
    }

    #[test]
    fn test_mdns_config_default() {
        let config = MdnsConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.scan_interval_secs, 30);
        assert_eq!(config.scan_timeout_ms, 3000);
        assert_eq!(config.stale_threshold_secs, 120);
    }

    #[test]
    fn test_mdns_config_serialization() {
        let config = MdnsConfig {
            enabled: true,
            scan_interval_secs: 60,
            scan_timeout_ms: 5000,
            stale_threshold_secs: 300,
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: MdnsConfig = serde_json::from_str(&json).unwrap();
        assert!(restored.enabled);
        assert_eq!(restored.scan_interval_secs, 60);
    }

    #[test]
    fn test_mdns_service_record_parse_capabilities() {
        let record = sample_record("node-1", "10.0.0.1", 8080);
        let caps = record.parse_capabilities();
        assert_eq!(caps.len(), 2);
        assert_eq!(caps[0], Capability::Shell);
        assert_eq!(caps[1], Capability::FileSystem);
    }

    #[test]
    fn test_mdns_service_record_parse_empty_capabilities() {
        let mut record = sample_record("node-1", "10.0.0.1", 8080);
        record.capabilities_csv = String::new();
        let caps = record.parse_capabilities();
        assert!(caps.is_empty());
    }

    #[test]
    fn test_mdns_service_record_parse_all_capability_types() {
        let mut record = sample_record("node-1", "10.0.0.1", 8080);
        record.capabilities_csv = "shell,filesystem,applescript,automator,screenshot,clipboard,notifications,browser,camera,screen_record,location,app_control:Safari,custom:gpu".to_string();
        let caps = record.parse_capabilities();
        assert_eq!(caps.len(), 13);
        assert_eq!(caps[0], Capability::Shell);
        assert_eq!(caps[6], Capability::Notifications);
        assert_eq!(caps[11], Capability::AppControl("Safari".to_string()));
        assert_eq!(caps[12], Capability::Custom("gpu".to_string()));
    }

    #[test]
    fn test_mdns_service_record_capabilities_to_csv() {
        let caps = vec![
            Capability::Shell,
            Capability::FileSystem,
            Capability::Screenshot,
        ];
        let csv = MdnsServiceRecord::capabilities_to_csv(&caps);
        assert_eq!(csv, "shell,filesystem,screenshot");
    }

    #[test]
    fn test_mdns_service_record_to_discovered_node() {
        let record = sample_record("node-x", "192.168.1.50", 9090);
        let node = record.to_discovered_node();
        assert_eq!(node.node_id, NodeId::new("node-x"));
        assert_eq!(node.address, "192.168.1.50");
        assert_eq!(node.port, 9090);
        assert_eq!(node.platform, Platform::Linux);
        assert_eq!(node.capabilities.len(), 2);
    }

    #[test]
    fn test_mdns_service_record_serialization() {
        let record = sample_record("node-1", "10.0.0.1", 8080);
        let json = serde_json::to_string(&record).unwrap();
        assert!(json.contains("_rustant._tcp.local."));
        let restored: MdnsServiceRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.node_id, "node-1");
        assert_eq!(restored.address, "10.0.0.1");
    }

    // ── MdnsDiscovery coordinator tests ─────────────────────────────

    #[tokio::test]
    async fn test_mdns_discovery_register() {
        let transport = MockMdnsTransport::new();
        let registered = transport.registered.clone();
        let config = MdnsConfig::default();
        let mut disc = MdnsDiscovery::new(Box::new(transport), config);

        assert!(!disc.is_registered());

        let record = sample_record("local-1", "127.0.0.1", 8080);
        disc.register(record).await.unwrap();

        assert!(disc.is_registered());
        let reg = registered.lock().unwrap();
        assert_eq!(reg.as_ref().unwrap().node_id, "local-1");
    }

    #[tokio::test]
    async fn test_mdns_discovery_unregister() {
        let transport = MockMdnsTransport::new();
        let config = MdnsConfig::default();
        let mut disc = MdnsDiscovery::new(Box::new(transport), config);

        let record = sample_record("local-1", "127.0.0.1", 8080);
        disc.register(record).await.unwrap();
        assert!(disc.is_registered());

        disc.unregister().await.unwrap();
        assert!(!disc.is_registered());
    }

    #[tokio::test]
    async fn test_mdns_discovery_scan_finds_remote_nodes() {
        let remote1 = sample_record("remote-a", "192.168.1.10", 9000);
        let remote2 = sample_record("remote-b", "192.168.1.11", 9001);
        let transport = MockMdnsTransport::with_scan_results(vec![remote1, remote2]);
        let config = MdnsConfig::default();
        let mut disc = MdnsDiscovery::new(Box::new(transport), config);

        let new_nodes = disc.scan().await.unwrap();
        assert_eq!(new_nodes.len(), 2);
        assert_eq!(disc.found_nodes().len(), 2);
        assert_eq!(disc.found_nodes()[0].node_id, NodeId::new("remote-a"));
        assert_eq!(disc.found_nodes()[1].node_id, NodeId::new("remote-b"));
    }

    #[tokio::test]
    async fn test_mdns_discovery_scan_skips_self() {
        let local = sample_record("local-1", "127.0.0.1", 8080);
        let remote = sample_record("remote-a", "192.168.1.10", 9000);
        let transport = MockMdnsTransport::with_scan_results(vec![local, remote]);
        let config = MdnsConfig::default();
        let mut disc = MdnsDiscovery::new(Box::new(transport), config);

        // Register so MdnsDiscovery knows its own node_id.
        let own = sample_record("local-1", "127.0.0.1", 8080);
        disc.register(own).await.unwrap();

        let new_nodes = disc.scan().await.unwrap();
        // Only the remote node should be discovered, not ourselves.
        assert_eq!(new_nodes.len(), 1);
        assert_eq!(new_nodes[0].node_id, NodeId::new("remote-a"));
        assert_eq!(disc.found_nodes().len(), 1);
    }

    #[tokio::test]
    async fn test_mdns_discovery_scan_refreshes_known_nodes() {
        let remote = sample_record("remote-a", "192.168.1.10", 9000);
        let transport = MockMdnsTransport::with_scan_results(vec![remote]);
        let config = MdnsConfig::default();
        let mut disc = MdnsDiscovery::new(Box::new(transport), config);

        // First scan: discovers the node.
        let new1 = disc.scan().await.unwrap();
        assert_eq!(new1.len(), 1);

        // Second scan: same node, should refresh, not duplicate.
        let new2 = disc.scan().await.unwrap();
        assert_eq!(new2.len(), 0); // not new
        assert_eq!(disc.found_nodes().len(), 1); // still just one
    }

    #[tokio::test]
    async fn test_mdns_discovery_scan_empty() {
        let transport = MockMdnsTransport::with_scan_results(vec![]);
        let config = MdnsConfig::default();
        let mut disc = MdnsDiscovery::new(Box::new(transport), config);

        let new_nodes = disc.scan().await.unwrap();
        assert!(new_nodes.is_empty());
        assert!(disc.found_nodes().is_empty());
    }

    #[tokio::test]
    async fn test_mdns_discovery_prune_stale() {
        let remote = sample_record("remote-a", "192.168.1.10", 9000);
        let transport = MockMdnsTransport::with_scan_results(vec![remote]);
        let config = MdnsConfig {
            stale_threshold_secs: 1, // very short threshold
            ..Default::default()
        };
        let mut disc = MdnsDiscovery::new(Box::new(transport), config);

        disc.scan().await.unwrap();
        assert_eq!(disc.found_nodes().len(), 1);

        // Manually backdate the discovered_at to make it stale.
        disc.found[0].discovered_at = Utc::now() - chrono::Duration::seconds(10);

        let pruned = disc.prune_stale();
        assert_eq!(pruned, 1);
        assert!(disc.found_nodes().is_empty());
    }

    #[tokio::test]
    async fn test_mdns_discovery_clear() {
        let remote = sample_record("remote-a", "192.168.1.10", 9000);
        let transport = MockMdnsTransport::with_scan_results(vec![remote]);
        let config = MdnsConfig::default();
        let mut disc = MdnsDiscovery::new(Box::new(transport), config);

        disc.scan().await.unwrap();
        assert_eq!(disc.found_nodes().len(), 1);

        disc.clear();
        assert!(disc.found_nodes().is_empty());
    }

    #[test]
    fn test_udp_mdns_transport_default() {
        let transport = UdpMdnsTransport::default();
        assert_eq!(transport.bind_addr, "0.0.0.0:5353");
    }

    #[test]
    fn test_udp_mdns_transport_custom_bind() {
        let transport = UdpMdnsTransport::with_bind_addr("0.0.0.0:15353");
        assert_eq!(transport.bind_addr, "0.0.0.0:15353");
    }
}
