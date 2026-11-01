use std::collections::HashSet;

use sha2::{Digest, Sha256};
use tree_sitter::{Language, Node, Parser};

mod css;
mod graph;
mod html;
mod persist;
mod python;
mod rust;
mod typescript;

pub use graph::{LinkStats, link};
pub use persist::store;

const SIGNATURE_LIMIT: usize = 300;

#[derive(Debug, Clone)]
pub struct Unit {
    pub kind: String,
    pub name: String,
    pub qualified: String,
    pub signature: String,
    pub exported: bool,
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: u32,
    pub end_line: u32,
    pub parent: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Reference {
    pub name: String,
    pub kind: String,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct Import {
    pub source: String,
    pub names: Vec<String>,
    pub line: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Extracted {
    pub units: Vec<Unit>,
    pub refs: Vec<Reference>,
    pub imports: Vec<Import>,
    pub error_nodes: u32,
}

pub fn sig8(signature: &str) -> String {
    let normalised = collapse(signature);
    let digest = Sha256::digest(normalised.as_bytes());
    let mut out = String::with_capacity(8);
    for byte in digest.iter().take(4) {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn stable_id(repo: &str, path: &str, qualified: &str, signature: &str) -> String {
    format!("formslot {repo} {path}#{qualified}~{}", sig8(signature))
}

pub fn family(language: &str) -> Option<&'static str> {
    match language {
        "ts" | "tsx" | "js" => Some("ts"),
        "py" => Some("py"),
        "rs" => Some("rs"),
        "css" => Some("css"),
        "html" => Some("html"),
        _ => None,
    }
}

pub fn extract(language: &str, path: &str, text: &str) -> Option<Extracted> {
    let _ = path;
    let grammar: Language = match language {
        "ts" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" | "js" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "py" => tree_sitter_python::LANGUAGE.into(),
        "rs" => tree_sitter_rust::LANGUAGE.into(),
        "css" => tree_sitter_css::LANGUAGE.into(),
        "html" => tree_sitter_html::LANGUAGE.into(),
        _ => return None,
    };
    let mut parser = Parser::new();
    parser.set_language(&grammar).ok()?;
    let tree = parser.parse(text, None)?;
    let root = tree.root_node();
    let mut cx = Cx::new(text);
    match language {
        "ts" | "tsx" | "js" => typescript::run(&mut cx, root),
        "py" => python::run(&mut cx, root),
        "rs" => rust::run(&mut cx, root),
        "css" => css::run(&mut cx, root),
        _ => html::run(&mut cx, root),
    }
    cx.out.error_nodes = count_errors(root);
    Some(cx.out)
}

pub(crate) fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn truncate(text: String, limit: usize) -> String {
    if text.chars().count() <= limit {
        return text;
    }
    text.chars().take(limit).collect()
}

fn count_errors(root: Node) -> u32 {
    let mut cursor = root.walk();
    let mut total = 0u32;
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            total += 1;
        }
        if node.has_error() && cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return total;
            }
        }
    }
}

pub(crate) fn walk<'t>(root: Node<'t>, mut visit: impl FnMut(Node<'t>)) {
    let mut cursor = root.walk();
    loop {
        visit(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}

pub(crate) fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

pub(crate) struct Cx<'a> {
    pub src: &'a str,
    pub out: Extracted,
    seen: HashSet<(String, &'static str, u32)>,
    depth: usize,
}

pub(crate) const MAX_NESTING: usize = 200;

impl<'a> Cx<'a> {
    fn new(src: &'a str) -> Cx<'a> {
        Cx {
            src,
            out: Extracted::default(),
            seen: HashSet::new(),
            depth: 0,
        }
    }

    pub(crate) fn enter(&mut self) -> bool {
        if self.depth >= MAX_NESTING {
            return false;
        }
        self.depth += 1;
        true
    }

    pub(crate) fn leave(&mut self) {
        self.depth = self.depth.saturating_sub(1);
    }

    pub(crate) fn text(&self, node: Node) -> &'a str {
        self.src.get(node.byte_range()).unwrap_or("")
    }

    pub(crate) fn field_text(&self, node: Node, field: &str) -> &'a str {
        match node.child_by_field_name(field) {
            Some(child) => self.text(child),
            None => "",
        }
    }

    pub(crate) fn unit(
        &mut self,
        kind: &str,
        name: &str,
        qualified: &str,
        outer: Node,
        body: Option<Node>,
        exported: bool,
        parent: Option<usize>,
    ) -> usize {
        let start = outer.start_byte();
        let end = outer.end_byte();
        let cut = match body {
            Some(b) => b.start_byte().clamp(start, end),
            None => end,
        };
        let signature = truncate(
            collapse(self.src.get(start..cut).unwrap_or("")),
            SIGNATURE_LIMIT,
        );
        self.out.units.push(Unit {
            kind: kind.to_string(),
            name: name.to_string(),
            qualified: qualified.to_string(),
            signature,
            exported,
            start_byte: start,
            end_byte: end,
            start_line: line_of(outer),
            end_line: outer.end_position().row as u32 + 1,
            parent,
        });
        self.out.units.len() - 1
    }

    pub(crate) fn reference(&mut self, name: &str, kind: &'static str, line: u32) {
        if name.is_empty() {
            return;
        }
        if !self.seen.insert((name.to_string(), kind, line)) {
            return;
        }
        self.out.refs.push(Reference {
            name: name.to_string(),
            kind: kind.to_string(),
            line,
        });
    }

    pub(crate) fn import(&mut self, source: &str, names: Vec<String>, line: u32) {
        if source.is_empty() {
            return;
        }
        self.out.imports.push(Import {
            source: source.to_string(),
            names,
            line,
        });
    }

    pub(crate) fn css_vars(&mut self, text: &str, first_line: u32) {
        if !text.contains("var(") {
            return;
        }
        for (at, _) in text.match_indices("var(") {
            let rest = text[at + 4..].trim_start();
            if !rest.starts_with("--") {
                continue;
            }
            let end = rest
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
                .unwrap_or(rest.len());
            let name = &rest[..end];
            let line = first_line + text[..at].matches('\n').count() as u32;
            self.reference(name, "css_var", line);
        }
    }
}
