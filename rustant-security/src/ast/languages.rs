//! Feature-gated language grammar setup for tree-sitter.
//!
//! Each language grammar is behind a Cargo feature flag to avoid bloating
//! the binary with grammars the user doesn't need.

use super::{AstError, Language, Symbol, SymbolKind, Visibility};
use std::path::Path;

/// Check if a language has its tree-sitter grammar compiled in.
pub fn is_supported(lang: Language) -> bool {
    match lang {
        #[cfg(feature = "sast-rust")]
        Language::Rust => true,
        #[cfg(feature = "sast-python")]
        Language::Python => true,
        #[cfg(feature = "sast-javascript")]
        Language::JavaScript | Language::TypeScript => true,
        #[cfg(feature = "sast-go")]
        Language::Go => true,
        #[cfg(feature = "sast-java")]
        Language::Java => true,
        #[cfg(feature = "sast-shell")]
        Language::Bash => true,
        #[cfg(feature = "sast-iac")]
        Language::Hcl => true,
        _ => false,
    }
}

// ────────────────────────────── Rust ──────────────────────────────

#[cfg(feature = "sast-rust")]
pub fn extract_rust_symbols(source: &str, file: &Path) -> Result<Vec<Symbol>, AstError> {
    use tree_sitter::Parser;

    let mut parser = Parser::new();
    let language = tree_sitter_rust::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| AstError::ParseError {
            file: file.display().to_string(),
            message: format!("Failed to set Rust language: {e}"),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AstError::ParseError {
            file: file.display().to_string(),
            message: "Failed to parse Rust source".into(),
        })?;

    let mut symbols = Vec::new();
    let root = tree.root_node();

    extract_rust_symbols_from_node(root, source, file, &mut symbols);

    Ok(symbols)
}

#[cfg(feature = "sast-rust")]
fn extract_rust_symbols_from_node(
    node: tree_sitter::Node,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                let visibility = if source[..node.start_byte()].ends_with("pub ")
                    || node.parent().is_some_and(|p| {
                        let node_src = &source[p.start_byte()..node.start_byte()];
                        node_src.contains("pub ")
                    }) {
                    Visibility::Public
                } else {
                    Visibility::Private
                };

                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    language: Language::Rust,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "struct_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Struct,
                    language: Language::Rust,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Unknown,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "enum_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Enum,
                    language: Language::Rust,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Unknown,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "trait_item" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Trait,
                    language: Language::Rust,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Unknown,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "impl_item" => {
            // Recurse into impl blocks to find methods
            let cursor = &mut node.walk();
            for child in node.children(cursor) {
                extract_rust_symbols_from_node(child, source, file, symbols);
            }
            return; // Don't recurse again below
        }
        _ => {}
    }

    // Recurse into children
    let cursor = &mut node.walk();
    for child in node.children(cursor) {
        extract_rust_symbols_from_node(child, source, file, symbols);
    }
}

// ────────────────────────────── Python ──────────────────────────────

#[cfg(feature = "sast-python")]
pub fn extract_python_symbols(source: &str, file: &Path) -> Result<Vec<Symbol>, AstError> {
    use tree_sitter::Parser;

    let mut parser = Parser::new();
    let language = tree_sitter_python::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| AstError::ParseError {
            file: file.display().to_string(),
            message: format!("Failed to set Python language: {e}"),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AstError::ParseError {
            file: file.display().to_string(),
            message: "Failed to parse Python source".into(),
        })?;

    let mut symbols = Vec::new();
    let root = tree.root_node();

    extract_python_symbols_from_node(root, source, file, &mut symbols);

    Ok(symbols)
}

#[cfg(feature = "sast-python")]
fn extract_python_symbols_from_node(
    node: tree_sitter::Node,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                let visibility = if name.starts_with('_') {
                    Visibility::Private
                } else {
                    Visibility::Public
                };
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    language: Language::Python,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "class_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Class,
                    language: Language::Python,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Public,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        _ => {}
    }

    let cursor = &mut node.walk();
    for child in node.children(cursor) {
        extract_python_symbols_from_node(child, source, file, symbols);
    }
}

// ────────────────────────────── JavaScript/TypeScript ──────────────────────────────

#[cfg(feature = "sast-javascript")]
pub fn extract_js_symbols(
    source: &str,
    language: Language,
    file: &Path,
) -> Result<Vec<Symbol>, AstError> {
    use tree_sitter::Parser;

    let mut parser = Parser::new();
    let ts_language = if language == Language::TypeScript {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT
    } else {
        tree_sitter_javascript::LANGUAGE
    };
    parser
        .set_language(&ts_language.into())
        .map_err(|e| AstError::ParseError {
            file: file.display().to_string(),
            message: format!("Failed to set JS/TS language: {e}"),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AstError::ParseError {
            file: file.display().to_string(),
            message: "Failed to parse JS/TS source".into(),
        })?;

    let mut symbols = Vec::new();
    extract_js_symbols_from_node(tree.root_node(), source, language, file, &mut symbols);
    Ok(symbols)
}

#[cfg(feature = "sast-javascript")]
fn extract_js_symbols_from_node(
    node: tree_sitter::Node,
    source: &str,
    language: Language,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: source[name_node.byte_range()].to_string(),
                    kind: SymbolKind::Function,
                    language,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Unknown,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: source[name_node.byte_range()].to_string(),
                    kind: SymbolKind::Class,
                    language,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Unknown,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        _ => {}
    }

    let cursor = &mut node.walk();
    for child in node.children(cursor) {
        extract_js_symbols_from_node(child, source, language, file, symbols);
    }
}

// ────────────────────────────── Go ──────────────────────────────

#[cfg(feature = "sast-go")]
pub fn extract_go_symbols(source: &str, file: &Path) -> Result<Vec<Symbol>, AstError> {
    use tree_sitter::Parser;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .map_err(|e| AstError::ParseError {
            file: file.display().to_string(),
            message: format!("Failed to set Go language: {e}"),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AstError::ParseError {
            file: file.display().to_string(),
            message: "Failed to parse Go source".into(),
        })?;

    let mut symbols = Vec::new();
    extract_go_symbols_from_node(tree.root_node(), source, file, &mut symbols);
    Ok(symbols)
}

#[cfg(feature = "sast-go")]
fn extract_go_symbols_from_node(
    node: tree_sitter::Node,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "function_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                let visibility = if name.starts_with(|c: char| c.is_uppercase()) {
                    Visibility::Public
                } else {
                    Visibility::Private
                };
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    language: Language::Go,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "type_declaration" => {
            // Go type declarations (struct, interface)
            let cursor = &mut node.walk();
            for child in node.children(cursor) {
                if child.kind() == "type_spec" {
                    if let Some(name_node) = child.child_by_field_name("name") {
                        let name = &source[name_node.byte_range()];
                        let kind = if child
                            .child_by_field_name("type")
                            .is_some_and(|t| t.kind() == "interface_type")
                        {
                            SymbolKind::Interface
                        } else {
                            SymbolKind::Struct
                        };
                        symbols.push(Symbol {
                            name: name.to_string(),
                            kind,
                            language: Language::Go,
                            file: file.to_path_buf(),
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                            visibility: Visibility::Unknown,
                            parameters: Vec::new(),
                            return_type: None,
                        });
                    }
                }
            }
            return;
        }
        _ => {}
    }

    let cursor = &mut node.walk();
    for child in node.children(cursor) {
        extract_go_symbols_from_node(child, source, file, symbols);
    }
}

// ────────────────────────────── Java ──────────────────────────────

#[cfg(feature = "sast-java")]
pub fn extract_java_symbols(source: &str, file: &Path) -> Result<Vec<Symbol>, AstError> {
    use tree_sitter::Parser;

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_java::LANGUAGE.into())
        .map_err(|e| AstError::ParseError {
            file: file.display().to_string(),
            message: format!("Failed to set Java language: {e}"),
        })?;

    let tree = parser
        .parse(source, None)
        .ok_or_else(|| AstError::ParseError {
            file: file.display().to_string(),
            message: "Failed to parse Java source".into(),
        })?;

    let mut symbols = Vec::new();
    extract_java_symbols_from_node(tree.root_node(), source, file, &mut symbols);
    Ok(symbols)
}

#[cfg(feature = "sast-java")]
fn extract_java_symbols_from_node(
    node: tree_sitter::Node,
    source: &str,
    file: &Path,
    symbols: &mut Vec<Symbol>,
) {
    match node.kind() {
        "method_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: source[name_node.byte_range()].to_string(),
                    kind: SymbolKind::Method,
                    language: Language::Java,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Unknown,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "class_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: source[name_node.byte_range()].to_string(),
                    kind: SymbolKind::Class,
                    language: Language::Java,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Unknown,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        "interface_declaration" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                symbols.push(Symbol {
                    name: source[name_node.byte_range()].to_string(),
                    kind: SymbolKind::Interface,
                    language: Language::Java,
                    file: file.to_path_buf(),
                    start_line: node.start_position().row + 1,
                    end_line: node.end_position().row + 1,
                    visibility: Visibility::Unknown,
                    parameters: Vec::new(),
                    return_type: None,
                });
            }
        }
        _ => {}
    }

    let cursor = &mut node.walk();
    for child in node.children(cursor) {
        extract_java_symbols_from_node(child, source, file, symbols);
    }
}
