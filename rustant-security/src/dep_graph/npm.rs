//! NPM/Yarn/pnpm lockfile parsers.

use super::{DepEdge, DepNode, DependencyGraph};
use crate::error::DepGraphError;

/// Parse a package-lock.json file and populate the dependency graph.
pub fn parse_package_lock(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    let lock: serde_json::Value =
        serde_json::from_str(content).map_err(|e| DepGraphError::LockfileParse {
            file: "package-lock.json".into(),
            message: e.to_string(),
        })?;

    // v2/v3 format uses "packages" key
    if let Some(packages) = lock.get("packages").and_then(|p| p.as_object()) {
        for (path, pkg_info) in packages {
            // Skip the root package (empty path)
            if path.is_empty() {
                continue;
            }

            // Extract package name from path (e.g., "node_modules/lodash")
            let name = path
                .strip_prefix("node_modules/")
                .unwrap_or(path)
                .to_string();
            let version = pkg_info
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0")
                .to_string();
            let is_dev = pkg_info
                .get("dev")
                .and_then(|d| d.as_bool())
                .unwrap_or(false);

            let idx = graph.add_node(DepNode {
                name: name.clone(),
                version,
                ecosystem: "npm".into(),
                is_direct: !path.contains("node_modules/node_modules/"),
                is_dev,
                license: pkg_info
                    .get("license")
                    .and_then(|l| l.as_str())
                    .map(|s| s.to_string()),
                source: pkg_info
                    .get("resolved")
                    .and_then(|r| r.as_str())
                    .map(|s| s.to_string()),
            });

            // Add edges from dependencies
            if let Some(deps) = pkg_info.get("dependencies").and_then(|d| d.as_object()) {
                for (dep_name, version_req) in deps {
                    let req = version_req.as_str().unwrap_or("*").to_string();
                    // Find the dependency node
                    if let Some(dep_idx) = graph.find_node(dep_name) {
                        graph.add_edge(
                            idx,
                            dep_idx,
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
    // v1 format uses "dependencies" key
    else if let Some(deps) = lock.get("dependencies").and_then(|d| d.as_object()) {
        parse_npm_v1_deps(deps, graph, true)?;
    }

    Ok(())
}

fn parse_npm_v1_deps(
    deps: &serde_json::Map<String, serde_json::Value>,
    graph: &mut DependencyGraph,
    is_direct: bool,
) -> Result<(), DepGraphError> {
    for (name, info) in deps {
        let version = info
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();
        let is_dev = info.get("dev").and_then(|d| d.as_bool()).unwrap_or(false);

        graph.add_node(DepNode {
            name: name.clone(),
            version,
            ecosystem: "npm".into(),
            is_direct,
            is_dev,
            license: None,
            source: info
                .get("resolved")
                .and_then(|r| r.as_str())
                .map(|s| s.to_string()),
        });

        // Recurse into nested dependencies
        if let Some(nested) = info.get("dependencies").and_then(|d| d.as_object()) {
            parse_npm_v1_deps(nested, graph, false)?;
        }
    }
    Ok(())
}

/// Parse a yarn.lock file (v1 format) and populate the dependency graph.
pub fn parse_yarn_lock(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    // yarn.lock v1 has a custom format:
    // "package@^version":
    //   version "resolved_version"
    //   resolved "url"
    //   dependencies:
    //     dep "^version"

    let mut current_name = String::new();
    #[allow(unused_assignments)]
    let mut current_version = String::new();
    let mut in_deps = false;
    let mut current_idx = None;

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // New package entry
        if !line.starts_with(' ') && !line.starts_with('\t') && trimmed.ends_with(':') {
            in_deps = false;
            // Parse package name from header like "lodash@^4.17.21:"
            let header = trimmed.trim_end_matches(':').trim_matches('"');
            if let Some(at_pos) = header.rfind('@') {
                current_name = header[..at_pos].to_string();
            } else {
                current_name = header.to_string();
            }
        } else if trimmed.starts_with("version ") {
            current_version = trimmed
                .strip_prefix("version ")
                .unwrap_or("")
                .trim_matches('"')
                .to_string();

            let idx = graph.add_node(DepNode {
                name: current_name.clone(),
                version: current_version.clone(),
                ecosystem: "npm".into(),
                is_direct: false,
                is_dev: false,
                license: None,
                source: None,
            });
            current_idx = Some(idx);
        } else if trimmed == "dependencies:" {
            in_deps = true;
        } else if in_deps && trimmed.contains(' ') {
            // Dependency line: "dep-name" "^version"
            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
            if let Some(dep_name) = parts.first() {
                let dep_name = dep_name.trim_matches('"');
                let version_req = parts.get(1).map(|v| v.trim_matches('"').to_string());

                if let (Some(from_idx), Some(to_idx)) = (current_idx, graph.find_node(dep_name)) {
                    graph.add_edge(
                        from_idx,
                        to_idx,
                        DepEdge {
                            version_req,
                            optional: false,
                        },
                    );
                }
            }
        } else if !line.starts_with(' ') && !line.starts_with('\t') {
            in_deps = false;
        }
    }

    Ok(())
}

/// Parse a pnpm-lock.yaml file and populate the dependency graph.
///
/// pnpm-lock.yaml v6+ uses a `packages:` section with keys like `/pkg@version`.
/// Earlier versions use `dependencies:` and `devDependencies:` top-level keys.
pub fn parse_pnpm_lock(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    // pnpm-lock.yaml has a structured format. We parse line-by-line since
    // we don't have a YAML dependency and the format is simple enough.
    let mut in_packages = false;
    let mut in_dependencies = false;
    let mut in_dev_dependencies = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines without resetting section state
        if trimmed.is_empty() {
            continue;
        }

        // Detect top-level sections
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_packages = trimmed == "packages:" || trimmed.starts_with("packages:");
            in_dependencies = trimmed == "dependencies:" || trimmed.starts_with("dependencies:");
            in_dev_dependencies =
                trimmed == "devDependencies:" || trimmed.starts_with("devDependencies:");
            continue;
        }

        // Parse packages section (pnpm v6+): keys like "/lodash@4.17.21:" or "lodash@4.17.21:"
        if in_packages {
            let indent = line.len() - line.trim_start().len();
            // Package entries are at 2-space indent, their properties at 4+
            if indent <= 4 && trimmed.contains('@') && trimmed.ends_with(':') {
                let key = trimmed.trim_end_matches(':').trim_start_matches('/');
                if let Some(at_pos) = key.rfind('@') {
                    let name = &key[..at_pos];
                    let version = &key[at_pos + 1..];
                    if !name.is_empty() && !version.is_empty() {
                        graph.add_node(DepNode {
                            name: name.to_string(),
                            version: version.to_string(),
                            ecosystem: "npm".into(),
                            is_direct: false,
                            is_dev: false,
                            license: None,
                            source: None,
                        });
                    }
                }
            }
        }

        // Parse dependencies/devDependencies sections (version specifiers)
        if (in_dependencies || in_dev_dependencies) && trimmed.contains(':') {
            let indent = line.len() - line.trim_start().len();
            if (2..=4).contains(&indent) {
                let parts: Vec<&str> = trimmed.splitn(2, ':').collect();
                if let (Some(name), Some(version_str)) = (parts.first(), parts.get(1)) {
                    let name = name.trim().trim_matches('\'').trim_matches('"');
                    let version = version_str.trim().trim_matches('\'').trim_matches('"');
                    if !name.is_empty() && !version.is_empty() {
                        graph.add_node(DepNode {
                            name: name.to_string(),
                            version: version.to_string(),
                            ecosystem: "npm".into(),
                            is_direct: true,
                            is_dev: in_dev_dependencies,
                            license: None,
                            source: None,
                        });
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
    fn test_parse_package_lock_v2() {
        let content = r#"{
  "name": "my-app",
  "version": "1.0.0",
  "lockfileVersion": 2,
  "packages": {
    "": {
      "name": "my-app",
      "version": "1.0.0"
    },
    "node_modules/lodash": {
      "version": "4.17.21",
      "license": "MIT"
    },
    "node_modules/express": {
      "version": "4.18.2",
      "license": "MIT",
      "dependencies": {
        "lodash": "^4.17.0"
      }
    }
  }
}"#;

        let mut graph = DependencyGraph::new();
        parse_package_lock(content, &mut graph).unwrap();

        assert!(graph.package_count() >= 2);
        assert!(graph.find_node("lodash").is_some());
        assert!(graph.find_node("express").is_some());
    }

    #[test]
    fn test_parse_pnpm_lock_v6_packages() {
        let content = "\
lockfileVersion: '6.0'

packages:

  /lodash@4.17.21:
    resolution: {integrity: sha512-abc}
    dev: false

  /express@4.18.2:
    resolution: {integrity: sha512-def}
    dependencies:
      lodash: 4.17.21
    dev: false
";
        let mut graph = DependencyGraph::new();
        parse_pnpm_lock(content, &mut graph).unwrap();

        assert!(graph.package_count() >= 2);
        assert!(graph.find_node("lodash").is_some());
        assert!(graph.find_node("express").is_some());
    }

    #[test]
    fn test_parse_pnpm_lock_dependencies_section() {
        let content = "\
lockfileVersion: '9.0'

dependencies:
  react: 18.2.0
  react-dom: 18.2.0

devDependencies:
  typescript: 5.3.3
  eslint: 8.56.0
";
        let mut graph = DependencyGraph::new();
        parse_pnpm_lock(content, &mut graph).unwrap();

        assert!(graph.package_count() >= 4);
        assert!(graph.find_node("react").is_some());
        assert!(graph.find_node("typescript").is_some());
    }

    #[test]
    fn test_parse_pnpm_lock_empty() {
        let content = "lockfileVersion: '6.0'\n";
        let mut graph = DependencyGraph::new();
        parse_pnpm_lock(content, &mut graph).unwrap();
        assert_eq!(graph.package_count(), 0);
    }
}
