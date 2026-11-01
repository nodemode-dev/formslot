use tree_sitter::Node;

use super::{Cx, line_of, walk};

pub(crate) fn run(cx: &mut Cx, root: Node) {
    let mut cursor = root.walk();
    let children: Vec<Node> = root.named_children(&mut cursor).collect();
    for child in children {
        item(cx, child, None, "");
    }
    walk(root, |node| match node.kind() {
        "call" => {
            if let Some(function) = node.child_by_field_name("function") {
                let name = match function.kind() {
                    "identifier" => cx.text(function).to_string(),
                    "attribute" => cx.field_text(function, "attribute").to_string(),
                    _ => String::new(),
                };
                cx.reference(&name, "call", line_of(node));
            }
        }
        "import_statement" => import_plain(cx, node),
        "import_from_statement" => import_from(cx, node),
        _ => {}
    });
}

fn is_constant(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_uppercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

fn item(cx: &mut Cx, node: Node, parent: Option<usize>, owner: &str) {
    let (outer, def) = if node.kind() == "decorated_definition" {
        match node.child_by_field_name("definition") {
            Some(def) => (node, def),
            None => return,
        }
    } else {
        (node, node)
    };
    match def.kind() {
        "function_definition" => {
            let name = cx.field_text(def, "name").to_string();
            let qualified = if owner.is_empty() {
                name.clone()
            } else {
                format!("{owner}.{name}")
            };
            let kind = if parent.is_some() { "method" } else { "function" };
            let exported = !name.starts_with('_');
            cx.unit(kind, &name, &qualified, outer, def.child_by_field_name("body"), exported, parent);
        }
        "class_definition" if parent.is_none() => {
            let name = cx.field_text(def, "name").to_string();
            let body = def.child_by_field_name("body");
            let exported = !name.starts_with('_');
            let id = cx.unit("class", &name, &name, outer, body, exported, None);
            if let Some(body) = body {
                let mut cursor = body.walk();
                let members: Vec<Node> = body.named_children(&mut cursor).collect();
                for member in members {
                    item(cx, member, Some(id), &name);
                }
            }
        }
        "expression_statement" if parent.is_none() => {
            let mut cursor = def.walk();
            let assignment = def
                .named_children(&mut cursor)
                .find(|c| c.kind() == "assignment");
            let Some(assignment) = assignment else {
                return;
            };
            let Some(left) = assignment.child_by_field_name("left") else {
                return;
            };
            if left.kind() != "identifier" {
                return;
            }
            let name = cx.text(left).to_string();
            if !is_constant(&name) {
                return;
            }
            cx.unit("constant", &name, &name, outer, assignment.child_by_field_name("right"), true, None);
        }
        _ => {}
    }
}

fn import_plain(cx: &mut Cx, node: Node) {
    let mut cursor = node.walk();
    let names: Vec<Node> = node.children_by_field_name("name", &mut cursor).collect();
    for name in names {
        let target = if name.kind() == "aliased_import" {
            cx.field_text(name, "name")
        } else {
            cx.text(name)
        };
        cx.import(target, Vec::new(), line_of(node));
    }
}

fn import_from(cx: &mut Cx, node: Node) {
    let module = cx.field_text(node, "module_name").to_string();
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.children_by_field_name("name", &mut cursor) {
        let text = if child.kind() == "aliased_import" {
            cx.field_text(child, "name")
        } else {
            cx.text(child)
        };
        names.push(text.to_string());
    }
    let mut inner = node.walk();
    if node
        .named_children(&mut inner)
        .any(|c| c.kind() == "wildcard_import")
    {
        names.push("*".to_string());
    }
    cx.import(&module, names, line_of(node));
}
