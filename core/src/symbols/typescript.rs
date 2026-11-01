use std::collections::HashSet;

use tree_sitter::Node;

use super::{Cx, line_of, walk};

pub(crate) fn run(cx: &mut Cx, root: Node) {
    let mut listed: HashSet<String> = HashSet::new();
    let mut cursor = root.walk();
    let children: Vec<Node> = root.named_children(&mut cursor).collect();
    let first = cx.out.units.len();
    for child in children {
        top(cx, child, &mut listed);
    }
    for unit in cx.out.units.iter_mut().skip(first) {
        if unit.parent.is_none() && listed.contains(&unit.name) {
            unit.exported = true;
        }
    }
    references(cx, root);
}

fn top(cx: &mut Cx, node: Node, listed: &mut HashSet<String>) {
    match node.kind() {
        "export_statement" => export(cx, node, listed),
        "import_statement" => import(cx, node),
        _ => declaration(cx, node, node, false, None),
    }
}

fn strip_quotes(text: &str) -> String {
    text.trim_matches(|c| c == '"' || c == '\'' || c == '`')
        .to_string()
}

fn export(cx: &mut Cx, node: Node, listed: &mut HashSet<String>) {
    let source = node.child_by_field_name("source");
    let mut cursor = node.walk();
    let clause = node
        .named_children(&mut cursor)
        .find(|c| c.kind() == "export_clause");
    let mut names = Vec::new();
    if let Some(clause) = clause {
        let mut inner = clause.walk();
        for spec in clause.named_children(&mut inner) {
            if spec.kind() == "export_specifier" {
                let local = cx.field_text(spec, "name").to_string();
                if source.is_none() {
                    listed.insert(local.clone());
                }
                names.push(local);
            }
        }
    }
    if let Some(source) = source {
        let from = strip_quotes(cx.text(source));
        cx.import(&from, names, line_of(node));
        return;
    }
    if let Some(decl) = node.child_by_field_name("declaration") {
        declaration(cx, node, decl, true, None);
        return;
    }
    if let Some(value) = node.child_by_field_name("value") {
        match value.kind() {
            "function_expression" | "arrow_function" | "generator_function" => {
                cx.unit("function", "default", "default", node, value.child_by_field_name("body"), true, None);
            }
            "class" => {
                let id = cx.unit("class", "default", "default", node, value.child_by_field_name("body"), true, None);
                members(cx, value, "default", id);
            }
            _ => {}
        }
    }
}

fn import(cx: &mut Cx, node: Node) {
    let Some(source) = node.child_by_field_name("source") else {
        return;
    };
    let from = strip_quotes(cx.text(source));
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "import_clause" {
            continue;
        }
        let mut inner = child.walk();
        for part in child.named_children(&mut inner) {
            match part.kind() {
                "identifier" => names.push(cx.text(part).to_string()),
                "named_imports" => {
                    let mut specs = part.walk();
                    for spec in part.named_children(&mut specs) {
                        if spec.kind() == "import_specifier" {
                            names.push(cx.field_text(spec, "name").to_string());
                        }
                    }
                }
                "namespace_import" => names.push("*".to_string()),
                _ => {}
            }
        }
    }
    cx.import(&from, names, line_of(node));
}

fn declaration(cx: &mut Cx, outer: Node, decl: Node, exported: bool, parent: Option<usize>) {
    match decl.kind() {
        "function_declaration" | "generator_function_declaration" | "function_signature" => {
            let name = cx.field_text(decl, "name").to_string();
            let qualified = qualify(cx, parent, &name);
            cx.unit("function", &name, &qualified, outer, decl.child_by_field_name("body"), exported, parent);
        }
        "class_declaration" | "abstract_class_declaration" => {
            let name = cx.field_text(decl, "name").to_string();
            let qualified = qualify(cx, parent, &name);
            let id = cx.unit("class", &name, &qualified, outer, decl.child_by_field_name("body"), exported, parent);
            members(cx, decl, &qualified, id);
        }
        "interface_declaration" => {
            let name = cx.field_text(decl, "name").to_string();
            let qualified = qualify(cx, parent, &name);
            cx.unit("interface", &name, &qualified, outer, decl.child_by_field_name("body"), exported, parent);
        }
        "type_alias_declaration" => {
            let name = cx.field_text(decl, "name").to_string();
            let qualified = qualify(cx, parent, &name);
            cx.unit("type", &name, &qualified, outer, decl.child_by_field_name("value"), exported, parent);
        }
        "enum_declaration" => {
            let name = cx.field_text(decl, "name").to_string();
            let qualified = qualify(cx, parent, &name);
            cx.unit("enum", &name, &qualified, outer, decl.child_by_field_name("body"), exported, parent);
        }
        "lexical_declaration" | "variable_declaration" => variables(cx, outer, decl, exported, parent),
        "ambient_declaration" => {
            let mut cursor = decl.walk();
            let inner = decl.named_children(&mut cursor).next();
            if let Some(inner) = inner {
                declaration(cx, outer, inner, exported, parent);
            }
        }
        "internal_module" | "module" => {
            let name = cx.field_text(decl, "name").to_string();
            let qualified = qualify(cx, parent, &name);
            let body = decl.child_by_field_name("body");
            let id = cx.unit("namespace", &name, &qualified, outer, body, exported, parent);
            if let Some(body) = body {
                let mut cursor = body.walk();
                let children: Vec<Node> = body.named_children(&mut cursor).collect();
                for child in children {
                    if child.kind() == "export_statement" {
                        if let Some(inner) = child.child_by_field_name("declaration") {
                            declaration(cx, child, inner, true, Some(id));
                        }
                    } else {
                        declaration(cx, child, child, false, Some(id));
                    }
                }
            }
        }
        _ => {}
    }
}

fn qualify(cx: &Cx, parent: Option<usize>, name: &str) -> String {
    match parent {
        Some(p) => format!("{}.{}", cx.out.units[p].qualified, name),
        None => name.to_string(),
    }
}

fn variables(cx: &mut Cx, outer: Node, decl: Node, exported: bool, parent: Option<usize>) {
    let mut cursor = decl.walk();
    let declarators: Vec<Node> = decl
        .named_children(&mut cursor)
        .filter(|c| c.kind() == "variable_declarator")
        .collect();
    for (index, declarator) in declarators.into_iter().enumerate() {
        let Some(name_node) = declarator.child_by_field_name("name") else {
            continue;
        };
        if name_node.kind() != "identifier" {
            continue;
        }
        let name = cx.text(name_node).to_string();
        let span = if index == 0 { outer } else { declarator };
        let qualified = qualify(cx, parent, &name);
        let value = declarator.child_by_field_name("value");
        match value {
            Some(v)
                if matches!(
                    v.kind(),
                    "arrow_function" | "function_expression" | "function" | "generator_function"
                ) =>
            {
                cx.unit("function", &name, &qualified, span, v.child_by_field_name("body"), exported, parent);
            }
            Some(v) if exported => {
                cx.unit("const", &name, &qualified, span, Some(v), exported, parent);
            }
            None if exported => {
                cx.unit("const", &name, &qualified, span, None, exported, parent);
            }
            _ => {}
        }
    }
}

fn members(cx: &mut Cx, class: Node, owner: &str, id: usize) {
    let Some(body) = class.child_by_field_name("body") else {
        return;
    };
    let mut cursor = body.walk();
    let children: Vec<Node> = body.named_children(&mut cursor).collect();
    for member in children {
        match member.kind() {
            "method_definition" | "abstract_method_signature" => {
                let name = cx.field_text(member, "name").to_string();
                let qualified = format!("{owner}.{name}");
                cx.unit("method", &name, &qualified, member, member.child_by_field_name("body"), false, Some(id));
            }
            "public_field_definition" => {
                let Some(value) = member.child_by_field_name("value") else {
                    continue;
                };
                if !matches!(value.kind(), "arrow_function" | "function_expression" | "function") {
                    continue;
                }
                let name = cx.field_text(member, "name").to_string();
                let qualified = format!("{owner}.{name}");
                cx.unit("method", &name, &qualified, member, value.child_by_field_name("body"), false, Some(id));
            }
            _ => {}
        }
    }
}

fn references(cx: &mut Cx, root: Node) {
    walk(root, |node| match node.kind() {
        "call_expression" => call(cx, node),
        "new_expression" => {
            if let Some(ctor) = node.child_by_field_name("constructor") {
                let name = callee(cx, ctor);
                cx.reference(&name, "call", line_of(node));
            }
        }
        "type_identifier" => {
            let is_name = node
                .parent()
                .and_then(|p| p.child_by_field_name("name"))
                .is_some_and(|n| n.id() == node.id());
            if !is_name {
                let name = cx.text(node).to_string();
                cx.reference(&name, "type", line_of(node));
            }
        }
        "string" | "template_string" => {
            let text = cx.text(node);
            cx.css_vars(text, line_of(node));
        }
        _ => {}
    });
}

fn callee(cx: &Cx, node: Node) -> String {
    match node.kind() {
        "identifier" => cx.text(node).to_string(),
        "member_expression" => cx.field_text(node, "property").to_string(),
        "non_null_expression" | "parenthesized_expression" => {
            let mut cursor = node.walk();
            let inner = node.named_children(&mut cursor).next();
            match inner {
                Some(inner) => callee(cx, inner),
                None => String::new(),
            }
        }
        _ => String::new(),
    }
}

fn call(cx: &mut Cx, node: Node) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    let line = line_of(node);
    if function.kind() == "import" {
        if let Some(first) = first_string(cx, node) {
            cx.import(&first, Vec::new(), line);
        }
        return;
    }
    let name = callee(cx, function);
    if name == "invoke" {
        if let Some(command) = first_string(cx, node) {
            cx.reference(&command, "invoke", line);
            return;
        }
    }
    cx.reference(&name, "call", line);
}

fn first_string(cx: &Cx, call: Node) -> Option<String> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    let first = args.named_children(&mut cursor).next()?;
    if first.kind() != "string" {
        return None;
    }
    Some(strip_quotes(cx.text(first)))
}
