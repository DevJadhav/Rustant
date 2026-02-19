//! Python lockfile parsers (requirements.txt, poetry.lock).

use super::{DepNode, DependencyGraph};
use crate::error::DepGraphError;

/// Parse a requirements.txt file and populate the dependency graph.
pub fn parse_requirements_txt(
    content: &str,
    graph: &mut DependencyGraph,
) -> Result<(), DepGraphError> {
    for line in content.lines() {
        let trimmed = line.trim();

        // Skip empty lines, comments, and options
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }

        // Handle inline comments
        let trimmed = if let Some(idx) = trimmed.find('#') {
            trimmed[..idx].trim()
        } else {
            trimmed
        };

        // Skip URLs and path references
        if trimmed.starts_with("http://")
            || trimmed.starts_with("https://")
            || trimmed.starts_with('/')
            || trimmed.starts_with('.')
        {
            continue;
        }

        // Parse: package==version, package>=version, package~=version, package!=version, etc.
        let (name, version) = if let Some(pos) = trimmed.find("==") {
            (trimmed[..pos].trim(), trimmed[pos + 2..].trim())
        } else if let Some(pos) = trimmed.find(">=") {
            (trimmed[..pos].trim(), trimmed[pos + 2..].trim())
        } else if let Some(pos) = trimmed.find("<=") {
            (trimmed[..pos].trim(), trimmed[pos + 2..].trim())
        } else if let Some(pos) = trimmed.find("~=") {
            (trimmed[..pos].trim(), trimmed[pos + 2..].trim())
        } else if let Some(pos) = trimmed.find("!=") {
            (trimmed[..pos].trim(), trimmed[pos + 2..].trim())
        } else if let Some(pos) = trimmed.find('>') {
            (trimmed[..pos].trim(), trimmed[pos + 1..].trim())
        } else if let Some(pos) = trimmed.find('<') {
            (trimmed[..pos].trim(), trimmed[pos + 1..].trim())
        } else {
            (trimmed, "0.0.0")
        };

        // Strip extras like package[extra1,extra2]
        let name = if let Some(bracket_pos) = name.find('[') {
            &name[..bracket_pos]
        } else {
            name
        };

        // Strip environment markers like ; python_version >= "3.6"
        let version = if let Some(semi_pos) = version.find(';') {
            version[..semi_pos].trim()
        } else {
            version
        };

        // Strip comma-separated version constraints (take first)
        let version = if let Some(comma_pos) = version.find(',') {
            version[..comma_pos].trim()
        } else {
            version
        };

        if !name.is_empty() {
            graph.add_node(DepNode {
                name: name.to_lowercase(),
                version: version.to_string(),
                ecosystem: "pypi".into(),
                is_direct: true,
                is_dev: false,
                license: None,
                source: None,
            });
        }
    }

    Ok(())
}

/// Parse a poetry.lock file and populate the dependency graph.
pub fn parse_poetry_lock(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    let lock: toml::Value = toml::from_str(content).map_err(|e| DepGraphError::LockfileParse {
        file: "poetry.lock".into(),
        message: e.to_string(),
    })?;

    let packages = lock
        .get("package")
        .and_then(|p| p.as_array())
        .ok_or_else(|| DepGraphError::LockfileParse {
            file: "poetry.lock".into(),
            message: "No [[package]] entries found".into(),
        })?;

    // First pass: add all nodes
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
        let category = pkg
            .get("category")
            .and_then(|c| c.as_str())
            .unwrap_or("main");
        let is_dev = category == "dev";

        graph.add_node(DepNode {
            name: name.to_lowercase(),
            version,
            ecosystem: "pypi".into(),
            is_direct: false,
            is_dev,
            license: None,
            source: pkg
                .get("source")
                .and_then(|s| s.get("url"))
                .and_then(|u| u.as_str())
                .map(|s| s.to_string()),
        });
    }

    // Second pass: add edges from dependencies
    for pkg in packages {
        let name = pkg
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or("unknown")
            .to_lowercase();

        if let Some(from_idx) = graph.find_node(&name)
            && let Some(deps) = pkg.get("dependencies").and_then(|d| d.as_table())
        {
            for (dep_name, _version_req) in deps {
                let dep_name_lower = dep_name.to_lowercase();
                if let Some(to_idx) = graph.find_node(&dep_name_lower) {
                    graph.add_edge(
                        from_idx,
                        to_idx,
                        super::DepEdge {
                            version_req: None,
                            optional: false,
                        },
                    );
                }
            }
        }
    }

    Ok(())
}

/// Parse a Pipfile.lock (JSON format) and populate the dependency graph.
pub fn parse_pipfile_lock(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    let lock: serde_json::Value =
        serde_json::from_str(content).map_err(|e| DepGraphError::LockfileParse {
            file: "Pipfile.lock".into(),
            message: e.to_string(),
        })?;

    // Parse "default" (production) dependencies
    if let Some(defaults) = lock.get("default").and_then(|d| d.as_object()) {
        for (name, info) in defaults {
            let version = info
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0")
                .trim_start_matches("==")
                .to_string();
            graph.add_node(DepNode {
                name: name.to_lowercase(),
                version,
                ecosystem: "pypi".into(),
                is_direct: true,
                is_dev: false,
                license: None,
                source: info
                    .get("index")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }

    // Parse "develop" (dev) dependencies
    if let Some(develop) = lock.get("develop").and_then(|d| d.as_object()) {
        for (name, info) in develop {
            let version = info
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("0.0.0")
                .trim_start_matches("==")
                .to_string();
            graph.add_node(DepNode {
                name: name.to_lowercase(),
                version,
                ecosystem: "pypi".into(),
                is_direct: true,
                is_dev: true,
                license: None,
                source: info
                    .get("index")
                    .and_then(|i| i.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_requirements_txt() {
        let content = r#"
# This is a comment
requests==2.31.0
flask>=2.0.0
numpy~=1.24.0
pandas
# Another comment
boto3==1.28.0  # inline comment
"#;

        let mut graph = DependencyGraph::new();
        parse_requirements_txt(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 5);
        assert!(graph.find_node("requests").is_some());
        assert!(graph.find_node("flask").is_some());
        assert!(graph.find_node("numpy").is_some());
        assert!(graph.find_node("pandas").is_some());
        assert!(graph.find_node("boto3").is_some());
    }

    #[test]
    fn test_parse_requirements_txt_extras() {
        let content = "requests[security]==2.31.0\nuvicorn[standard]>=0.20.0\n";
        let mut graph = DependencyGraph::new();
        parse_requirements_txt(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 2);
        assert!(graph.find_node("requests").is_some());
        assert!(graph.find_node("uvicorn").is_some());
    }

    #[test]
    fn test_parse_poetry_lock() {
        let content = r#"
[[package]]
name = "requests"
version = "2.31.0"
category = "main"

[package.dependencies]
urllib3 = ">=1.21.1,<3"
certifi = ">=2017.4.17"

[[package]]
name = "urllib3"
version = "2.0.4"
category = "main"

[[package]]
name = "certifi"
version = "2023.7.22"
category = "main"

[[package]]
name = "pytest"
version = "7.4.0"
category = "dev"
"#;

        let mut graph = DependencyGraph::new();
        parse_poetry_lock(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 4);
        assert!(graph.find_node("requests").is_some());
        assert!(graph.find_node("urllib3").is_some());
        assert!(graph.find_node("certifi").is_some());
        assert!(graph.find_node("pytest").is_some());
    }

    #[test]
    fn test_parse_pipfile_lock() {
        let content = r#"{
    "_meta": {"hash": {"sha256": "abc"}, "pipfile-spec": 6},
    "default": {
        "requests": {"version": "==2.31.0"},
        "urllib3": {"version": "==2.0.4", "index": "pypi"},
        "certifi": {"version": "==2023.7.22"}
    },
    "develop": {
        "pytest": {"version": "==7.4.0"},
        "coverage": {"version": "==7.3.0"}
    }
}"#;
        let mut graph = DependencyGraph::new();
        parse_pipfile_lock(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 5);
        assert!(graph.find_node("requests").is_some());
        assert!(graph.find_node("pytest").is_some());
        assert!(graph.find_node("coverage").is_some());
    }

    #[test]
    fn test_parse_pipfile_lock_empty() {
        let content = r#"{"_meta": {}, "default": {}, "develop": {}}"#;
        let mut graph = DependencyGraph::new();
        parse_pipfile_lock(content, &mut graph).unwrap();
        assert_eq!(graph.package_count(), 0);
    }
}
