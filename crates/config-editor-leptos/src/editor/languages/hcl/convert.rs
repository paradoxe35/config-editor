//! Writing a JSON value as this language.

use super::*;

/// Paths of the maps a document writes as labelled blocks (`clients "acme"
/// { … }` makes `["clients"]` a map with one label level), with the number
/// of labels.
pub(super) type LabelledBlocks = std::collections::HashMap<Vec<String>, usize>;

/// How the host writes its maps, so a rewrite writes them the same way.
#[derive(Default)]
pub(super) struct HostForms {
    /// Maps written as labelled blocks.
    pub(super) labelled: LabelledBlocks,
    /// Maps written as attributes (`udp = { … }`), not blocks.
    pub(super) attributes: std::collections::HashSet<Vec<String>>,
    /// Blocks written on one line (`clients "a" { secret = "s" }`).
    pub(super) one_line: std::collections::HashSet<Vec<String>>,
    /// Strings the host writes as templates (`"${var.port}"`): written so
    /// again, not escaped as text.
    pub(super) templates: std::collections::HashSet<String>,
    /// Paths written as unlabelled blocks (`dns { … }`, once or repeated):
    /// a list there is written as repeated blocks, an empty map as a block.
    pub(super) blocks: std::collections::HashSet<Vec<String>>,
}

pub(super) fn host_forms(source: &str) -> HostForms {
    let scan = analyze(source, 0);
    let attributes = scan
        .attributes
        .iter()
        .filter(|attribute| {
            attribute.prefix.is_empty()
                && source.get(attribute.value_span.0..attribute.value_span.1).is_some_and(|value| value.starts_with('{'))
        })
        .map(|attribute| attribute_path(&scan, attribute))
        .collect();
    HostForms {
        labelled: labelled_blocks(source),
        attributes,
        one_line: one_line_headers(source, &scan).into_iter().map(|(path, _)| path).collect(),
        templates: scan
            .attributes
            .iter()
            .filter_map(|attribute| source.get(attribute.value_span.0..attribute.value_span.1))
            .filter(|written| is_template(written))
            .filter_map(|written| hcl::from_str::<serde_json::Value>(&format!("v = {written}")).ok())
            .filter_map(|value| value.get("v").and_then(serde_json::Value::as_str).map(str::to_string))
            .collect(),
        blocks: scan
            .headers
            .iter()
            .filter(|header| header.labels.is_empty())
            .map(|header| {
                let mut path: Vec<String> =
                    body_path(&scan, header.parent).into_iter().filter(|segment| segment.parse::<usize>().is_err()).collect();
                path.push(header.block_type.clone());
                path
            })
            .collect(),
    }
}

/// The blocks of `source` opened and closed on their header's line, with
/// the line each is on.
fn one_line_headers(source: &str, scan: &Scan) -> Vec<(Vec<String>, usize)> {
    scan.headers
        .iter()
        .filter_map(|header| {
            let start = header.type_span.0;
            let line_end = source[start..].find('\n').map_or(source.len(), |at| start + at);
            let open = start + source[start..line_end].find('{')?;
            matching_close(&source[open..line_end], '}')?;
            Some((body_path(scan, header.child), source[..start].matches('\n').count()))
        })
        .collect()
}

/// `rendered` (as the formatter writes it) with each block the host wrote on
/// one line, and that still holds one one-line attribute, on one line again.
pub(super) fn fold_one_line_blocks(rendered: &str, one_line: &std::collections::HashSet<Vec<String>>) -> String {
    if one_line.is_empty() {
        return rendered.to_string();
    }
    let scan = analyze(rendered, 0);
    let mut lines: Vec<String> = rendered.split('\n').map(str::to_string).collect();
    let mut headers: Vec<(Vec<String>, usize)> = scan
        .headers
        .iter()
        .map(|header| (body_path(&scan, header.child), rendered[..header.type_span.0].matches('\n').count()))
        .filter(|(path, _)| one_line.contains(path))
        .collect();
    headers.sort_by_key(|(_, line)| std::cmp::Reverse(*line));
    for (_, line) in headers {
        let (Some(header), Some(attribute), Some(close)) = (lines.get(line), lines.get(line + 1), lines.get(line + 2)) else {
            continue;
        };
        let attribute_code = attribute.trim();
        let single = header.trim_end().ends_with('{')
            && close.trim() == "}"
            && attribute_code.contains('=')
            && !attribute_code.ends_with(['{', '[', '('])
            && !attribute_code.contains("<<");
        if single {
            let folded = format!("{} {} }}", header.trim_end(), attribute_code.split_whitespace().collect::<Vec<_>>().join(" "));
            lines.splice(line..line + 3, [folded]);
        }
    }
    lines.join("\n")
}

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
    forms: &HostForms,
    path: &[String],
) -> Result<hcl::Body, String> {
    let labelled = &forms.labelled;
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
                for block in labelled_entries(&identifier, children, labelled[&child_path], &child_path, forms)? {
                    body = body.add_block(block);
                }
            }
            // A list the host writes as repeated blocks.
            Value::Array(items)
                if forms.blocks.contains(&child_path)
                    && !items.is_empty()
                    && items.iter().all(|item| item.as_object().is_some_and(|map| map.keys().all(|key| is_ident(key)))) =>
            {
                for item in items {
                    if let Value::Object(children) = item {
                        body = body.add_block(hcl::Block {
                            identifier: identifier.clone(),
                            labels: Vec::new(),
                            body: hcl_body(children, forms, &child_path)?,
                        });
                    }
                }
            }
            Value::Object(children)
                if (!children.is_empty() || forms.blocks.contains(&child_path))
                    && children.keys().all(|child| is_ident(child))
                    && !forms.attributes.contains(&child_path) =>
            {
                body = body.add_block(hcl::Block {
                    identifier,
                    labels: Vec::new(),
                    body: hcl_body(children, forms, &child_path)?,
                });
            }
            _ => body = body.add_attribute(hcl::Attribute::new(identifier, hcl_expression_in(value, &forms.templates))),
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
    forms: &HostForms,
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
            body: hcl_body(body, forms, &body_path)?,
        });
    }
    Ok(blocks)
}

/// Whether `written` (a value as the host writes it) is a quoted string
/// holding a template sequence (an interpolation or directive not escaped).
pub(super) fn is_template(written: &str) -> bool {
    let plain = written.replace("$${", "").replace("%%{", "");
    written.starts_with('"') && (plain.contains("${") || plain.contains("%{"))
}

/// [`hcl_expression`], writing the strings in `templates` as templates.
pub(super) fn hcl_expression_in(value: &Value, templates: &std::collections::HashSet<String>) -> hcl::Expression {
    match value {
        Value::String(text) if templates.contains(text) => {
            hcl::Expression::TemplateExpr(Box::new(hcl::TemplateExpr::QuotedString(text.clone())))
        }
        Value::Array(items) if !templates.is_empty() => {
            hcl::Expression::Array(items.iter().map(|item| hcl_expression_in(item, templates)).collect())
        }
        _ => hcl_expression(value),
    }
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

/// The unlabelled block at `pointer` when written on one line
/// (`tcp { nodelay = true }`): kept as written by a structural save, where
/// the serializer writes `tcp = …`.
pub(super) fn one_line_block(source: &str, pointer: &str) -> Option<OneLine> {
    let wanted = pointer_segments(pointer);
    let scan = analyze(source, 0);
    let header = scan.headers.iter().find(|header| {
        let mut path = body_path(&scan, header.parent);
        path.push(header.block_type.clone());
        header.labels.is_empty() && path == wanted
    })?;
    let start = header.type_span.0;
    let line_end = source[start..].find('\n').map_or(source.len(), |at| start + at);
    let open = start + source[start..line_end].find('{')?;
    let close = open + matching_close(&source[open..line_end], '}')?;
    Some(OneLine {
        range: start..close + 1,
        written: format!("{} = ", header.block_type),
        value: open..close + 1,
    })
}

/// `source` with the block type or label at `pointer` renamed `new_key` in
/// every block header that writes it (`clients "a" { … }` and
/// `clients "b" { … }` both become `peers …`). `None` when no header does.
pub(super) fn rename_block_key(source: &str, pointer: &str, new_key: &str) -> Option<String> {
    let wanted = pointer_segments(pointer);
    let scan = analyze(source, 0);
    let mut edits: Vec<(usize, usize)> = Vec::new();
    for header in &scan.headers {
        let mut path = body_path(&scan, header.parent);
        path.push(header.block_type.clone());
        if path == wanted {
            edits.push(header.type_span);
            continue;
        }
        for (label, span) in header.labels.iter().zip(&header.label_spans) {
            path.push(label.clone());
            if path == wanted {
                let quoted = source[span.0..span.1].starts_with('"');
                edits.push(if quoted { (span.0 + 1, span.1 - 1) } else { *span });
                break;
            }
        }
    }
    if edits.is_empty() || !is_ident(new_key) && edits.iter().any(|(start, _)| !source[..*start].ends_with('"')) {
        return None;
    }
    edits.sort();
    edits.dedup();
    let mut renamed = source.to_string();
    for (start, end) in edits.into_iter().rev() {
        renamed.replace_range(start..end, new_key);
    }
    Some(renamed)
}
