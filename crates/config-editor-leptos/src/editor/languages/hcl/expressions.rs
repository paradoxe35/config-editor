//! The values a document writes as expressions (`var.z`, `"${var.x}-y"`,
//! a heredoc holding `${…}`): read as their `${…}` text, which no other
//! language tells from the text a literal `"$${x}"` reads as, so a
//! conversion lists them as losses.

use hcl_edit::expr::Expression;
use hcl_edit::template::Element;
use serde_json::Value;

/// Stands in for each expression: a string no document holds.
const MARK: &str = "\u{0}expression\u{0}";

/// The JSON pointer of each value `source` writes as an expression.
pub(super) fn expression_pointers(source: &str) -> Vec<String> {
    let text = source.strip_prefix('\u{FEFF}').unwrap_or(source);
    let Ok(mut body) = text.parse::<hcl_edit::structure::Body>() else {
        return Vec::new();
    };
    if !mark_body(&mut body) {
        return Vec::new();
    }
    let Ok(value) = hcl::from_body::<Value>(body.into()) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    marked(&value, &mut String::new(), &mut found);
    found
}

fn mark_body(body: &mut hcl_edit::structure::Body) -> bool {
    let mut any = false;
    for mut structure in body.iter_mut() {
        if let Some(mut attribute) = structure.as_attribute_mut() {
            any |= mark(attribute.value_mut());
        } else if let Some(block) = structure.as_block_mut() {
            any |= mark_body(&mut block.body);
        }
    }
    any
}

/// `expression` with each expression in it (not a literal value) marked;
/// whether there was one.
fn mark(expression: &mut Expression) -> bool {
    let literal = |template: &hcl_edit::template::Template| template.iter().all(|element| matches!(element, Element::Literal(_)));
    let is_expression = match expression {
        Expression::Null(_) | Expression::Bool(_) | Expression::Number(_) | Expression::String(_) => false,
        Expression::StringTemplate(template) => !template.iter().all(|element| matches!(element, Element::Literal(_))),
        Expression::HeredocTemplate(heredoc) => !literal(&heredoc.template),
        Expression::Array(items) => return items.iter_mut().fold(false, |any, item| mark(item) | any),
        Expression::Object(object) => return object.iter_mut().fold(false, |any, (_, value)| mark(value.expr_mut()) | any),
        // A reference, call, operation, condition or `for`.
        _ => true,
    };
    if is_expression {
        *expression = Expression::String(hcl_edit::Decorated::new(MARK.to_string()));
    }
    is_expression
}

fn marked(value: &Value, pointer: &mut String, found: &mut Vec<String>) {
    match value {
        Value::String(text) if text == MARK => found.push(pointer.clone()),
        Value::Object(map) => {
            for (key, child) in map {
                let length = pointer.len();
                pointer.push('/');
                pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
                marked(child, pointer, found);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                let length = pointer.len();
                pointer.push_str(&format!("/{index}"));
                marked(item, pointer, found);
                pointer.truncate(length);
            }
        }
        _ => {}
    }
}
