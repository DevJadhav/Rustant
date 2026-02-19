//! Cargo.lock parser.

use super::{DepEdge, DepNode, DependencyGraph};
use crate::error::DepGraphError;

/// Parse a Cargo.lock file and populate the dependency graph.
pub fn parse_cargo_lock(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    // Cargo.lock uses TOML format
    let lock: toml::Value = toml::from_str(content).map_err(|e| DepGraphError::LockfileParse {
        file: "Cargo.lock".into(),
        message: e.to_string(),
    })?;

    let packages = lock
        .get("package")
        .and_then(|p| p.as_array())
        .ok_or_else(|| DepGraphError::LockfileParse {
            file: "Cargo.lock".into(),
            message: "No [[package]] entries found".into(),
        })?;

    // First pass: add all nodes
    let mut node_map = std::collections::HashMap::new();

    for pkg in packages {
        let name = pkg
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown")
            .to_string();
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();
        let source = pkg
            .get("source")
            .and_then(|s| s.as_str())
            .map(|s| s.to_string());

        let idx = graph.add_node(DepNode {
            name: name.clone(),
            version: version.clone(),
            ecosystem: "cargo".into(),
            is_direct: false, // Updated below if it appears in Cargo.toml
            is_dev: false,
            license: None,
            source,
        });

        node_map.insert(format!("{name} {version}"), idx);
    }

    // Second pass: add edges from dependencies
    for pkg in packages {
        let name = pkg
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown");
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0");
        let key = format!("{name} {version}");

        if let Some(&from_idx) = node_map.get(&key)
            && let Some(deps) = pkg.get("dependencies").and_then(|d| d.as_array())
        {
            for dep in deps {
                if let Some(dep_str) = dep.as_str() {
                    // Dependencies in Cargo.lock are formatted as "name version"
                    // or just "name" (for workspace deps)
                    let dep_parts: Vec<&str> = dep_str.splitn(3, ' ').collect();
                    let dep_name = dep_parts[0];

                    // Find the matching node
                    let to_idx = if dep_parts.len() >= 2 {
                        let dep_key = format!("{} {}", dep_parts[0], dep_parts[1]);
                        node_map.get(&dep_key).copied()
                    } else {
                        // Find any node with this name
                        node_map
                            .iter()
                            .find(|(k, _)| k.starts_with(&format!("{dep_name} ")))
                            .map(|(_, &idx)| idx)
                    };

                    if let Some(to_idx) = to_idx {
                        graph.add_edge(
                            from_idx,
                            to_idx,
                            DepEdge {
                                version_req: dep_parts.get(1).map(|v| v.to_string()),
                                optional: false,
                            },
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_cargo_lock() {
        let content = r#"
[[package]]
name = "serde"
version = "1.0.200"
source = "registry+https://github.com/rust-lang/crates.io-index"

[[package]]
name = "serde_json"
version = "1.0.120"
source = "registry+https://github.com/rust-lang/crates.io-index"
dependencies = [
    "serde 1.0.200",
]

[[package]]
name = "my-app"
version = "0.1.0"
dependencies = [
    "serde 1.0.200",
    "serde_json 1.0.120",
]
"#;

        let mut graph = DependencyGraph::new();
        parse_cargo_lock(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 3);
        assert!(graph.find_node("serde").is_some());
        assert!(graph.find_node("serde_json").is_some());

        // serde_json should depend on serde
        let deps = graph.transitive_deps("serde_json");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "serde");
    }
}
