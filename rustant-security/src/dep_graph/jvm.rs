//! JVM dependency parsers (build.gradle, pom.xml).

use super::{DepEdge, DepNode, DependencyGraph};
use crate::error::DepGraphError;
use regex::Regex;

/// Parse a build.gradle file and populate the dependency graph.
///
/// Recognizes dependency declarations in the form:
/// ```text
/// implementation 'group:artifact:version'
/// testImplementation "group:artifact:version"
/// api 'group:artifact:version'
/// compileOnly "group:artifact:version"
/// runtimeOnly 'group:artifact:version'
/// ```
pub fn parse_build_gradle(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    // Match configuration lines like: implementation 'group:artifact:version'
    // Supports both single and double quotes, and various Gradle configurations.
    let dep_re = Regex::new(
        r#"(?m)^\s*(?:implementation|testImplementation|api|compileOnly|runtimeOnly|testCompileOnly|testRuntimeOnly|annotationProcessor|kapt)\s+['"]([^'"]+)['"]"#,
    )
    .map_err(|e| DepGraphError::LockfileParse {
        file: "build.gradle".into(),
        message: format!("regex error: {e}"),
    })?;

    // Also match Kotlin DSL style: implementation("group:artifact:version")
    let kotlin_dep_re = Regex::new(
        r#"(?m)^\s*(?:implementation|testImplementation|api|compileOnly|runtimeOnly|testCompileOnly|testRuntimeOnly|annotationProcessor|kapt)\s*\(\s*['"]([^'"]+)['"]\s*\)"#,
    )
    .map_err(|e| DepGraphError::LockfileParse {
        file: "build.gradle".into(),
        message: format!("regex error: {e}"),
    })?;

    // Configuration names that indicate test/dev dependencies
    let test_config_re = Regex::new(
        r#"(?m)^\s*(?:testImplementation|testCompileOnly|testRuntimeOnly)"#,
    )
    .map_err(|e| DepGraphError::LockfileParse {
        file: "build.gradle".into(),
        message: format!("regex error: {e}"),
    })?;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip comments
        if trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*') {
            continue;
        }

        // Try standard Groovy syntax
        let cap = dep_re
            .captures(line)
            .or_else(|| kotlin_dep_re.captures(line));

        if let Some(cap) = cap {
            let coord = &cap[1];
            let parts: Vec<&str> = coord.splitn(3, ':').collect();

            if parts.len() >= 2 {
                let group = parts[0];
                let artifact = parts[1];
                let version = if parts.len() >= 3 { parts[2] } else { "0.0.0" };
                let is_dev = test_config_re.is_match(line);

                let name = format!("{group}:{artifact}");

                graph.add_node(DepNode {
                    name,
                    version: version.to_string(),
                    ecosystem: "maven".into(),
                    is_direct: true,
                    is_dev,
                    license: None,
                    source: None,
                });
            }
        }
    }

    Ok(())
}

/// Parse a pom.xml file and populate the dependency graph.
///
/// Extracts `<dependency>` blocks containing `<groupId>`, `<artifactId>`, `<version>`,
/// and optional `<scope>`. Uses regex-based parsing (no XML crate).
pub fn parse_pom_xml(content: &str, graph: &mut DependencyGraph) -> Result<(), DepGraphError> {
    // Match <dependency>...</dependency> blocks (non-greedy)
    let dep_block_re = Regex::new(r"(?s)<dependency>(.*?)</dependency>").map_err(|e| {
        DepGraphError::LockfileParse {
            file: "pom.xml".into(),
            message: format!("regex error: {e}"),
        }
    })?;

    let group_re = Regex::new(r"<groupId>\s*(.*?)\s*</groupId>").map_err(|e| {
        DepGraphError::LockfileParse {
            file: "pom.xml".into(),
            message: format!("regex error: {e}"),
        }
    })?;

    let artifact_re = Regex::new(r"<artifactId>\s*(.*?)\s*</artifactId>").map_err(|e| {
        DepGraphError::LockfileParse {
            file: "pom.xml".into(),
            message: format!("regex error: {e}"),
        }
    })?;

    let version_re = Regex::new(r"<version>\s*(.*?)\s*</version>").map_err(|e| {
        DepGraphError::LockfileParse {
            file: "pom.xml".into(),
            message: format!("regex error: {e}"),
        }
    })?;

    let scope_re =
        Regex::new(r"<scope>\s*(.*?)\s*</scope>").map_err(|e| DepGraphError::LockfileParse {
            file: "pom.xml".into(),
            message: format!("regex error: {e}"),
        })?;

    // Find the project's own groupId/artifactId/version for creating a root node
    // (outside of <dependency> blocks)
    let mut root_idx = None;

    // Simple heuristic: look for top-level <groupId>, <artifactId>, <version>
    // that appear before the first <dependencies> section
    let before_deps = content
        .find("<dependencies>")
        .map(|pos| &content[..pos])
        .unwrap_or("");

    if !before_deps.is_empty() {
        let root_group = group_re
            .captures(before_deps)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let root_artifact = artifact_re
            .captures(before_deps)
            .map(|c| c[1].to_string())
            .unwrap_or_default();
        let root_version = version_re
            .captures(before_deps)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| "0.0.0".into());

        if !root_group.is_empty() && !root_artifact.is_empty() {
            root_idx = Some(graph.add_node(DepNode {
                name: format!("{root_group}:{root_artifact}"),
                version: root_version,
                ecosystem: "maven".into(),
                is_direct: true,
                is_dev: false,
                license: None,
                source: None,
            }));
        }
    }

    // Parse all <dependency> blocks
    for dep_cap in dep_block_re.captures_iter(content) {
        let block = &dep_cap[1];

        let group_id = match group_re.captures(block) {
            Some(c) => c[1].to_string(),
            None => continue,
        };

        let artifact_id = match artifact_re.captures(block) {
            Some(c) => c[1].to_string(),
            None => continue,
        };

        let version = version_re
            .captures(block)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| "0.0.0".into());

        let scope = scope_re
            .captures(block)
            .map(|c| c[1].to_string())
            .unwrap_or_else(|| "compile".into());

        let is_dev = scope == "test";

        let name = format!("{group_id}:{artifact_id}");

        let dep_idx = graph.add_node(DepNode {
            name,
            version: version.clone(),
            ecosystem: "maven".into(),
            is_direct: true,
            is_dev,
            license: None,
            source: None,
        });

        // Add edge from root to dependency if we have a root node
        if let Some(root) = root_idx {
            graph.add_edge(
                root,
                dep_idx,
                DepEdge {
                    version_req: Some(version),
                    optional: scope == "provided",
                },
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_build_gradle_basic() {
        let content = r#"
plugins {
    id 'java'
}

dependencies {
    implementation 'com.google.guava:guava:31.1-jre'
    testImplementation 'junit:junit:4.13.2'
    api 'org.apache.commons:commons-lang3:3.12.0'
    runtimeOnly 'org.postgresql:postgresql:42.6.0'
}
"#;

        let mut graph = DependencyGraph::new();
        parse_build_gradle(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 4);
        assert!(graph.find_node("com.google.guava:guava").is_some());
        assert!(graph.find_node("junit:junit").is_some());
        assert!(
            graph
                .find_node("org.apache.commons:commons-lang3")
                .is_some()
        );
        assert!(graph.find_node("org.postgresql:postgresql").is_some());

        // Verify test dependency
        let junit_idx = graph.find_node("junit:junit").unwrap();
        let junit_node = graph.get_node(junit_idx).unwrap();
        assert!(junit_node.is_dev);
        assert_eq!(junit_node.ecosystem, "maven");
        assert_eq!(junit_node.version, "4.13.2");

        // Verify production dependency
        let guava_idx = graph.find_node("com.google.guava:guava").unwrap();
        let guava_node = graph.get_node(guava_idx).unwrap();
        assert!(!guava_node.is_dev);
    }

    #[test]
    fn test_parse_build_gradle_kotlin_dsl() {
        let content = r#"
dependencies {
    implementation("org.jetbrains.kotlin:kotlin-stdlib:1.9.0")
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.0")
}
"#;

        let mut graph = DependencyGraph::new();
        parse_build_gradle(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 2);
        assert!(
            graph
                .find_node("org.jetbrains.kotlin:kotlin-stdlib")
                .is_some()
        );
        assert!(graph.find_node("org.junit.jupiter:junit-jupiter").is_some());
    }

    #[test]
    fn test_parse_build_gradle_double_quotes() {
        let content = r#"
dependencies {
    implementation "io.netty:netty-all:4.1.96.Final"
}
"#;

        let mut graph = DependencyGraph::new();
        parse_build_gradle(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 1);
        let idx = graph.find_node("io.netty:netty-all").unwrap();
        let node = graph.get_node(idx).unwrap();
        assert_eq!(node.version, "4.1.96.Final");
    }

    #[test]
    fn test_parse_pom_xml_basic() {
        let content = r#"<?xml version="1.0" encoding="UTF-8"?>
<project>
    <groupId>com.example</groupId>
    <artifactId>my-app</artifactId>
    <version>1.0.0</version>
    <packaging>jar</packaging>

    <dependencies>
        <dependency>
            <groupId>org.springframework</groupId>
            <artifactId>spring-core</artifactId>
            <version>6.0.11</version>
        </dependency>
        <dependency>
            <groupId>junit</groupId>
            <artifactId>junit</artifactId>
            <version>4.13.2</version>
            <scope>test</scope>
        </dependency>
        <dependency>
            <groupId>javax.servlet</groupId>
            <artifactId>javax.servlet-api</artifactId>
            <version>4.0.1</version>
            <scope>provided</scope>
        </dependency>
    </dependencies>
</project>"#;

        let mut graph = DependencyGraph::new();
        parse_pom_xml(content, &mut graph).unwrap();

        // Root node + 3 dependencies = 4
        assert_eq!(graph.package_count(), 4);
        assert!(graph.find_node("com.example:my-app").is_some());
        assert!(graph.find_node("org.springframework:spring-core").is_some());
        assert!(graph.find_node("junit:junit").is_some());
        assert!(graph.find_node("javax.servlet:javax.servlet-api").is_some());

        // Verify test scope is dev
        let junit_idx = graph.find_node("junit:junit").unwrap();
        let junit_node = graph.get_node(junit_idx).unwrap();
        assert!(junit_node.is_dev);

        // Verify compile scope is not dev
        let spring_idx = graph.find_node("org.springframework:spring-core").unwrap();
        let spring_node = graph.get_node(spring_idx).unwrap();
        assert!(!spring_node.is_dev);
        assert_eq!(spring_node.version, "6.0.11");

        // Verify edges from root
        let deps = graph.transitive_deps("com.example:my-app");
        assert_eq!(deps.len(), 3);
    }

    #[test]
    fn test_parse_pom_xml_no_version() {
        let content = r#"
<project>
    <dependencies>
        <dependency>
            <groupId>org.slf4j</groupId>
            <artifactId>slf4j-api</artifactId>
        </dependency>
    </dependencies>
</project>"#;

        let mut graph = DependencyGraph::new();
        parse_pom_xml(content, &mut graph).unwrap();

        assert_eq!(graph.package_count(), 1);
        let idx = graph.find_node("org.slf4j:slf4j-api").unwrap();
        let node = graph.get_node(idx).unwrap();
        assert_eq!(node.version, "0.0.0");
    }

    #[test]
    fn test_parse_pom_xml_empty() {
        let content = r#"<project></project>"#;

        let mut graph = DependencyGraph::new();
        parse_pom_xml(content, &mut graph).unwrap();
        assert_eq!(graph.package_count(), 0);
    }
}
