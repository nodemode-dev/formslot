use tree_sitter::Node;

use super::{Cx, collapse, line_of, walk};

pub(crate) fn run(cx: &mut Cx, root: Node) {
    walk(root, |node| match node.kind() {
        "element" | "script_element" | "style_element" => element(cx, node),
        _ => {}
    });
}

fn attribute_value(cx: &Cx, attribute: Node) -> String {
    let mut cursor = attribute.walk();
    let value = attribute
        .named_children(&mut cursor)
        .find(|c| c.kind() == "quoted_attribute_value" || c.kind() == "attribute_value");
    match value {
        Some(v) => cx
            .text(v)
            .trim_matches(|c| c == '"' || c == '\'')
            .to_string(),
        None => String::new(),
    }
}

fn element(cx: &mut Cx, node: Node) {
    let mut cursor = node.walk();
    let tag = node
        .named_children(&mut cursor)
        .find(|c| c.kind() == "start_tag" || c.kind() == "self_closing_tag");
    let Some(tag) = tag else {
        return;
    };
    let mut attrs = tag.walk();
    let attributes: Vec<Node> = tag
        .named_children(&mut attrs)
        .filter(|c| c.kind() == "attribute")
        .collect();
    let mut id = String::new();
    let mut has_src = false;
    for attribute in attributes {
        let mut inner = attribute.walk();
        let name = attribute
            .named_children(&mut inner)
            .find(|c| c.kind() == "attribute_name")
            .map(|n| cx.text(n).to_string())
            .unwrap_or_default();
        let value = attribute_value(cx, attribute);
        match name.as_str() {
            "id" => id = value,
            "src" => has_src = true,
            "style" => cx.css_vars(&value, line_of(attribute)),
            _ => {}
        }
    }
    let body = {
        let mut inner = node.walk();
        node.named_children(&mut inner).find(|c| c.kind() == "raw_text")
    };
    if let Some(body) = body {
        let text = cx.text(body);
        cx.css_vars(text, line_of(body));
    }
    let kind = match node.kind() {
        "script_element" => "script",
        "style_element" => "style",
        _ => "",
    };
    if !kind.is_empty() && !has_src && body.is_some() {
        let name = if id.is_empty() { kind.to_string() } else { id.clone() };
        let qualified = format!("{kind}@{}", line_of(node));
        let signature = collapse(cx.text(tag));
        let id_unit = cx.unit(kind, &name, &qualified, node, body, false, None);
        cx.out.units[id_unit].signature = signature;
        return;
    }
    if !id.is_empty() {
        cx.unit("element_id", &id, &id, node, node.named_child(0).filter(|c| c.kind() != "start_tag"), false, None);
        let last = cx.out.units.len() - 1;
        cx.out.units[last].signature = collapse(cx.text(tag));
    }
}
