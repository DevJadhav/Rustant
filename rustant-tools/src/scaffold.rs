//! Scaffold tool — project initialization and component generation.
//!
//! Uses project detection + handlebars templates to scaffold new projects,
//! add components, routes, and other framework-specific structures.

use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::project_detect::{ProjectType, detect_project};
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::path::{Path, PathBuf};

pub struct ScaffoldTool {
    workspace: PathBuf,
}

impl ScaffoldTool {
    pub fn new(workspace: PathBuf) -> Self {
        Self { workspace }
    }
}

#[async_trait]
impl crate::registry::Tool for ScaffoldTool {
    fn name(&self) -> &str {
        "scaffold"
    }

    fn description(&self) -> &str {
        "Scaffold new projects, components, routes, and other structures. Supports React, Next.js, FastAPI, Axum, SvelteKit, Express, and more."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["create_project", "add_component", "add_route", "list_templates"],
                    "description": "The scaffolding action to perform"
                },
                "template": {
                    "type": "string",
                    "description": "Template name (e.g. 'react-vite', 'nextjs', 'fastapi', 'rust-axum', 'sveltekit', 'express')"
                },
                "name": {
                    "type": "string",
                    "description": "Name for the project or component"
                },
                "target_dir": {
                    "type": "string",
                    "description": "Target directory (defaults to workspace)"
                },
                "options": {
                    "type": "object",
                    "description": "Additional template options (e.g. {'tailwind': true, 'typescript': true})"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = args.get("action").and_then(|v| v.as_str()).ok_or_else(|| {
            ToolError::InvalidArguments {
                name: "scaffold".to_string(),
                reason: "Missing 'action' parameter".to_string(),
            }
        })?;

        match action {
            "list_templates" => list_templates(),
            "create_project" => {
                let template = args
                    .get("template")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| ToolError::InvalidArguments {
                        name: "scaffold".to_string(),
                        reason: "Missing 'template' parameter for create_project".to_string(),
                    })?;
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "scaffold".to_string(),
                        reason: "Missing 'name' parameter for create_project".to_string(),
                    }
                })?;
                let target = args
                    .get("target_dir")
                    .and_then(|v| v.as_str())
                    .map(PathBuf::from)
                    .unwrap_or_else(|| self.workspace.join(name));
                let options = args.get("options").cloned().unwrap_or(json!({}));
                create_project(template, name, &target, &options).await
            }
            "add_component" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "scaffold".to_string(),
                        reason: "Missing 'name' parameter for add_component".to_string(),
                    }
                })?;
                let options = args.get("options").cloned().unwrap_or(json!({}));
                add_component(&self.workspace, name, &options).await
            }
            "add_route" => {
                let name = args.get("name").and_then(|v| v.as_str()).ok_or_else(|| {
                    ToolError::InvalidArguments {
                        name: "scaffold".to_string(),
                        reason: "Missing 'name' parameter for add_route".to_string(),
                    }
                })?;
                let options = args.get("options").cloned().unwrap_or(json!({}));
                add_route(&self.workspace, name, &options).await
            }
            _ => Err(ToolError::InvalidArguments {
                name: "scaffold".to_string(),
                reason: format!("Unknown scaffold action: {action}"),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    fn timeout(&self) -> std::time::Duration {
        std::time::Duration::from_secs(60)
    }
}

fn list_templates() -> Result<ToolOutput, ToolError> {
    let templates = vec![
        (
            "react-vite",
            "React + Vite + Tailwind CSS",
            "TypeScript SPA with fast HMR",
        ),
        (
            "nextjs",
            "Next.js 14 App Router",
            "Full-stack React with SSR/SSG",
        ),
        (
            "fastapi",
            "FastAPI + SQLAlchemy",
            "Python async API with ORM",
        ),
        (
            "rust-axum",
            "Axum web service",
            "Rust async web server with tokio",
        ),
        ("sveltekit", "SvelteKit", "Full-stack Svelte framework"),
        ("express", "Express.js + TypeScript", "Node.js REST API"),
    ];

    let mut output = String::from("Available project templates:\n\n");
    for (id, name, desc) in &templates {
        output.push_str(&format!("  {id:<16} {name}\n"));
        output.push_str(&format!("  {:<16} {desc}\n\n", ""));
    }

    Ok(ToolOutput::text(output))
}

async fn create_project(
    template: &str,
    name: &str,
    target: &Path,
    _options: &serde_json::Value,
) -> Result<ToolOutput, ToolError> {
    // Validate template exists
    let valid = [
        "react-vite",
        "nextjs",
        "fastapi",
        "rust-axum",
        "sveltekit",
        "express",
    ];
    if !valid.contains(&template) {
        return Err(ToolError::InvalidArguments {
            name: "scaffold".to_string(),
            reason: format!(
                "Unknown template '{template}'. Available: {}",
                valid.join(", ")
            ),
        });
    }

    // Create target directory
    if target.exists() {
        return Err(ToolError::ExecutionFailed {
            name: "scaffold".to_string(),
            message: format!("Target directory already exists: {}", target.display()),
        });
    }

    tokio::fs::create_dir_all(target)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: "scaffold".to_string(),
            message: format!("Failed to create directory: {e}"),
        })?;

    // Generate project files from template
    let files_created = generate_project_files(template, name, target).await?;

    Ok(ToolOutput::text(format!(
        "Created project '{name}' from template '{template}' at {}\n\
         Files created: {files_created}\n\n\
         Next steps:\n\
         {}",
        target.display(),
        next_steps(template)
    )))
}

async fn generate_project_files(
    template: &str,
    name: &str,
    target: &Path,
) -> Result<usize, ToolError> {
    let mut count = 0;

    match template {
        "react-vite" => {
            write_file(target, "package.json", &react_vite_package_json(name)).await?;
            write_file(target, "vite.config.ts", REACT_VITE_CONFIG).await?;
            write_file(target, "tsconfig.json", REACT_TSCONFIG).await?;
            write_file(target, "index.html", &react_index_html(name)).await?;
            write_file(target, "tailwind.config.js", TAILWIND_CONFIG).await?;
            write_file(target, "postcss.config.js", POSTCSS_CONFIG).await?;
            tokio::fs::create_dir_all(target.join("src"))
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "scaffold".to_string(),
                    message: format!("Failed to create src dir: {e}"),
                })?;
            write_file(target, "src/main.tsx", REACT_MAIN_TSX).await?;
            write_file(target, "src/App.tsx", REACT_APP_TSX).await?;
            write_file(target, "src/index.css", REACT_INDEX_CSS).await?;
            write_file(target, ".gitignore", NODE_GITIGNORE).await?;
            count = 10;
        }
        "nextjs" => {
            write_file(target, "package.json", &nextjs_package_json(name)).await?;
            write_file(target, "next.config.js", NEXTJS_CONFIG).await?;
            write_file(target, "tsconfig.json", NEXTJS_TSCONFIG).await?;
            tokio::fs::create_dir_all(target.join("app"))
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "scaffold".to_string(),
                    message: format!("Failed to create app dir: {e}"),
                })?;
            write_file(target, "app/layout.tsx", &nextjs_layout(name)).await?;
            write_file(target, "app/page.tsx", NEXTJS_PAGE).await?;
            write_file(target, "app/globals.css", NEXTJS_GLOBALS_CSS).await?;
            write_file(target, "tailwind.config.ts", NEXTJS_TAILWIND_CONFIG).await?;
            write_file(target, "postcss.config.js", POSTCSS_CONFIG).await?;
            write_file(target, ".gitignore", NODE_GITIGNORE).await?;
            count = 9;
        }
        "fastapi" => {
            write_file(target, "requirements.txt", FASTAPI_REQUIREMENTS).await?;
            write_file(target, "pyproject.toml", &fastapi_pyproject(name)).await?;
            tokio::fs::create_dir_all(target.join("app"))
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "scaffold".to_string(),
                    message: format!("Failed to create app dir: {e}"),
                })?;
            write_file(target, "app/__init__.py", "").await?;
            write_file(target, "app/main.py", FASTAPI_MAIN).await?;
            write_file(target, "app/models.py", FASTAPI_MODELS).await?;
            write_file(target, "app/database.py", FASTAPI_DATABASE).await?;
            write_file(target, "alembic.ini", &fastapi_alembic_ini(name)).await?;
            write_file(target, ".gitignore", PYTHON_GITIGNORE).await?;
            count = 8;
        }
        "rust-axum" => {
            write_file(target, "Cargo.toml", &axum_cargo_toml(name)).await?;
            tokio::fs::create_dir_all(target.join("src"))
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "scaffold".to_string(),
                    message: format!("Failed to create src dir: {e}"),
                })?;
            write_file(target, "src/main.rs", AXUM_MAIN_RS).await?;
            write_file(target, "src/routes.rs", AXUM_ROUTES_RS).await?;
            write_file(target, "src/models.rs", AXUM_MODELS_RS).await?;
            write_file(target, ".gitignore", RUST_GITIGNORE).await?;
            count = 5;
        }
        "sveltekit" => {
            write_file(target, "package.json", &sveltekit_package_json(name)).await?;
            write_file(target, "svelte.config.js", SVELTEKIT_CONFIG).await?;
            write_file(target, "vite.config.ts", SVELTEKIT_VITE_CONFIG).await?;
            write_file(target, "tsconfig.json", SVELTEKIT_TSCONFIG).await?;
            tokio::fs::create_dir_all(target.join("src/routes"))
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "scaffold".to_string(),
                    message: format!("Failed to create routes dir: {e}"),
                })?;
            write_file(target, "src/app.html", SVELTEKIT_APP_HTML).await?;
            write_file(target, "src/routes/+page.svelte", SVELTEKIT_PAGE).await?;
            write_file(target, ".gitignore", NODE_GITIGNORE).await?;
            count = 7;
        }
        "express" => {
            write_file(target, "package.json", &express_package_json(name)).await?;
            write_file(target, "tsconfig.json", EXPRESS_TSCONFIG).await?;
            tokio::fs::create_dir_all(target.join("src"))
                .await
                .map_err(|e| ToolError::ExecutionFailed {
                    name: "scaffold".to_string(),
                    message: format!("Failed to create src dir: {e}"),
                })?;
            write_file(target, "src/index.ts", EXPRESS_INDEX_TS).await?;
            write_file(target, "src/routes.ts", EXPRESS_ROUTES_TS).await?;
            write_file(target, ".gitignore", NODE_GITIGNORE).await?;
            count = 5;
        }
        _ => {}
    }

    Ok(count)
}

async fn write_file(base: &Path, rel: &str, content: &str) -> Result<(), ToolError> {
    let path = base.join(rel);
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: "scaffold".to_string(),
                message: format!("Failed to create directory {}: {e}", parent.display()),
            })?;
    }
    tokio::fs::write(&path, content)
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: "scaffold".to_string(),
            message: format!("Failed to write {}: {e}", path.display()),
        })
}

async fn add_component(
    workspace: &Path,
    name: &str,
    options: &serde_json::Value,
) -> Result<ToolOutput, ToolError> {
    let project = detect_project(workspace);
    let typescript = options
        .get("typescript")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let (file_path, content) = match project.project_type {
        ProjectType::Node => {
            let ext = if typescript { "tsx" } else { "jsx" };
            let dir = if workspace.join("src/components").exists() {
                "src/components"
            } else {
                "components"
            };
            let path = format!("{dir}/{name}.{ext}");
            let content = format!(
                r#"export default function {name}() {{
  return (
    <div>
      <h2>{name}</h2>
    </div>
  );
}}
"#
            );
            (path, content)
        }
        ProjectType::Python => {
            let snake = to_snake_case(name);
            let path = format!("app/{snake}.py");
            let content = format!(
                r#""""Module for {name}."""


class {name}:
    """TODO: Add description."""

    def __init__(self):
        pass
"#
            );
            (path, content)
        }
        ProjectType::Rust => {
            let snake = to_snake_case(name);
            let path = format!("src/{snake}.rs");
            let content = format!(
                r#"//! Module for {name}.

pub struct {name} {{
    // TODO: Add fields
}}

impl {name} {{
    pub fn new() -> Self {{
        Self {{}}
    }}
}}
"#
            );
            (path, content)
        }
        _ => {
            return Err(ToolError::ExecutionFailed {
                name: "scaffold".to_string(),
                message: format!(
                    "Cannot detect project type for component generation. Detected: {:?}",
                    project.project_type
                ),
            });
        }
    };

    let full_path = workspace.join(&file_path);
    write_file(workspace, &file_path, &content).await?;

    Ok(ToolOutput::text(format!(
        "Created component '{name}' at {}\n({} bytes)",
        full_path.display(),
        content.len()
    )))
}

async fn add_route(
    workspace: &Path,
    name: &str,
    options: &serde_json::Value,
) -> Result<ToolOutput, ToolError> {
    let project = detect_project(workspace);
    let method = options
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("get");

    let (file_path, content) = match project.project_type {
        ProjectType::Node => {
            if workspace.join("app").exists() {
                // Next.js App Router
                let dir = format!("app/{name}");
                let path = format!("{dir}/page.tsx");
                let pascal = to_pascal_case(name);
                let content = format!(
                    r#"export default function {pascal}Page() {{
  return (
    <div>
      <h1>{pascal}</h1>
    </div>
  );
}}
"#
                );
                (path, content)
            } else {
                let path = format!("src/routes/{name}.ts");
                let content = format!(
                    r#"import {{ Router }} from 'express';

const router = Router();

router.{method}('/', (req, res) => {{
  res.json({{ message: '{name} route' }});
}});

export default router;
"#
                );
                (path, content)
            }
        }
        ProjectType::Python => {
            let snake = to_snake_case(name);
            let path = format!("app/routes/{snake}.py");
            let content = format!(
                r#"from fastapi import APIRouter

router = APIRouter(prefix="/{snake}", tags=["{snake}"])


@router.{method}("/")
async def {method}_{snake}():
    return {{"message": "{name} route"}}
"#
            );
            (path, content)
        }
        ProjectType::Rust => {
            let snake = to_snake_case(name);
            let path = format!("src/routes/{snake}.rs");
            let content = format!(
                r#"use axum::{{Router, routing::{method}, Json}};
use serde_json::{{json, Value}};

pub fn router() -> Router {{
    Router::new().route("/{snake}", {method}(handle_{snake}))
}}

async fn handle_{snake}() -> Json<Value> {{
    Json(json!({{ "message": "{name} route" }}))
}}
"#
            );
            (path, content)
        }
        _ => {
            return Err(ToolError::ExecutionFailed {
                name: "scaffold".to_string(),
                message: format!(
                    "Cannot detect project type for route generation. Detected: {:?}",
                    project.project_type
                ),
            });
        }
    };

    write_file(workspace, &file_path, &content).await?;

    Ok(ToolOutput::text(format!(
        "Created route '{name}' ({method}) at {}",
        workspace.join(&file_path).display()
    )))
}

fn next_steps(template: &str) -> &'static str {
    match template {
        "react-vite" => "  cd <project> && npm install && npm run dev",
        "nextjs" => "  cd <project> && npm install && npm run dev",
        "fastapi" => {
            "  cd <project> && pip install -r requirements.txt && uvicorn app.main:app --reload"
        }
        "rust-axum" => "  cd <project> && cargo run",
        "sveltekit" => "  cd <project> && npm install && npm run dev",
        "express" => "  cd <project> && npm install && npm run dev",
        _ => "  cd <project>",
    }
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            result.push('_');
        }
        result.push(c.to_lowercase().next().unwrap_or(c));
    }
    result
}

fn to_pascal_case(s: &str) -> String {
    s.split(&['-', '_', ' '])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
            }
        })
        .collect()
}

// ── Template constants ──

fn react_vite_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {{
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview"
  }},
  "dependencies": {{
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  }},
  "devDependencies": {{
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0",
    "@vitejs/plugin-react": "^4.2.0",
    "autoprefixer": "^10.4.16",
    "postcss": "^8.4.32",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.3.0",
    "vite": "^5.0.0"
  }}
}}"#
    )
}

static REACT_VITE_CONFIG: &str = r#"import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
});
"#;

static REACT_TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true
  },
  "include": ["src"]
}
"#;

fn react_index_html(name: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{name}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
"#
    )
}

static TAILWIND_CONFIG: &str = r#"/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: { extend: {} },
  plugins: [],
};
"#;

static POSTCSS_CONFIG: &str = r#"export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
};
"#;

static REACT_MAIN_TSX: &str = r#"import React from 'react';
import ReactDOM from 'react-dom/client';
import App from './App';
import './index.css';

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
"#;

static REACT_APP_TSX: &str = r#"function App() {
  return (
    <div className="min-h-screen bg-gray-100 flex items-center justify-center">
      <div className="text-center">
        <h1 className="text-4xl font-bold text-gray-900">Welcome</h1>
        <p className="mt-4 text-gray-600">Edit src/App.tsx to get started.</p>
      </div>
    </div>
  );
}

export default App;
"#;

static REACT_INDEX_CSS: &str = r#"@tailwind base;
@tailwind components;
@tailwind utilities;
"#;

static NODE_GITIGNORE: &str = r#"node_modules/
dist/
.env
.env.local
*.log
"#;

// ── Next.js templates ──

fn nextjs_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "dev": "next dev",
    "build": "next build",
    "start": "next start",
    "lint": "next lint"
  }},
  "dependencies": {{
    "next": "^14.0.0",
    "react": "^18.2.0",
    "react-dom": "^18.2.0"
  }},
  "devDependencies": {{
    "@types/node": "^20.10.0",
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0",
    "autoprefixer": "^10.4.16",
    "postcss": "^8.4.32",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.3.0"
  }}
}}"#
    )
}

static NEXTJS_CONFIG: &str = r#"/** @type {import('next').NextConfig} */
const nextConfig = {};
module.exports = nextConfig;
"#;

static NEXTJS_TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "es5",
    "lib": ["dom", "dom.iterable", "esnext"],
    "allowJs": true,
    "skipLibCheck": true,
    "strict": true,
    "noEmit": true,
    "esModuleInterop": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "jsx": "preserve",
    "incremental": true,
    "plugins": [{ "name": "next" }],
    "paths": { "@/*": ["./*"] }
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
  "exclude": ["node_modules"]
}
"#;

fn nextjs_layout(name: &str) -> String {
    format!(
        r#"import type {{ Metadata }} from 'next';
import './globals.css';

export const metadata: Metadata = {{
  title: '{name}',
  description: 'Generated by Rustant',
}};

export default function RootLayout({{ children }}: {{ children: React.ReactNode }}) {{
  return (
    <html lang="en">
      <body>{{children}}</body>
    </html>
  );
}}
"#
    )
}

static NEXTJS_PAGE: &str = r#"export default function Home() {
  return (
    <main className="min-h-screen flex items-center justify-center">
      <h1 className="text-4xl font-bold">Welcome</h1>
    </main>
  );
}
"#;

static NEXTJS_GLOBALS_CSS: &str = r#"@tailwind base;
@tailwind components;
@tailwind utilities;
"#;

static NEXTJS_TAILWIND_CONFIG: &str = r#"import type { Config } from 'tailwindcss';

const config: Config = {
  content: ['./app/**/*.{js,ts,jsx,tsx,mdx}', './components/**/*.{js,ts,jsx,tsx,mdx}'],
  theme: { extend: {} },
  plugins: [],
};
export default config;
"#;

// ── FastAPI templates ──

static FASTAPI_REQUIREMENTS: &str = r#"fastapi>=0.104.0
uvicorn[standard]>=0.24.0
sqlalchemy>=2.0.0
alembic>=1.13.0
pydantic>=2.5.0
python-dotenv>=1.0.0
"#;

fn fastapi_pyproject(name: &str) -> String {
    format!(
        r#"[project]
name = "{name}"
version = "0.1.0"
requires-python = ">=3.11"
"#
    )
}

static FASTAPI_MAIN: &str = r#"from fastapi import FastAPI
from app.database import engine
from app import models

models.Base.metadata.create_all(bind=engine)

app = FastAPI(title="API", version="0.1.0")


@app.get("/")
async def root():
    return {"message": "Hello, World!"}


@app.get("/health")
async def health():
    return {"status": "ok"}
"#;

static FASTAPI_MODELS: &str = r#"from sqlalchemy import Column, Integer, String, DateTime
from sqlalchemy.sql import func
from app.database import Base


class Item(Base):
    __tablename__ = "items"

    id = Column(Integer, primary_key=True, index=True)
    name = Column(String, index=True)
    description = Column(String, nullable=True)
    created_at = Column(DateTime(timezone=True), server_default=func.now())
"#;

static FASTAPI_DATABASE: &str = r#"from sqlalchemy import create_engine
from sqlalchemy.orm import sessionmaker, declarative_base

SQLALCHEMY_DATABASE_URL = "sqlite:///./app.db"

engine = create_engine(
    SQLALCHEMY_DATABASE_URL, connect_args={"check_same_thread": False}
)
SessionLocal = sessionmaker(autocommit=False, autoflush=False, bind=engine)

Base = declarative_base()


def get_db():
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()
"#;

fn fastapi_alembic_ini(_name: &str) -> String {
    r#"[alembic]
script_location = alembic
sqlalchemy.url = sqlite:///./app.db

[loggers]
keys = root,sqlalchemy,alembic

[logger_root]
level = WARN
handlers = console

[logger_sqlalchemy]
level = WARN
handlers =
qualname = sqlalchemy.engine

[logger_alembic]
level = INFO
handlers =
qualname = alembic

[handlers]
keys = console

[handler_console]
class = StreamHandler
args = (sys.stderr,)
level = NOTSET
formatter = generic

[formatters]
keys = generic

[formatter_generic]
format = %(levelname)-5.5s [%(name)s] %(message)s
"#
    .to_string()
}

static PYTHON_GITIGNORE: &str = r#"__pycache__/
*.py[cod]
*.egg-info/
dist/
.env
.venv/
venv/
*.db
"#;

// ── Axum templates ──

fn axum_cargo_toml(name: &str) -> String {
    format!(
        r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = {{ version = "1", features = ["full"] }}
serde = {{ version = "1", features = ["derive"] }}
serde_json = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
tower-http = {{ version = "0.5", features = ["cors", "trace"] }}
"#
    )
}

static AXUM_MAIN_RS: &str = r#"use axum::Router;
use tracing_subscriber;
use std::net::SocketAddr;

mod routes;
mod models;

#[tokio::main]
async fn main() {
    tracing_subscriber::init();

    let app = Router::new().merge(routes::router());

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
"#;

static AXUM_ROUTES_RS: &str = r#"use axum::{Router, routing::get, Json};
use serde_json::{json, Value};

pub fn router() -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
}

async fn root() -> Json<Value> {
    Json(json!({ "message": "Hello, World!" }))
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
"#;

static AXUM_MODELS_RS: &str = r#"use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
}
"#;

static RUST_GITIGNORE: &str = r#"target/
Cargo.lock
.env
"#;

// ── SvelteKit templates ──

fn sveltekit_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview"
  }},
  "devDependencies": {{
    "@sveltejs/adapter-auto": "^3.0.0",
    "@sveltejs/kit": "^2.0.0",
    "svelte": "^4.2.0",
    "typescript": "^5.3.0",
    "vite": "^5.0.0"
  }}
}}"#
    )
}

static SVELTEKIT_CONFIG: &str = r#"import adapter from '@sveltejs/adapter-auto';

/** @type {import('@sveltejs/kit').Config} */
const config = {
  kit: { adapter: adapter() },
};
export default config;
"#;

static SVELTEKIT_VITE_CONFIG: &str = r#"import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
});
"#;

static SVELTEKIT_TSCONFIG: &str = r#"{
  "extends": "./.svelte-kit/tsconfig.json",
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "esModuleInterop": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "skipLibCheck": true,
    "sourceMap": true,
    "strict": true
  }
}
"#;

static SVELTEKIT_APP_HTML: &str = r#"<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    %sveltekit.head%
  </head>
  <body data-sveltekit-preload-data="hover">
    <div style="display: contents">%sveltekit.body%</div>
  </body>
</html>
"#;

static SVELTEKIT_PAGE: &str = r#"<script lang="ts">
  // Add your page logic here
</script>

<main>
  <h1>Welcome to SvelteKit</h1>
  <p>Edit src/routes/+page.svelte to get started.</p>
</main>
"#;

// ── Express templates ──

fn express_package_json(name: &str) -> String {
    format!(
        r#"{{
  "name": "{name}",
  "version": "0.1.0",
  "private": true,
  "scripts": {{
    "dev": "tsx watch src/index.ts",
    "build": "tsc",
    "start": "node dist/index.js"
  }},
  "dependencies": {{
    "express": "^4.18.0",
    "cors": "^2.8.5"
  }},
  "devDependencies": {{
    "@types/express": "^4.17.21",
    "@types/cors": "^2.8.17",
    "@types/node": "^20.10.0",
    "tsx": "^4.6.0",
    "typescript": "^5.3.0"
  }}
}}"#
    )
}

static EXPRESS_TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2020",
    "module": "commonjs",
    "lib": ["ES2020"],
    "outDir": "./dist",
    "rootDir": "./src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true
  },
  "include": ["src/**/*"],
  "exclude": ["node_modules", "dist"]
}
"#;

static EXPRESS_INDEX_TS: &str = r#"import express from 'express';
import cors from 'cors';
import { router } from './routes';

const app = express();
const port = process.env.PORT || 3000;

app.use(cors());
app.use(express.json());
app.use('/api', router);

app.get('/health', (_req, res) => {
  res.json({ status: 'ok' });
});

app.listen(port, () => {
  console.log(`Server running on http://localhost:${port}`);
});
"#;

static EXPRESS_ROUTES_TS: &str = r#"import { Router } from 'express';

export const router = Router();

router.get('/', (_req, res) => {
  res.json({ message: 'Hello, World!' });
});
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Tool;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_list_templates() {
        let tool = ScaffoldTool::new(PathBuf::from("/tmp"));
        let result = tool
            .execute(json!({"action": "list_templates"}))
            .await
            .unwrap();
        assert!(result.content.contains("react-vite"));
        assert!(result.content.contains("nextjs"));
        assert!(result.content.contains("fastapi"));
        assert!(result.content.contains("rust-axum"));
    }

    #[tokio::test]
    async fn test_create_react_vite_project() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("my-app");
        let tool = ScaffoldTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "create_project",
                "template": "react-vite",
                "name": "my-app",
                "target_dir": target.to_str().unwrap()
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Created project"));
        assert!(target.join("package.json").exists());
        assert!(target.join("src/App.tsx").exists());
    }

    #[tokio::test]
    async fn test_create_rust_axum_project() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("my-api");
        let tool = ScaffoldTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "create_project",
                "template": "rust-axum",
                "name": "my-api",
                "target_dir": target.to_str().unwrap()
            }))
            .await
            .unwrap();
        assert!(result.content.contains("Created project"));
        assert!(target.join("Cargo.toml").exists());
        assert!(target.join("src/main.rs").exists());
    }

    #[tokio::test]
    async fn test_invalid_template() {
        let dir = TempDir::new().unwrap();
        let tool = ScaffoldTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({
                "action": "create_project",
                "template": "nonexistent",
                "name": "app"
            }))
            .await;
        assert!(result.is_err());
    }

    #[test]
    fn test_to_snake_case() {
        assert_eq!(to_snake_case("MyComponent"), "my_component");
        assert_eq!(to_snake_case("hello"), "hello");
    }

    #[test]
    fn test_to_pascal_case() {
        assert_eq!(to_pascal_case("my-component"), "MyComponent");
        assert_eq!(to_pascal_case("hello_world"), "HelloWorld");
    }
}
