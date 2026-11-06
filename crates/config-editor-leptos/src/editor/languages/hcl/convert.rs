//! Writing a JSON value as this language.

use super::*;

/// Paths of the maps a document writes as labelled blocks (`clients "acme"
/// { … }` makes `["clients"]` a map with one label level), with the number
/// of labels.
pub(super) type LabelledBlocks = std::collections::HashMap<Vec<String>, usize>;

/// The labelled blocks of `source`, so a rewrite keeps writing them that way.
pub(super) fn labelled_blocks(source: &str) -> LabelledBlocks {
    fn walk(body: &hcl::Body, path: &mut Vec<String>, found: &mut LabelledBlocks) {
        for block in body.blocks() {
            path.push(block.identifier().to_string());
            let labels: Vec<String> = block.labels().iter().map(|label| label.as_str().to_string()).collect();
            if !labels.is_empty() {
                found.insert(path.clone(), labels.len());
            }
            let depth = path.len();
            path.extend(labels);
            walk(block.body(), path, found);
            path.truncate(depth - 1);
        }
    }
    let mut found = LabelledBlocks::new();
    if let Ok(body) = hcl::from_str::<hcl::Body>(source) {
        walk(&body, &mut Vec::new(), &mut found);
    }
    found
}

pub(super) fn hcl_body(
    map: &serde_json::Map<String, Value>,
    labelled: &LabelledBlocks,
    path: &[String],
) -> Result<hcl::Body, String> {
    let mut body = hcl::Body::builder();
    for (key, value) in map {
        let identifier = hcl::Identifier::new(key.as_str()).map_err(|_| {
            format!("\"{key}\" is not an HCL identifier, so it cannot be a top-level key")
        })?;
        let mut child_path = path.to_vec();
        child_path.push(key.clone());
        match value {
            // Written as labelled blocks before: one block per entry.
            Value::Object(children) if labelled.contains_key(&child_path) => {
                for block in labelled_entries(&identifier, children, labelled[&child_path], &child_path, labelled)? {
                    body = body.add_block(block);
                }
            }
            Value::Object(children)
                if !children.is_empty() && children.keys().all(|child| is_ident(child)) =>
            {
                body = body.add_block(hcl::Block {
                    identifier,
                    labels: Vec::new(),
                    body: hcl_body(children, labelled, &child_path)?,
                });
            }
            _ => body = body.add_attribute(hcl::Attribute::new(identifier, hcl_expression(value))),
        }
    }
    Ok(body.build())
}

/// The blocks `identifier "label" … { body }` for a map written with `depth`
/// label levels; an entry that is not an object at a level ends the labels
/// there.
fn labelled_entries(
    identifier: &hcl::Identifier,
    map: &serde_json::Map<String, Value>,
    depth: usize,
    path: &[String],
    labelled: &LabelledBlocks,
) -> Result<Vec<hcl::Block>, String> {
    let mut blocks = Vec::new();
    for (label, value) in map {
        let mut labels = vec![hcl::BlockLabel::String(label.clone())];
        let mut current = value;
        let mut body_path = path.to_vec();
        body_path.push(label.clone());
        while labels.len() < depth
            && let Value::Object(inner) = current
            && inner.len() == 1
            && let Some((next, inner_value)) = inner.iter().next()
            && inner_value.is_object()
        {
            labels.push(hcl::BlockLabel::String(next.clone()));
            body_path.push(next.clone());
            current = inner_value;
        }
        let Value::Object(body) = current else {
            return Err(format!("{}: a labelled block needs an object", body_path.join(".")));
        };
        blocks.push(hcl::Block {
            identifier: identifier.clone(),
            labels,
            body: hcl_body(body, labelled, &body_path)?,
        });
    }
    Ok(blocks)
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
