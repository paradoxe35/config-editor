//! Writing a JSON value as this language.

use super::*;

pub(super) fn hcl_body(map: &serde_json::Map<String, Value>) -> Result<hcl::Body, String> {
    let mut body = hcl::Body::builder();
    for (key, value) in map {
        let identifier = hcl::Identifier::new(key.as_str()).map_err(|_| {
            format!("\"{key}\" is not an HCL identifier, so it cannot be a top-level key")
        })?;
        match value {
            Value::Object(children)
                if !children.is_empty() && children.keys().all(|child| is_ident(child)) =>
            {
                body = body.add_block(hcl::Block {
                    identifier,
                    labels: Vec::new(),
                    body: hcl_body(children)?,
                });
            }
            _ => body = body.add_attribute(hcl::Attribute::new(identifier, hcl_expression(value))),
        }
    }
    Ok(body.build())
}

/// A document value as an HCL expression; identifier keys stay unquoted.
pub(super) fn hcl_expression(value: &Value) -> hcl::Expression {
    use hcl::expr::{Expression, Object, ObjectKey};
    match value {
        Value::Null => Expression::Null,
        Value::Bool(flag) => Expression::Bool(*flag),
        Value::Number(number) => hcl::to_expression(number).unwrap_or(Expression::Null),
        Value::String(text) => Expression::String(text.clone()),
        Value::Array(items) => Expression::Array(items.iter().map(hcl_expression).collect()),
        Value::Object(map) => Expression::Object(
            map.iter()
                .map(|(key, value)| {
                    let key = match hcl::Identifier::new(key.as_str()) {
                        Ok(identifier) if is_ident(key) => ObjectKey::Identifier(identifier),
                        _ => ObjectKey::Expression(Expression::String(key.clone())),
                    };
                    (key, hcl_expression(value))
                })
                .collect::<Object<ObjectKey, Expression>>(),
        ),
    }
}
