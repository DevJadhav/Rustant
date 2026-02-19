//! PHP dependency parser (composer.lock).

use super::{DepEdge, DepNode, DependencyGraph};
use crate::error::DepGraphError;

/// Parse a composer.lock file and populate the dependency graph.
///
/// composer.lock is a JSON file with two main arrays:
/// - `packages`: production dependencies
/// - `packages-dev`: development-only dependencies
///
/// Each entry has `name`, `version`, and optionally `require` (a map of dependency name to
/// version constraint).
pub fn parse_composer_lock(
    content: &str,
    graph: &mut DependencyGraph,
) -> Result<(), DepGraphError> {
    let lock: serde_json::Value =
        serde_json::from_str(content).map_err(|e| DepGraphError::LockfileParse {
            file: "composer.lock".into(),
            message: e.to_string(),
        })?;

    let mut node_map = std::collections::HashMap::new();

    // First pass: add all nodes from both packages and packages-dev
    for (section, is_dev) in &[("packages", false), ("packages-dev", true)] {
        if let Some(packages) = lock.get(*section).and_then(|p| p.as_array()) {
            for pkg in packages {
                let name = pkg
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                let version = pkg
                    .get("version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("0.0.0");

                // Strip leading "v" prefix if present (e.g., "v1.0.0" -> "1.0.0")
                let version = version.strip_prefix('v').unwrap_or(version).to_string();

                let source = pkg
                    .get("source")
                    .and_then(|s| s.get("url"))
                    .and_then(|u| u.as_str())
                    .map(|s| s.to_string());

                let license = pkg
                    .get("license")
                    .and_then(|l| l.as_array())
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                let idx = graph.add_node(DepNode {
                    name: name.clone(),
                    version,
                    ecosystem: "packagist".into(),
                    is_direct: true,
                    is_dev: *is_dev,
                    license,
                    source,
                });

                node_map.insert(name, idx);
            }
        }
    }

    // Second pass: add edges from require fields
    for section in &["packages", "packages-dev"] {
        if let Some(packages) = lock.get(*section).and_then(|p| p.as_array()) {
            for pkg in packages {
                let name = pkg
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                if let (Some(&from_idx), Some(require)) = (
                    node_map.get(&name),
                    pkg.get("require").and_then(|r| r.as_object()),
                ) {
                    for (dep_name, version_req) in require {
                        // Skip PHP platform requirements (php, ext-*)
                        if dep_name == "php"
                            || dep_name.starts_with("ext-")
                            || dep_name.starts_with("lib-")
                        {
                            continue;
                        }

                        let req = version_req.as_str().unwrap_or("*").to_string();

                        if let Some(&to_idx) = node_map.get(dep_name) {
                            graph.add_edge(
                                from_idx,
                                to_idx,
                                DepEdge {
                                    version_req: Some(req),
                                    optional: false,
                                },
                            );
                        }
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
    fn test_parse_composer_lock_basic() {
        let content = r#"{
    "packages": [
        {
            "name": "monolog/monolog",
            "version": "3.4.0",
            "require": {
                "php": ">=8.1",
                "psr/log": "^2.0 || ^3.0"
            },
            "license": ["MIT"],
            "source": {
                "type": "git",
                "url": "https://github.com/Seldaek/monolog.git"
            }
        },
        {
            "name": "psr/log",
            "version": "3.0.0",
            "require": {
                "php": ">=8.0.0"
            },
            "license": ["MIT"]
        }
    ],
    "packages-dev": [
        {
            "name": "phpunit/phpunit",
            "version": "v10.3.1",
            "require": {
                "php": ">=8.1",
                "psr/log": "^3.0"
            },
            "license": ["BSD-3-Clause"]
        }
    ]
}"#;

        let mut graph = DependencyGraph::new();
        parse_composer_lock(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 3);
        assert!(graph.find_node("monolog/monolog").is_some());
        assert!(graph.find_node("psr/log").is_some());
        assert!(graph.find_node("phpunit/phpunit").is_some());

        // Verify monolog properties
        let mono_idx = graph.find_node("monolog/monolog").unwrap();
        let mono_node = graph.get_node(mono_idx).unwrap();
        assert_eq!(mono_node.version, "3.4.0");
        assert_eq!(mono_node.ecosystem, "packagist");
        assert!(!mono_node.is_dev);
        assert_eq!(mono_node.license.as_deref(), Some("MIT"));
        assert!(mono_node.source.is_some());

        // Verify phpunit is dev
        let phpunit_idx = graph.find_node("phpunit/phpunit").unwrap();
        let phpunit_node = graph.get_node(phpunit_idx).unwrap();
        assert!(phpunit_node.is_dev);
        // Version should have "v" prefix stripped
        assert_eq!(phpunit_node.version, "10.3.1");

        // Verify edge: monolog -> psr/log (php requirement should be skipped)
        let deps = graph.transitive_deps("monolog/monolog");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "psr/log");

        // Verify edge: phpunit -> psr/log
        let dev_deps = graph.transitive_deps("phpunit/phpunit");
        assert_eq!(dev_deps.len(), 1);
        assert_eq!(dev_deps[0].name, "psr/log");
    }

    #[test]
    fn test_parse_composer_lock_no_require() {
        let content = r#"{
    "packages": [
        {
            "name": "vendor/simple-pkg",
            "version": "1.0.0"
        }
    ],
    "packages-dev": []
}"#;

        let mut graph = DependencyGraph::new();
        parse_composer_lock(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 1);
        let idx = graph.find_node("vendor/simple-pkg").unwrap();
        let node = graph.get_node(idx).unwrap();
        assert_eq!(node.version, "1.0.0");
        assert!(!node.is_dev);
        assert_eq!(graph.edge_count(), 0);
    }

    #[test]
    fn test_parse_composer_lock_empty() {
        let content = r#"{"packages": [], "packages-dev": []}"#;

        let mut graph = DependencyGraph::new();
        parse_composer_lock(content, &mut graph).unwrap();
        assert_eq!(graph.package_count(), 0);
    }

    #[test]
    fn test_parse_composer_lock_invalid_json() {
        let content = "not valid json";

        let mut graph = DependencyGraph::new();
        let result = parse_composer_lock(content, &mut graph);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_composer_lock_skips_platform_deps() {
        let content = r#"{
    "packages": [
        {
            "name": "guzzlehttp/guzzle",
            "version": "7.8.0",
            "require": {
                "php": "^8.1",
                "ext-json": "*",
                "ext-curl": "*",
                "lib-openssl": ">=1.0.1",
                "psr/http-message": "^1.1 || ^2.0"
            }
        },
        {
            "name": "psr/http-message",
            "version": "2.0.0"
        }
    ],
    "packages-dev": []
}"#;

        let mut graph = DependencyGraph::new();
        parse_composer_lock(content, &mut graph).unwrap();

        // php, ext-json, ext-curl, lib-openssl should all be skipped
        let deps = graph.transitive_deps("guzzlehttp/guzzle");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "psr/http-message");
    }
}
