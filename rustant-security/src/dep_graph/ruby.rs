//! Ruby lockfile parser (Gemfile.lock).

use super::{DepEdge, DepNode, DependencyGraph};
use crate::error::DepGraphError;

/// Parse a Gemfile.lock file and populate the dependency graph.
///
/// Gemfile.lock format:
/// ```text
/// GEM
///   remote: https://rubygems.org/
///   specs:
///     rails (7.0.0)
///       actioncable (= 7.0.0)
///       actionmailer (= 7.0.0)
///     actioncable (7.0.0)
///       websocket-driver (>= 0.6.1)
///     websocket-driver (0.7.5)
/// ```
///
/// Indented entries under `specs:` (4 spaces) are packages with versions in parens.
/// Sub-indented entries (6+ spaces) are dependencies of the preceding package.
pub fn parse_gemfile_lock(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    let mut in_gem_section = false;
    let mut in_specs = false;
    let mut node_map = std::collections::HashMap::new();

    // First pass: collect all packages and their versions
    for line in content.lines() {
        // Detect section boundaries
        if !line.starts_with(' ') && !line.is_empty() {
            if line.trim() == "GEM" {
                in_gem_section = true;
                in_specs = false;
            } else {
                in_gem_section = false;
                in_specs = false;
            }
            continue;
        }

        if !in_gem_section {
            continue;
        }

        let trimmed = line.trim();

        if trimmed == "specs:" {
            in_specs = true;
            continue;
        }

        if !in_specs {
            continue;
        }

        // Count leading spaces to determine indentation level
        let indent = line.len() - line.trim_start().len();

        // Package line (4 spaces): "    rails (7.0.0)"
        if indent == 4
            && let Some((name, version)) = parse_gem_entry(trimmed)
        {
            let idx = graph.add_node(DepNode {
                name: name.clone(),
                version,
                ecosystem: "rubygems".into(),
                is_direct: false,
                is_dev: false,
                license: None,
                source: None,
            });
            node_map.insert(name, idx);
        }
    }

    // Second pass: collect dependency edges
    in_gem_section = false;
    in_specs = false;
    let mut current_pkg_name: Option<String> = None;

    for line in content.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            if line.trim() == "GEM" {
                in_gem_section = true;
                in_specs = false;
            } else {
                in_gem_section = false;
                in_specs = false;
            }
            current_pkg_name = None;
            continue;
        }

        if !in_gem_section {
            continue;
        }

        let trimmed = line.trim();

        if trimmed == "specs:" {
            in_specs = true;
            continue;
        }

        if !in_specs {
            continue;
        }

        let indent = line.len() - line.trim_start().len();

        // Package line (4 spaces)
        if indent == 4 {
            if let Some((name, _version)) = parse_gem_entry(trimmed) {
                current_pkg_name = Some(name);
            } else {
                current_pkg_name = None;
            }
        }
        // Dependency line (6+ spaces)
        else if indent >= 6
            && let Some(ref parent_name) = current_pkg_name
        {
            // Parse dependency entry, e.g., "actioncable (= 7.0.0)" or "websocket-driver (>= 0.6.1)"
            let dep_name = extract_gem_name(trimmed);
            let version_req = extract_gem_version_req(trimmed);

            if let (Some(&from_idx), Some(&to_idx)) =
                (node_map.get(parent_name), node_map.get(&dep_name))
            {
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
    }

    // Mark gems listed in DEPENDENCIES section as direct
    let mut in_dependencies = false;
    for line in content.lines() {
        if !line.starts_with(' ') && !line.is_empty() {
            in_dependencies = line.trim() == "DEPENDENCIES";
            continue;
        }

        if in_dependencies {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let dep_name = extract_gem_name(trimmed);
            if let Some(&idx) = node_map.get(&dep_name)
                && let Some(node) = graph.get_node_mut(idx)
            {
                node.is_direct = true;
            }
        }
    }

    Ok(())
}

/// Parse a gem entry like "rails (7.0.0)" into (name, version).
fn parse_gem_entry(entry: &str) -> Option<(String, String)> {
    let trimmed = entry.trim();

    // Find the version in parentheses
    if let Some(paren_start) = trimmed.find('(') {
        let name = trimmed[..paren_start].trim().to_string();
        let version = trimmed[paren_start + 1..]
            .trim_end_matches(')')
            .trim()
            .to_string();

        if !name.is_empty() {
            return Some((name, version));
        }
    }

    // No version in parens — just a name
    if !trimmed.is_empty() {
        return Some((trimmed.to_string(), "0.0.0".to_string()));
    }

    None
}

/// Extract the gem name from a dependency line like "actioncable (= 7.0.0)".
fn extract_gem_name(entry: &str) -> String {
    let trimmed = entry.trim();
    if let Some(paren_start) = trimmed.find('(') {
        trimmed[..paren_start].trim().to_string()
    } else if let Some(space_pos) = trimmed.find(' ') {
        trimmed[..space_pos].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Extract a version requirement string from a gem dependency line.
fn extract_gem_version_req(entry: &str) -> Option<String> {
    let trimmed = entry.trim();
    if let Some(paren_start) = trimmed.find('(') {
        let req = trimmed[paren_start + 1..]
            .trim_end_matches(')')
            .trim()
            .to_string();
        if !req.is_empty() {
            return Some(req);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gemfile_lock_basic() {
        let content = r#"GEM
  remote: https://rubygems.org/
  specs:
    rails (7.0.0)
      actioncable (= 7.0.0)
      activesupport (= 7.0.0)
    actioncable (7.0.0)
      websocket-driver (>= 0.6.1)
    activesupport (7.0.0)
    websocket-driver (0.7.5)

PLATFORMS
  ruby

DEPENDENCIES
  rails (~> 7.0)

BUNDLED WITH
   2.4.0
"#;

        let mut graph = DependencyGraph::new();
        parse_gemfile_lock(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 4);
        assert!(graph.find_node("rails").is_some());
        assert!(graph.find_node("actioncable").is_some());
        assert!(graph.find_node("activesupport").is_some());
        assert!(graph.find_node("websocket-driver").is_some());

        // Verify version
        let rails_idx = graph.find_node("rails").unwrap();
        let rails_node = graph.get_node(rails_idx).unwrap();
        assert_eq!(rails_node.version, "7.0.0");
        assert_eq!(rails_node.ecosystem, "rubygems");

        // Verify edges: rails -> actioncable, rails -> activesupport
        let deps = graph.transitive_deps("rails");
        assert!(deps.len() >= 2);
        let dep_names: Vec<&str> = deps.iter().map(|d| d.name.as_str()).collect();
        assert!(dep_names.contains(&"actioncable"));
        assert!(dep_names.contains(&"activesupport"));

        // actioncable -> websocket-driver
        let ac_deps = graph.transitive_deps("actioncable");
        assert_eq!(ac_deps.len(), 1);
        assert_eq!(ac_deps[0].name, "websocket-driver");
    }

    #[test]
    fn test_parse_gemfile_lock_multiple_sections() {
        let content = r#"GIT
  remote: https://github.com/example/repo.git
  revision: abc123
  specs:
    custom-gem (0.1.0)

GEM
  remote: https://rubygems.org/
  specs:
    rack (2.2.7)
    sinatra (3.0.6)
      rack (~> 2.2)

PLATFORMS
  ruby

DEPENDENCIES
  sinatra

BUNDLED WITH
   2.4.0
"#;

        let mut graph = DependencyGraph::new();
        parse_gemfile_lock(content, &mut graph).unwrap();

        // Should only parse GEM section specs, not GIT section
        // GIT section has 4-space indent under specs too, but is not GEM
        assert_eq!(graph.package_count(), 2);
        assert!(graph.find_node("rack").is_some());
        assert!(graph.find_node("sinatra").is_some());

        // Verify edge: sinatra -> rack
        let deps = graph.transitive_deps("sinatra");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "rack");
    }

    #[test]
    fn test_parse_gemfile_lock_empty() {
        let content = r#"GEM
  remote: https://rubygems.org/
  specs:

PLATFORMS
  ruby

DEPENDENCIES

BUNDLED WITH
   2.4.0
"#;

        let mut graph = DependencyGraph::new();
        parse_gemfile_lock(content, &mut graph).unwrap();
        assert_eq!(graph.package_count(), 0);
    }
}
