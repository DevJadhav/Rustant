//! Project template library for scaffolding new projects.
//!
//! Provides templates for popular frameworks: React+Vite, Next.js, FastAPI, Axum,
//! SvelteKit, and Express.

pub mod express;
pub mod fastapi;
pub mod nextjs;
pub mod react_vite;
pub mod rust_axum;
pub mod sveltekit;

/// A single file in a project template.
#[derive(Debug, Clone)]
pub struct TemplateFile {
    /// Relative path from project root.
    pub path: String,
    /// File content (may contain `{{variable}}` handlebars placeholders).
    pub content: String,
}

/// A complete project template.
#[derive(Debug, Clone)]
pub struct ProjectTemplate {
    /// Template identifier.
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Framework/language.
    pub framework: String,
    /// Files to create.
    pub files: Vec<TemplateFile>,
    /// Commands to run after file creation (e.g., `npm install`).
    pub post_install: Vec<String>,
}

/// Summary info for listing templates.
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    pub name: String,
    pub description: String,
    pub framework: String,
}

/// Get a project template by name.
pub fn get_template(name: &str) -> Option<ProjectTemplate> {
    match name {
        "react-vite" | "react" | "vite" => Some(react_vite::template()),
        "nextjs" | "next" => Some(nextjs::template()),
        "fastapi" | "python" => Some(fastapi::template()),
        "axum" | "rust" | "rust-axum" => Some(rust_axum::template()),
        "sveltekit" | "svelte" => Some(sveltekit::template()),
        "express" | "node" | "express-ts" => Some(express::template()),
        _ => None,
    }
}

/// List all available templates.
pub fn list_templates() -> Vec<TemplateInfo> {
    vec![
        TemplateInfo {
            name: "react-vite".into(),
            description: "React + Vite + Tailwind CSS".into(),
            framework: "React".into(),
        },
        TemplateInfo {
            name: "nextjs".into(),
            description: "Next.js 14 App Router + Tailwind CSS".into(),
            framework: "Next.js".into(),
        },
        TemplateInfo {
            name: "fastapi".into(),
            description: "FastAPI + SQLAlchemy + Alembic".into(),
            framework: "FastAPI".into(),
        },
        TemplateInfo {
            name: "axum".into(),
            description: "Axum web service + SQLx + Tokio".into(),
            framework: "Axum".into(),
        },
        TemplateInfo {
            name: "sveltekit".into(),
            description: "SvelteKit + Tailwind CSS".into(),
            framework: "SvelteKit".into(),
        },
        TemplateInfo {
            name: "express".into(),
            description: "Express.js + TypeScript + Prisma".into(),
            framework: "Express".into(),
        },
    ]
}

/// Apply variable substitution to template content.
pub fn apply_variables(content: &str, project_name: &str) -> String {
    content
        .replace("{{project_name}}", project_name)
        .replace("{{project-name}}", &project_name.replace('_', "-"))
        .replace("{{ProjectName}}", &to_pascal_case(project_name))
}

fn to_pascal_case(s: &str) -> String {
    s.split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + &chars.as_str().to_lowercase(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_templates() {
        let templates = list_templates();
        assert_eq!(templates.len(), 6);
    }

    #[test]
    fn test_get_template_by_name() {
        assert!(get_template("react-vite").is_some());
        assert!(get_template("react").is_some());
        assert!(get_template("nextjs").is_some());
        assert!(get_template("fastapi").is_some());
        assert!(get_template("axum").is_some());
        assert!(get_template("sveltekit").is_some());
        assert!(get_template("express").is_some());
        assert!(get_template("nonexistent").is_none());
    }

    #[test]
    fn test_apply_variables() {
        let content = "name: {{project_name}}\nclass: {{ProjectName}}";
        let result = apply_variables(content, "my_app");
        assert!(result.contains("name: my_app"));
        assert!(result.contains("class: MyApp"));
    }

    #[test]
    fn test_pascal_case() {
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
        assert_eq!(to_pascal_case("my-app"), "MyApp");
        assert_eq!(to_pascal_case("single"), "Single");
    }

    #[test]
    fn test_all_templates_have_files() {
        for info in list_templates() {
            let template = get_template(&info.name).unwrap();
            assert!(
                !template.files.is_empty(),
                "Template {} has no files",
                info.name
            );
        }
    }
}
