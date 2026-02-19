//! Reference extraction from tree-sitter AST.
//!
//! Extracts function calls, imports, and type references.

use super::{Reference, ReferenceKind};

/// Extract references from a parsed tree.
#[cfg(any(
    feature = "ast-rust",
    feature = "ast-python",
    feature = "ast-javascript",
    feature = "ast-go",
    feature = "ast-java"
))]
pub fn extract_references_from_tree(
    tree: &tree_sitter::Tree,
    source: &str,
    file_path: &str,
    ext: &str,
) -> Vec<Reference> {
    let root = tree.root_node();
    let mut refs = Vec::new();
    let source_bytes = source.as_bytes();

    collect_references(&root, source_bytes, file_path, ext, &mut refs);
    refs
}

#[cfg(any(
    feature = "ast-rust",
    feature = "ast-python",
    feature = "ast-javascript",
    feature = "ast-go",
    feature = "ast-java"
))]
fn collect_references(
    node: &tree_sitter::Node,
    source: &[u8],
    file: &str,
    ext: &str,
    refs: &mut Vec<Reference>,
) {
    let kind = node.kind();
    let line = node.start_position().row + 1;

    match ext {
        "rs" => match kind {
            "call_expression" => {
                if let Some(func) = node.child_by_field_name("function")
                    && let Ok(name) = func.utf8_text(source)
                {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: name.to_string(),
                        kind: ReferenceKind::Call,
                    });
                }
            }
            "use_declaration" => {
                if let Ok(text) = node.utf8_text(source) {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: text.to_string(),
                        kind: ReferenceKind::Import,
                    });
                }
            }
            "type_identifier" => {
                if let Ok(name) = node.utf8_text(source) {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: name.to_string(),
                        kind: ReferenceKind::TypeRef,
                    });
                }
            }
            _ => {}
        },
        "py" => match kind {
            "call" => {
                if let Some(func) = node.child_by_field_name("function")
                    && let Ok(name) = func.utf8_text(source)
                {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: name.to_string(),
                        kind: ReferenceKind::Call,
                    });
                }
            }
            "import_statement" | "import_from_statement" => {
                if let Ok(text) = node.utf8_text(source) {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: text.to_string(),
                        kind: ReferenceKind::Import,
                    });
                }
            }
            _ => {}
        },
        "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" => match kind {
            "call_expression" => {
                if let Some(func) = node.child_by_field_name("function")
                    && let Ok(name) = func.utf8_text(source)
                {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: name.to_string(),
                        kind: ReferenceKind::Call,
                    });
                }
            }
            "import_statement" => {
                if let Ok(text) = node.utf8_text(source) {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: text.to_string(),
                        kind: ReferenceKind::Import,
                    });
                }
            }
            "type_identifier" => {
                if let Ok(name) = node.utf8_text(source) {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: name.to_string(),
                        kind: ReferenceKind::TypeRef,
                    });
                }
            }
            _ => {}
        },
        "go" => match kind {
            "call_expression" => {
                if let Some(func) = node.child_by_field_name("function")
                    && let Ok(name) = func.utf8_text(source)
                {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: name.to_string(),
                        kind: ReferenceKind::Call,
                    });
                }
            }
            "import_declaration" => {
                if let Ok(text) = node.utf8_text(source) {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: text.to_string(),
                        kind: ReferenceKind::Import,
                    });
                }
            }
            _ => {}
        },
        "java" => match kind {
            "method_invocation" => {
                if let Some(name_node) = node.child_by_field_name("name")
                    && let Ok(name) = name_node.utf8_text(source)
                {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: name.to_string(),
                        kind: ReferenceKind::Call,
                    });
                }
            }
            "import_declaration" => {
                if let Ok(text) = node.utf8_text(source) {
                    refs.push(Reference {
                        from_file: file.to_string(),
                        from_line: line,
                        to_name: text.to_string(),
                        kind: ReferenceKind::Import,
                    });
                }
            }
            _ => {}
        },
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_references(&child, source, file, ext, refs);
    }
}
