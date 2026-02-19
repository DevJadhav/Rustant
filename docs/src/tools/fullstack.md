# Fullstack Development Tools

Rustant includes 5 fullstack development tools and 6 project templates for end-to-end web and API development. All tools use `detect_project()` for framework-aware behavior and have a `workspace: PathBuf` field for workspace-scoped operations.

## Tool Summary

| Tool | Risk Level | Actions | Description |
|------|-----------|---------|-------------|
| `scaffold` | Write | 4 | Project scaffolding and component generation |
| `dev_server` | Execute | 5 | Development server lifecycle management |
| `database` | Write | 6 | Database migrations, queries, and schema |
| `test_runner` | Execute | 5 | Multi-framework test execution |
| `lint` | Read-only / Write | 5 | Multi-language linting, formatting, and type checking |

---

## scaffold

Initialize new projects from templates and generate framework-specific components, routes, and structures.

| Action | Description |
|--------|-------------|
| `create_project` | Create a new project from a template |
| `add_component` | Add a component to an existing project |
| `add_route` | Add a route or endpoint |
| `list_templates` | List all available project templates |

**Key Parameters:**

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `action` | string | Yes | One of the 4 actions above |
| `template` | string | For `create_project` | Template name (see Templates below) |
| `name` | string | For `create_project`, `add_component`, `add_route` | Name for the project or component |
| `target_dir` | string | No | Target directory (defaults to workspace) |
| `options` | object | No | Template options (e.g., `{"tailwind": true, "typescript": true}`) |

**Example -- create a new project:**
```json
{
  "action": "create_project",
  "template": "react-vite",
  "name": "my-dashboard",
  "options": {"tailwind": true, "typescript": true}
}
```

**Example -- add a component:**
```json
{
  "action": "add_component",
  "name": "UserProfile"
}
```

---

## dev_server

Manage development server processes. Detects the project framework and uses the appropriate dev server command.

| Action | Description |
|--------|-------------|
| `start` | Start the development server |
| `stop` | Stop a running dev server |
| `restart` | Restart the dev server |
| `status` | Check if the dev server is running |
| `logs` | View recent dev server logs |

**Example:**
```json
{
  "action": "start"
}
```

The tool automatically detects the project type (e.g., Vite, Next.js, FastAPI, Axum) and runs the appropriate start command.

---

## database

Database operations including migrations, rollbacks, seeding, and queries.

| Action | Description |
|--------|-------------|
| `migrate` | Run pending database migrations |
| `rollback` | Rollback the last migration |
| `seed` | Seed the database with initial data |
| `query` | Execute a database query |
| `schema` | Display the current database schema |
| `status` | Show migration status |

**Key Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `action` | string | One of the 6 actions above |
| `query` | string | SQL query (for `query` action) |
| `migration_name` | string | Migration identifier |
| `steps` | integer | Number of migrations to rollback |

**Example:**
```json
{
  "action": "query",
  "query": "SELECT COUNT(*) FROM users WHERE active = true"
}
```

---

## test_runner

Run tests across multiple frameworks. Detects the project's test framework and executes accordingly (cargo test, pytest, jest, vitest, etc.).

| Action | Description |
|--------|-------------|
| `run_all` | Run all tests in the project |
| `run_file` | Run tests in a specific file |
| `run_test` | Run a single named test |
| `run_changed` | Run tests for changed files only |
| `coverage` | Generate test coverage report |

**Key Parameters:**

| Parameter | Type | Description |
|-----------|------|-------------|
| `action` | string | One of the 5 actions above |
| `file` | string | File path (for `run_file`) |
| `test_name` | string | Test name (for `run_test`) |
| `verbose` | boolean | Enable verbose output |

**Example:**
```json
{
  "action": "run_changed"
}
```

---

## lint

Multi-language linting, formatting, and type checking. Detects the project type and uses the appropriate tools (clippy, ruff, eslint, prettier, mypy, tsc, etc.).

| Action | Description |
|--------|-------------|
| `check` | Run linter and report issues |
| `fix` | Auto-fix linting issues |
| `typecheck` | Run type checker |
| `format` | Format code |
| `format_check` | Check formatting without making changes |

**Example:**
```json
{
  "action": "check"
}
```

---

## Project Templates

The `TemplateLibrary` provides 6 built-in project templates with alias support and handlebars-like variable substitution (`{{variable_name}}`).

| Template | Alias | Language | Framework | Description |
|----------|-------|----------|-----------|-------------|
| `react_vite` | `react-vite` | TypeScript | React + Vite | SPA with hot module replacement |
| `nextjs` | `next` | TypeScript | Next.js | Full-stack React with SSR/SSG |
| `fastapi` | `fastapi` | Python | FastAPI | Async Python API server |
| `rust_axum` | `rust-axum`, `axum` | Rust | Axum | Async Rust web server |
| `sveltekit` | `svelte` | TypeScript | SvelteKit | Full-stack Svelte application |
| `express` | `express` | TypeScript | Express.js | Node.js REST API server |

### Template Variables

Templates support handlebars-like variable substitution. Common variables:

| Variable | Description |
|----------|-------------|
| `{{project_name}}` | Name of the project |
| `{{author}}` | Author name |
| `{{description}}` | Project description |
| `{{port}}` | Default server port |

### Template Options

Each template supports framework-specific options passed via the `options` parameter:

| Option | Templates | Description |
|--------|-----------|-------------|
| `tailwind` | react_vite, nextjs, sveltekit | Include Tailwind CSS |
| `typescript` | All JS/TS templates | Use TypeScript (default: true) |
| `docker` | All | Include Dockerfile |
| `ci` | All | Include CI configuration |

---

## Related REPL Commands

| Command | Description |
|---------|-------------|
| `/init` | Initialize a new project (wraps scaffold) |
| `/preview` | Start dev server preview |
| `/db` | Database operations |
| `/test` | Run tests |
| `/lint` | Run linter |
| `/deps` | Manage dependencies |
| `/verify` | Run verification loop |
| `/repomap` | Show repository map |
| `/symbols` | List code symbols |
| `/refs` | Find code references |

## Verification Loop

The fullstack tools integrate with the Verification Loop (`rustant-core/src/verification/`), which runs iterative fix cycles:

1. Run linter/tests
2. If issues found, attempt auto-fix (up to `max_fix_attempts: 3`)
3. Re-run verification
4. Report results

Use the `/verify` slash command or the `fullstack_verify` workflow template.
