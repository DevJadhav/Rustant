# Codebase Intelligence

Rustant provides deep codebase understanding through AST parsing, repository mapping, context hydration, indexing, and verification.

## AST Engine (`rustant-core/src/ast/`)

Tree-sitter-based parsing with feature-gated grammars:
- **Supported**: Rust, Python, JavaScript, TypeScript, Go, Java
- **Fallback**: Regex-based extraction for unsupported languages
- **Features**: Symbol extraction (functions, structs, classes, methods), cyclomatic complexity, cross-file references

## RepoMap (`rustant-core/src/repo_map/`)

`CodeGraph` built on `petgraph::DiGraph`:
- Nodes represent code symbols (functions, types, modules)
- Edges represent relationships (calls, imports, inheritance)
- **PageRank** ranking for identifying important files and symbols
- Used to prioritize context assembly

## Hydration (`rustant-core/src/hydration/`)

`HydrationPipeline` assembles optimal context for LLM prompts:
1. **Selection** — Uses RepoMap ranking to select relevant files
2. **Assembly** — Token-budgeted context assembly fitting within model limits
3. **Threshold** — `should_hydrate()` skips projects with fewer than 10 code files

## Project Indexer (`rustant-core/src/indexer.rs`)

Background workspace indexer:
- `.gitignore`-aware file walking via the `ignore` crate
- Multi-language signature extraction (Rust, Python, JS/TS, Go, Java, Ruby, C/C++, SQL, and 50+ more via `CodeBlockKind`)
- `HybridSearchEngine` combining full-text and vector search
- **Incremental re-indexing** via `FileHashRegistry` (skip unchanged files, persist hashes to `.rustant/index_hashes.json`)

### CodeBlockKind

11 variants for typed code block extraction: Function, Class, Method, Struct, Enum, Interface, Module, Import, Constant, Type, Other.

## Verification (`rustant-core/src/verification/`)

Automated verification after code changes:
- `VerificationConfig` with `max_fix_attempts = 3`
- Runs test suite, linter, and build checks
- Feedback loop: if verification fails, the agent attempts to fix issues
- `fullstack_verify` workflow template integrates with the verification pipeline

## Project Detection (`rustant-core/src/project_detect.rs`)

Auto-detection of project characteristics:
- **Languages**: Rust, Python, JavaScript/TypeScript, Go, Java, Ruby, C#, C/C++
- **Frameworks**: React, Next.js, FastAPI, Axum, Django, Flask, Express, etc.
- **CI/CD**: GitHub Actions, GitLab CI, Jenkins, etc.
- Generates safety whitelists based on detected project type

## Search

The `codebase_search` tool provides natural language search over the indexed project:
- Supports `filter` parameter for block-kind filtering
- Supports `language` parameter for language-specific search
- Powered by the `HybridSearchEngine` (Tantivy + SQLite vector)
