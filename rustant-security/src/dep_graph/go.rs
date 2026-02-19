//! Go module lockfile parser (go.sum).

use super::{DepEdge, DepNode, DependencyGraph};
use crate::error::DepGraphError;

/// Parse a go.sum file and populate the dependency graph.
///
/// go.sum format: each line is `<module> <version>[/go.mod] <hash>`.
/// Lines with `/go.mod` suffix on the version are metadata entries and are skipped;
/// only direct module lines (without `/go.mod`) are included.
pub fn parse_go_sum(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    let mut node_map = std::collections::HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() < 3 {
            continue;
        }

        let module = parts[0];
        let version_field = parts[1];

        // Skip /go.mod entries — only keep the direct module lines
        if version_field.ends_with("/go.mod") {
            continue;
        }

        // Strip the leading "v" prefix if present (e.g., "v1.2.3" -> "1.2.3")
        let version = version_field.strip_prefix('v').unwrap_or(version_field);

        let key = format!("{module}@{version}");
        if node_map.contains_key(&key) {
            continue;
        }

        let idx = graph.add_node(DepNode {
            name: module.to_string(),
            version: version.to_string(),
            ecosystem: "go".into(),
            is_direct: false,
            is_dev: false,
            license: None,
            source: None,
        });

        node_map.insert(key, idx);
    }

    // go.sum does not encode dependency edges, so we only have nodes.
    // Edges would require parsing go.mod, which is a separate concern.

    Ok(())
}

/// Parse a go.mod file and add dependency edges where possible.
///
/// This is a supplementary parser that enriches a graph already populated by `parse_go_sum`.
/// It marks `require` entries as direct dependencies and adds edges from the module root.
pub fn parse_go_mod(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    let mut in_require_block = false;
    let mut root_idx = None;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines and comments
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        // Parse the module declaration
        if trimmed.starts_with("module ") {
            let module_name = trimmed
                .strip_prefix("module ")
                .unwrap_or("")
                .trim()
                .to_string();
            root_idx = Some(graph.add_node(DepNode {
                name: module_name,
                version: "0.0.0".into(),
                ecosystem: "go".into(),
                is_direct: true,
                is_dev: false,
                license: None,
                source: None,
            }));
            continue;
        }

        // Detect require block start/end
        if trimmed == "require (" {
            in_require_block = true;
            continue;
        }
        if trimmed == ")" {
            in_require_block = false;
            continue;
        }

        // Single-line require
        if trimmed.starts_with("require ") && !trimmed.contains('(') {
            let dep_part = trimmed.strip_prefix("require ").unwrap_or("").trim();
            let parts: Vec<&str> = dep_part.split_whitespace().collect();
            if parts.len() >= 2 {
                let dep_name = parts[0];
                let dep_version = parts[1].strip_prefix('v').unwrap_or(parts[1]);
                let dep_idx = graph.add_node(DepNode {
                    name: dep_name.to_string(),
                    version: dep_version.to_string(),
                    ecosystem: "go".into(),
                    is_direct: true,
                    is_dev: false,
                    license: None,
                    source: None,
                });
                if let Some(root) = root_idx {
                    graph.add_edge(
                        root,
                        dep_idx,
                        DepEdge {
                            version_req: Some(dep_version.to_string()),
                            optional: false,
                        },
                    );
                }
            }
            continue;
        }

        // Lines inside require block
        if in_require_block {
            // Strip inline comments like "// indirect"
            let dep_part = if let Some(comment_pos) = trimmed.find("//") {
                trimmed[..comment_pos].trim()
            } else {
                trimmed
            };

            let parts: Vec<&str> = dep_part.split_whitespace().collect();
            if parts.len() >= 2 {
                let dep_name = parts[0];
                let dep_version = parts[1].strip_prefix('v').unwrap_or(parts[1]);
                let is_indirect = trimmed.contains("// indirect");

                let dep_idx = graph.add_node(DepNode {
                    name: dep_name.to_string(),
                    version: dep_version.to_string(),
                    ecosystem: "go".into(),
                    is_direct: !is_indirect,
                    is_dev: false,
                    license: None,
                    source: None,
                });
                if let Some(root) = root_idx {
                    graph.add_edge(
                        root,
                        dep_idx,
                        DepEdge {
                            version_req: Some(dep_version.to_string()),
                            optional: false,
                        },
                    );
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
    fn test_parse_go_sum_basic() {
        let content = r#"
github.com/gin-gonic/gin v1.9.1 h1:4+fr/el88TOO3ewCmQr8cx/CtZ/umlIRIs5M4NTNjf8=
github.com/gin-gonic/gin v1.9.1/go.mod h1:hPrL/0KcuGhE=
github.com/go-playground/validator/v10 v10.14.1 h1:9c50NUPC30ztFKKN7=
github.com/go-playground/validator/v10 v10.14.1/go.mod h1:GnVsG=
golang.org/x/net v0.10.0 h1:tnT7PkKs/b1bVJsJf48CX=
golang.org/x/net v0.10.0/go.mod h1:0fRc=
"#;

        let mut graph = DependencyGraph::new();
        parse_go_sum(content, &mut graph).unwrap();

        // Should have 3 packages (skipping /go.mod lines)
        assert_eq!(graph.package_count(), 3);
        assert!(graph.find_node("github.com/gin-gonic/gin").is_some());
        assert!(
            graph
                .find_node("github.com/go-playground/validator/v10")
                .is_some()
        );
        assert!(graph.find_node("golang.org/x/net").is_some());

        // Verify version stripping of "v" prefix
        let gin_idx = graph.find_node("github.com/gin-gonic/gin").unwrap();
        let gin_node = graph.get_node(gin_idx).unwrap();
        assert_eq!(gin_node.version, "1.9.1");
        assert_eq!(gin_node.ecosystem, "go");
    }

    #[test]
    fn test_parse_go_sum_deduplication() {
        let content = r#"
github.com/pkg/errors v0.9.1 h1:FEBLx1zS214owpjy7qsBeixbURkuhQAwrK5UwLGTwt4=
github.com/pkg/errors v0.9.1 h1:FEBLx1zS214owpjy7qsBeixbURkuhQAwrK5UwLGTwt4=
"#;

        let mut graph = DependencyGraph::new();
        parse_go_sum(content, &mut graph).unwrap();

        // Duplicate lines should be deduplicated
        assert_eq!(graph.package_count(), 1);
    }

    #[test]
    fn test_parse_go_sum_empty() {
        let content = "";
        let mut graph = DependencyGraph::new();
        parse_go_sum(content, &mut graph).unwrap();
        assert_eq!(graph.package_count(), 0);
    }

    #[test]
    fn test_parse_go_mod_with_require_block() {
        let content = r#"
module github.com/myorg/myapp

go 1.21

require (
	github.com/gin-gonic/gin v1.9.1
	github.com/go-sql-driver/mysql v1.7.1
	golang.org/x/net v0.10.0 // indirect
)
"#;

        let mut graph = DependencyGraph::new();
        parse_go_mod(content, &mut graph).unwrap();

        // module root + 3 deps = 4
        assert_eq!(graph.package_count(), 4);
        assert!(graph.find_node("github.com/myorg/myapp").is_some());
        assert!(graph.find_node("github.com/gin-gonic/gin").is_some());

        // indirect dep should not be marked as direct
        let net_idx = graph.find_node("golang.org/x/net").unwrap();
        let net_node = graph.get_node(net_idx).unwrap();
        assert!(!net_node.is_direct);

        // direct dep should be marked as direct
        let gin_idx = graph.find_node("github.com/gin-gonic/gin").unwrap();
        let gin_node = graph.get_node(gin_idx).unwrap();
        assert!(gin_node.is_direct);

        // Verify edges from root to deps
        let deps = graph.transitive_deps("github.com/myorg/myapp");
        assert_eq!(deps.len(), 3);
    }

    #[test]
    fn test_parse_go_mod_single_require() {
        let content = r#"
module example.com/hello

go 1.20

require github.com/pkg/errors v0.9.1
"#;

        let mut graph = DependencyGraph::new();
        parse_go_mod(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 2);
        assert!(graph.find_node("example.com/hello").is_some());
        assert!(graph.find_node("github.com/pkg/errors").is_some());
    }
}
