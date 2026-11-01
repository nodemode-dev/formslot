use tree_sitter::Node;

use super::{Cx, collapse, line_of, walk};

pub(crate) fn run(cx: &mut Cx, root: Node) {
    items(cx, root, "", None, false);
    walk(root, |node| match node.kind() {
        "call_expression" => {
            if let Some(function) = node.child_by_field_name("function") {
                let name = callee(cx, function);
                cx.reference(&name, "call", line_of(node));
            }
        }
        "use_declaration" => {
            if let Some(arg) = node.child_by_field_name("argument") {
                let source = collapse(cx.text(arg));
                let mut names = Vec::new();
                leaves(cx, arg, &mut names);
                cx.import(&source, names, line_of(node));
            }
        }
        _ => {}
    });
}

fn callee(cx: &Cx, node: Node) -> String {
    match node.kind() {
        "identifier" => cx.text(node).to_string(),
        "scoped_identifier" => cx.field_text(node, "name").to_string(),
        "field_expression" => cx.field_text(node, "field").to_string(),
        "generic_function" => match node.child_by_field_name("function") {
            Some(inner) => callee(cx, inner),
            None => String::new(),
        },
        _ => String::new(),
    }
}

fn leaves(cx: &Cx, node: Node, out: &mut Vec<String>) {
    match node.kind() {
        "identifier" | "self" | "crate" | "super" => out.push(cx.text(node).to_string()),
        "scoped_identifier" => out.push(cx.field_text(node, "name").to_string()),
        "use_as_clause" => out.push(cx.field_text(node, "alias").to_string()),
        "use_wildcard" => out.push("*".to_string()),
        "scoped_use_list" => {
            if let Some(list) = node.child_by_field_name("list") {
                leaves(cx, list, out);
            }
        }
        "use_list" => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                leaves(cx, child, out);
            }
        }
        _ => {}
    }
}

fn is_pub(cx: &Cx, node: Node) -> bool {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .any(|c| c.kind() == "visibility_modifier" && cx.text(c).starts_with("pub"))
}

fn is_tauri(cx: &Cx, attribute: Node) -> bool {
    let text: String = cx.text(attribute).split_whitespace().collect();
    text.starts_with("#[tauri::command")
}

fn type_name(cx: &Cx, node: Node) -> String {
    match node.kind() {
        "type_identifier" => cx.text(node).to_string(),
        "generic_type" => match node.child_by_field_name("type") {
            Some(inner) => type_name(cx, inner),
            None => cx.text(node).to_string(),
        },
        "scoped_type_identifier" => cx.field_text(node, "name").to_string(),
        "reference_type" => match node.child_by_field_name("type") {
            Some(inner) => type_name(cx, inner),
            None => cx.text(node).to_string(),
        },
        _ => collapse(cx.text(node)),
    }
}

fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}::{name}")
    }
}

fn items(cx: &mut Cx, container: Node, prefix: &str, parent: Option<usize>, in_type: bool) {
    if !cx.enter() {
        return;
    }
    item_list(cx, container, prefix, parent, in_type);
    cx.leave();
}

fn item_list(cx: &mut Cx, container: Node, prefix: &str, parent: Option<usize>, in_type: bool) {
    let mut cursor = container.walk();
    let children: Vec<Node> = container.named_children(&mut cursor).collect();
    let mut tauri = false;
    for node in children {
        match node.kind() {
            "attribute_item" => {
                if is_tauri(cx, node) {
                    tauri = true;
                }
                continue;
            }
            "line_comment" | "block_comment" | "inner_attribute_item" => continue,
            _ => {}
        }
        let command = tauri;
        tauri = false;
        let exported = is_pub(cx, node);
        match node.kind() {
            "function_item" | "function_signature_item" => {
                let name = cx.field_text(node, "name").to_string();
                let qualified = join(prefix, &name);
                let kind = if command {
                    "tauri_command"
                } else if in_type {
                    "method"
                } else {
                    "function"
                };
                cx.unit(kind, &name, &qualified, node, node.child_by_field_name("body"), exported, parent);
            }
            "struct_item" => simple(cx, node, "struct", prefix, parent, exported),
            "enum_item" => simple(cx, node, "enum", prefix, parent, exported),
            "type_item" => simple(cx, node, "type", prefix, parent, exported),
            "const_item" | "static_item" => {
                let kind = if node.kind() == "const_item" { "constant" } else { "static" };
                let name = cx.field_text(node, "name").to_string();
                let qualified = join(prefix, &name);
                cx.unit(kind, &name, &qualified, node, node.child_by_field_name("value"), exported, parent);
            }
            "trait_item" => {
                let name = cx.field_text(node, "name").to_string();
                let qualified = join(prefix, &name);
                let body = node.child_by_field_name("body");
                let id = cx.unit("trait", &name, &qualified, node, body, exported, parent);
                if let Some(body) = body {
                    items(cx, body, &qualified, Some(id), true);
                }
            }
            "mod_item" => {
                let name = cx.field_text(node, "name").to_string();
                let qualified = join(prefix, &name);
                let body = node.child_by_field_name("body");
                let id = cx.unit("mod", &name, &qualified, node, body, exported, parent);
                if let Some(body) = body {
                    items(cx, body, &qualified, Some(id), false);
                }
            }
            "impl_item" => {
                let owner = match node.child_by_field_name("type") {
                    Some(t) => type_name(cx, t),
                    None => String::new(),
                };
                if let Some(body) = node.child_by_field_name("body") {
                    items(cx, body, &join(prefix, &owner), parent, true);
                }
            }
            _ => {}
        }
    }
}

fn simple(cx: &mut Cx, node: Node, kind: &str, prefix: &str, parent: Option<usize>, exported: bool) {
    let name = cx.field_text(node, "name").to_string();
    let qualified = join(prefix, &name);
    cx.unit(kind, &name, &qualified, node, node.child_by_field_name("body"), exported, parent);
}
