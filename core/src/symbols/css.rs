use tree_sitter::Node;

use super::{Cx, collapse, line_of, walk};

pub(crate) fn run(cx: &mut Cx, root: Node) {
    visit(cx, root, None);
    walk(root, |node| {
        if node.kind() == "call_expression" {
            let text = cx.text(node);
            if text.starts_with("var(") {
                cx.css_vars(text, line_of(node));
            }
        }
    });
}

fn visit(cx: &mut Cx, node: Node, parent: Option<usize>) {
    let mut cursor = node.walk();
    let children: Vec<Node> = node.named_children(&mut cursor).collect();
    for child in children {
        match child.kind() {
            "rule_set" => {
                let selector = match child.child_by_field_name("selectors") {
                    Some(s) => collapse(cx.text(s)),
                    None => {
                        let mut inner = child.walk();
                        let found = child
                            .named_children(&mut inner)
                            .find(|c| c.kind() == "selectors");
                        match found {
                            Some(s) => collapse(cx.text(s)),
                            None => String::new(),
                        }
                    }
                };
                let mut inner = child.walk();
                let block = child.named_children(&mut inner).find(|c| c.kind() == "block");
                let id = cx.unit("selector", &selector, &selector, child, block, false, parent);
                if let Some(block) = block {
                    visit(cx, block, Some(id));
                }
            }
            "declaration" => {
                let mut inner = child.walk();
                let property = child
                    .named_children(&mut inner)
                    .find(|c| c.kind() == "property_name");
                let Some(property) = property else {
                    continue;
                };
                let name = cx.text(property).to_string();
                if !name.starts_with("--") {
                    continue;
                }
                let mut values = child.walk();
                let value = child
                    .named_children(&mut values)
                    .find(|c| c.id() != property.id());
                cx.unit("custom_property", &name, &name, child, value, true, parent);
            }
            "block" | "media_statement" | "supports_statement" | "layer_statement"
            | "container_statement" | "scope_statement" => visit(cx, child, parent),
            _ => {}
        }
    }
}
