//! Per-language tree-sitter queries and symbol extraction.

#[cfg(any(
    feature = "ast-rust",
    feature = "ast-python",
    feature = "ast-javascript",
    feature = "ast-go",
    feature = "ast-java"
))]
use tree_sitter::Language;

use super::{Symbol, SymbolKind};

/// Get the tree-sitter language for a file extension.
#[cfg(any(
    feature = "ast-rust",
    feature = "ast-python",
    feature = "ast-javascript",
    feature = "ast-go",
    feature = "ast-java"
))]
pub fn language_for_extension(ext: &str) -> Option<Language> {
    match ext {
        #[cfg(feature = "ast-rust")]
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        #[cfg(feature = "ast-python")]
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        #[cfg(feature = "ast-javascript")]
        "js" | "jsx" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        #[cfg(feature = "ast-javascript")]
        "ts" | "tsx" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        #[cfg(feature = "ast-go")]
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        #[cfg(feature = "ast-java")]
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        _ => None,
    }
}

/// Extract symbols from a parsed tree-sitter tree.
#[cfg(any(
    feature = "ast-rust",
    feature = "ast-python",
    feature = "ast-javascript",
    feature = "ast-go",
    feature = "ast-java"
))]
pub fn extract_symbols_from_tree(
    tree: &tree_sitter::Tree,
    source: &str,
    file_path: &str,
    ext: &str,
) -> Vec<Symbol> {
    let root = tree.root_node();
    let mut symbols = Vec::new();
    let source_bytes = source.as_bytes();

    collect_symbols(&root, source_bytes, file_path, ext, &mut symbols);
    symbols
}

#[cfg(any(
    feature = "ast-rust",
    feature = "ast-python",
    feature = "ast-javascript",
    feature = "ast-go",
    feature = "ast-java"
))]
fn collect_symbols(
    node: &tree_sitter::Node,
    source: &[u8],
    file: &str,
    ext: &str,
    symbols: &mut Vec<Symbol>,
) {
    let kind = node.kind();

    let symbol_info = match ext {
        "rs" => match_rust_symbol(kind, node, source),
        "py" => match_python_symbol(kind, node, source),
        "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" => match_js_symbol(kind, node, source),
        "go" => match_go_symbol(kind, node, source),
        "java" => match_java_symbol(kind, node, source),
        _ => None,
    };

    if let Some((name, sym_kind, signature)) = symbol_info {
        symbols.push(Symbol {
            name,
            kind: sym_kind,
            file: file.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            signature,
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_symbols(&child, source, file, ext, symbols);
    }
}

#[cfg(feature = "ast-rust")]
fn match_rust_symbol(
    kind: &str,
    node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    match kind {
        "function_item" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Function, sig))
        }
        "struct_item" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Struct, sig))
        }
        "trait_item" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Trait, sig))
        }
        "enum_item" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Enum, sig))
        }
        "impl_item" => {
            // Extract method definitions inside impl blocks
            None // Methods are collected via recursion into children
        }
        _ => None,
    }
}

#[cfg(all(
    not(feature = "ast-rust"),
    any(
        feature = "ast-python",
        feature = "ast-javascript",
        feature = "ast-go",
        feature = "ast-java"
    )
))]
fn match_rust_symbol(
    _kind: &str,
    _node: &tree_sitter::Node,
    _source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    None
}

#[cfg(feature = "ast-python")]
fn match_python_symbol(
    kind: &str,
    node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    match kind {
        "function_definition" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Function, sig))
        }
        "class_definition" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Class, sig))
        }
        _ => None,
    }
}

#[cfg(all(
    not(feature = "ast-python"),
    any(
        feature = "ast-rust",
        feature = "ast-javascript",
        feature = "ast-go",
        feature = "ast-java"
    )
))]
fn match_python_symbol(
    _kind: &str,
    _node: &tree_sitter::Node,
    _source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    None
}

#[cfg(feature = "ast-javascript")]
fn match_js_symbol(
    kind: &str,
    node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    match kind {
        "function_declaration" | "generator_function_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Function, sig))
        }
        "class_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Class, sig))
        }
        "interface_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Interface, sig))
        }
        "type_alias_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::TypeAlias, sig))
        }
        _ => None,
    }
}

#[cfg(all(
    not(feature = "ast-javascript"),
    any(
        feature = "ast-rust",
        feature = "ast-python",
        feature = "ast-go",
        feature = "ast-java"
    )
))]
fn match_js_symbol(
    _kind: &str,
    _node: &tree_sitter::Node,
    _source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    None
}

#[cfg(feature = "ast-go")]
fn match_go_symbol(
    kind: &str,
    node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    match kind {
        "function_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Function, sig))
        }
        "method_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Method, sig))
        }
        "type_declaration" => {
            let name = find_child_text(node, "name", source).or_else(|| {
                // Go type declarations have a type_spec child
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "type_spec" {
                        return find_child_text(&child, "name", source);
                    }
                }
                None
            })?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Struct, sig))
        }
        _ => None,
    }
}

#[cfg(all(
    not(feature = "ast-go"),
    any(
        feature = "ast-rust",
        feature = "ast-python",
        feature = "ast-javascript",
        feature = "ast-java"
    )
))]
fn match_go_symbol(
    _kind: &str,
    _node: &tree_sitter::Node,
    _source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    None
}

#[cfg(feature = "ast-java")]
fn match_java_symbol(
    kind: &str,
    node: &tree_sitter::Node,
    source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    match kind {
        "method_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Method, sig))
        }
        "class_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Class, sig))
        }
        "interface_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Interface, sig))
        }
        "enum_declaration" => {
            let name = find_child_text(node, "name", source)?;
            let sig = first_line_text(node, source);
            Some((name, SymbolKind::Enum, sig))
        }
        _ => None,
    }
}

#[cfg(all(
    not(feature = "ast-java"),
    any(
        feature = "ast-rust",
        feature = "ast-python",
        feature = "ast-javascript",
        feature = "ast-go"
    )
))]
fn match_java_symbol(
    _kind: &str,
    _node: &tree_sitter::Node,
    _source: &[u8],
) -> Option<(String, SymbolKind, String)> {
    None
}

#[cfg(any(
    feature = "ast-rust",
    feature = "ast-python",
    feature = "ast-javascript",
    feature = "ast-go",
    feature = "ast-java"
))]
fn find_child_text(node: &tree_sitter::Node, field_name: &str, source: &[u8]) -> Option<String> {
    let child = node.child_by_field_name(field_name)?;
    child.utf8_text(source).ok().map(|s| s.to_string())
}

#[cfg(any(
    feature = "ast-rust",
    feature = "ast-python",
    feature = "ast-javascript",
    feature = "ast-go",
    feature = "ast-java"
))]
fn first_line_text(node: &tree_sitter::Node, source: &[u8]) -> String {
    let text = node.utf8_text(source).unwrap_or("");
    text.lines().next().unwrap_or("").to_string()
}
