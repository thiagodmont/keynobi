use crate::models::lsp::{SymbolInfo, SymbolKind, TextRange};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use tree_sitter::{Node, Parser, Tree};

const MAX_CACHED_TREES: usize = 50;

pub struct TreeSitterService {
    parser: Parser,
    cache: HashMap<PathBuf, Tree>,
    lru_order: VecDeque<PathBuf>,
}

impl TreeSitterService {
    pub fn new() -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_kotlin_ng::LANGUAGE.into())
            .expect("failed to load Kotlin grammar");
        Self {
            parser,
            cache: HashMap::new(),
            lru_order: VecDeque::new(),
        }
    }

    fn touch_lru(&mut self, path: &Path) {
        let key = path.to_path_buf();
        self.lru_order.retain(|p| p != &key);
        self.lru_order.push_back(key);
        while self.lru_order.len() > MAX_CACHED_TREES {
            if let Some(evicted) = self.lru_order.pop_front() {
                self.cache.remove(&evicted);
            }
        }
    }

    pub fn parse_file(&mut self, path: &Path, content: &str) -> Option<&Tree> {
        let tree = self.parser.parse(content, None)?;
        self.cache.insert(path.to_path_buf(), tree);
        self.touch_lru(path);
        self.cache.get(path)
    }

    /// Re-parse a file with a fresh parse. The old cached tree is replaced.
    /// Incremental parsing with edit deltas is planned as a future optimization.
    pub fn reparse_file(&mut self, path: &Path, content: &str) -> Option<&Tree> {
        let tree = self.parser.parse(content, None)?;
        self.cache.insert(path.to_path_buf(), tree);
        self.touch_lru(path);
        self.cache.get(path)
    }

    pub fn get_cached_tree(&self, path: &Path) -> Option<&Tree> {
        self.cache.get(path)
    }

    pub fn invalidate_cache(&mut self, path: &Path) {
        self.cache.remove(path);
    }

    pub fn extract_symbols(&self, path: &Path, source: &str) -> Vec<SymbolInfo> {
        let tree = match self.cache.get(path) {
            Some(t) => t,
            None => return Vec::new(),
        };
        let root = tree.root_node();
        collect_symbols(root, source)
    }

    pub fn find_node_at_position<'a>(
        &self,
        path: &Path,
        line: u32,
        col: u32,
        source: &'a str,
    ) -> Option<String> {
        let tree = self.cache.get(path)?;
        let point = tree_sitter::Point::new(line as usize, col as usize);
        let node = tree
            .root_node()
            .descendant_for_point_range(point, point)?;

        if node.kind() == "simple_identifier" || node.kind() == "identifier" {
            let text = &source[node.byte_range()];
            return Some(text.to_string());
        }

        None
    }
}

impl Default for TreeSitterService {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_symbols(node: Node, source: &str) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if let Some(sym) = node_to_symbol(child, source) {
            symbols.push(sym);
        }
    }

    symbols
}

fn node_to_symbol(node: Node, source: &str) -> Option<SymbolInfo> {
    let kind = match node.kind() {
        "class_declaration" => {
            // Distinguish class vs interface: check if first keyword child is "interface"
            if has_keyword_child(node, "interface") {
                SymbolKind::Interface
            } else {
                SymbolKind::Class
            }
        }
        "object_declaration" => SymbolKind::Class,
        "function_declaration" => SymbolKind::Function,
        "property_declaration" => SymbolKind::Property,
        _ => return None,
    };

    let name = extract_name(node, source)?;
    let range = node_range(node);
    let name_node = find_name_node(node);
    let selection_range = name_node.map(|n| node_range(n)).unwrap_or(range.clone());

    let children = if matches!(
        node.kind(),
        "class_declaration" | "object_declaration"
    ) {
        find_body_symbols(node, source)
    } else {
        None
    };

    Some(SymbolInfo {
        name,
        kind,
        range,
        selection_range,
        children,
    })
}

fn has_keyword_child(node: Node, keyword: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == keyword {
            return true;
        }
        if child.kind() == "identifier" || child.kind() == "class_body" {
            break;
        }
    }
    false
}

fn find_body_symbols(node: Node, source: &str) -> Option<Vec<SymbolInfo>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "class_body" {
            let syms = collect_symbols(child, source);
            if !syms.is_empty() {
                return Some(syms);
            }
        }
    }
    None
}

fn extract_name(node: Node, source: &str) -> Option<String> {
    // For property_declaration: look inside variable_declaration for the identifier
    if node.kind() == "property_declaration" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declaration" {
                let mut inner = child.walk();
                for grandchild in child.children(&mut inner) {
                    if grandchild.kind() == "identifier" {
                        let text = &source[grandchild.byte_range()];
                        if !text.is_empty() {
                            return Some(text.to_string());
                        }
                    }
                }
            }
        }
    }

    // For class/interface/function/object: direct identifier child
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            let text = &source[child.byte_range()];
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }

    None
}

fn find_name_node(node: Node) -> Option<Node> {
    if node.kind() == "property_declaration" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "variable_declaration" {
                let mut inner = child.walk();
                for grandchild in child.children(&mut inner) {
                    if grandchild.kind() == "identifier" {
                        return Some(grandchild);
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return Some(child);
        }
    }
    None
}

fn node_range(node: Node) -> TextRange {
    let start = node.start_position();
    let end = node.end_position();
    TextRange {
        start_line: start.row as u32,
        start_col: start.column as u32,
        end_line: end.row as u32,
        end_col: end.column as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_KOTLIN: &str = r#"
package com.example

class MyClass {
    val name: String = "hello"

    fun greet(): String {
        return "Hello, $name"
    }

    fun compute(x: Int): Int {
        return x * 2
    }
}

interface Greeter {
    fun sayHello()
}

fun topLevelFunction() {
    println("top level")
}

val topLevelProperty = 42
"#;

    #[test]
    fn parses_kotlin_file() {
        let mut service = TreeSitterService::new();
        let path = Path::new("/test/Main.kt");
        let tree = service.parse_file(path, SAMPLE_KOTLIN);
        assert!(tree.is_some());
        assert!(!tree.unwrap().root_node().has_error());
    }

    #[test]
    fn extracts_class_symbols() {
        let mut service = TreeSitterService::new();
        let path = Path::new("/test/Main.kt");
        service.parse_file(path, SAMPLE_KOTLIN);
        let symbols = service.extract_symbols(path, SAMPLE_KOTLIN);

        let class_sym = symbols.iter().find(|s| s.name == "MyClass");
        assert!(class_sym.is_some(), "Should find MyClass");
        let class_sym = class_sym.unwrap();
        assert!(matches!(class_sym.kind, SymbolKind::Class));
        assert!(class_sym.children.is_some());

        let children = class_sym.children.as_ref().unwrap();
        let method_names: Vec<&str> = children.iter().map(|c| c.name.as_str()).collect();
        assert!(
            method_names.contains(&"greet"),
            "Should find greet method, got: {:?}",
            method_names
        );
        assert!(
            method_names.contains(&"compute"),
            "Should find compute method, got: {:?}",
            method_names
        );
    }

    #[test]
    fn extracts_interface_symbols() {
        let mut service = TreeSitterService::new();
        let path = Path::new("/test/Main.kt");
        service.parse_file(path, SAMPLE_KOTLIN);
        let symbols = service.extract_symbols(path, SAMPLE_KOTLIN);

        let iface = symbols.iter().find(|s| s.name == "Greeter");
        assert!(iface.is_some(), "Should find Greeter interface");
        assert!(matches!(iface.unwrap().kind, SymbolKind::Interface));
    }

    #[test]
    fn extracts_top_level_function() {
        let mut service = TreeSitterService::new();
        let path = Path::new("/test/Main.kt");
        service.parse_file(path, SAMPLE_KOTLIN);
        let symbols = service.extract_symbols(path, SAMPLE_KOTLIN);

        let func = symbols.iter().find(|s| s.name == "topLevelFunction");
        assert!(func.is_some(), "Should find topLevelFunction");
        assert!(matches!(func.unwrap().kind, SymbolKind::Function));
    }

    #[test]
    fn extracts_top_level_property() {
        let mut service = TreeSitterService::new();
        let path = Path::new("/test/Main.kt");
        service.parse_file(path, SAMPLE_KOTLIN);
        let symbols = service.extract_symbols(path, SAMPLE_KOTLIN);

        let prop = symbols.iter().find(|s| s.name == "topLevelProperty");
        assert!(
            prop.is_some(),
            "Should find topLevelProperty. Found symbols: {:?}",
            symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(matches!(prop.unwrap().kind, SymbolKind::Property));
    }

    #[test]
    fn caches_and_invalidates() {
        let mut service = TreeSitterService::new();
        let path = Path::new("/test/Main.kt");
        service.parse_file(path, SAMPLE_KOTLIN);
        assert!(service.get_cached_tree(path).is_some());

        service.invalidate_cache(path);
        assert!(service.get_cached_tree(path).is_none());
    }

    #[test]
    fn reparses_with_cache() {
        let mut service = TreeSitterService::new();
        let path = Path::new("/test/Main.kt");
        service.parse_file(path, SAMPLE_KOTLIN);

        let modified = SAMPLE_KOTLIN.replace("MyClass", "MyUpdatedClass");
        service.reparse_file(path, &modified);

        let symbols = service.extract_symbols(path, &modified);
        let found = symbols.iter().any(|s| s.name == "MyUpdatedClass");
        assert!(found, "Should find renamed class");
    }

    #[test]
    fn find_node_at_position_returns_identifier() {
        let mut service = TreeSitterService::new();
        let path = Path::new("/test/Main.kt");
        service.parse_file(path, SAMPLE_KOTLIN);

        // "MyClass" starts on line 2 (0-indexed), after "class "
        // Find the exact position by searching the source
        let line_idx = SAMPLE_KOTLIN
            .lines()
            .position(|l| l.contains("class MyClass"))
            .expect("should find class line");

        let line = SAMPLE_KOTLIN.lines().nth(line_idx).unwrap();
        let col = line.find("MyClass").unwrap();

        let result = service.find_node_at_position(path, line_idx as u32, col as u32, SAMPLE_KOTLIN);
        assert_eq!(result, Some("MyClass".to_string()));
    }

    #[test]
    fn returns_empty_for_uncached_path() {
        let service = TreeSitterService::new();
        let path = Path::new("/nonexistent/Main.kt");
        let symbols = service.extract_symbols(path, "");
        assert!(symbols.is_empty());
    }
}
