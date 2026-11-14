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
    /// Strings the host writes as bare expressions (`var.azs`, `number`),
    /// which read as `${…}`: written as those expressions again.
    pub(super) expressions: std::collections::HashSet<String>,
    /// Heredocs holding a template sequence, by the text they read as:
    /// written as those heredocs again (moved to another block).
    pub(super) heredocs: std::collections::HashMap<String, hcl::Heredoc>,
    /// Paths written as unlabelled blocks (`dns { … }`, once or repeated):
    /// a list there is written as repeated blocks, an empty map as a block.
    pub(super) blocks: std::collections::HashSet<Vec<String>>,
}

/// The maps `source` writes as object attributes (`udp = { … }`) and as
/// unlabelled blocks.
type MapForms = (std::collections::HashSet<Vec<String>>, std::collections::HashSet<Vec<String>>);

fn map_forms(source: &str, scan: &Scan) -> MapForms {
    let attributes = scan
        .attributes
        .iter()
        .filter(|attribute| {
            attribute.prefix.is_empty()
                && source.get(attribute.value_span.0..attribute.value_span.1).is_some_and(|value| value.starts_with('{'))
        })
        .map(|attribute| attribute_path(scan, attribute))
        .collect();
    let blocks = scan
        .headers
        .iter()
        .filter(|header| header.labels.is_empty())
        .map(|header| {
            let mut path: Vec<String> =
                body_path(scan, header.parent).into_iter().filter(|segment| crate::editor::language::pointer_index(segment).is_none()).collect();
            path.push(header.block_type.clone());
            path
        })
        .collect();
    (attributes, blocks)
}

pub(super) fn host_forms(source: &str) -> HostForms {
    let scan = analyze(source, 0);
    let (attributes, blocks) = map_forms(source, &scan);
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
        expressions: written_expressions(source),
        heredocs: scan
            .attributes
            .iter()
            .filter_map(|attribute| source.get(attribute.value_span.0..attribute.value_span.1))
            .filter_map(super::heredoc::written_heredoc)
            .collect(),
        blocks,
    }
}

/// The first map `patched` writes as a block that `previous` writes as an
/// attribute (`name = { … }`), or the other way round.
pub(super) fn changed_form(previous: &str, patched: &str) -> Option<String> {
    let forms = |source: &str| map_forms(source, &analyze(source, 0));
    let ((attributes_before, blocks_before), (attributes_after, blocks_after)) = (forms(previous), forms(patched));
    let unnumbered = |paths: std::collections::HashSet<Vec<String>>| -> std::collections::HashSet<Vec<String>> {
        paths.into_iter().map(|path| path.into_iter().filter(|segment| crate::editor::language::pointer_index(segment).is_none()).collect()).collect()
    };
    let (attributes_before, attributes_after) = (unnumbered(attributes_before), unnumbered(attributes_after));
    let switched = attributes_before
        .iter()
        .find(|path| blocks_after.contains(*path))
        .or_else(|| blocks_before.iter().find(|path| attributes_after.contains(*path)))?;
    Some(switched.iter().map(|segment| format!("/{}", segment.replace('~', "~0").replace('/', "~1"))).collect())
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
    fn walk(body: &hcl_edit::structure::Body, path: &mut Vec<String>, found: &mut LabelledBlocks) {
        for block in body.iter().filter_map(|structure| structure.as_block()) {
            path.push(block.ident.as_str().to_string());
            let labels: Vec<String> = block.labels.iter().map(|label| label.as_str().to_string()).collect();
            if !labels.is_empty() {
                found.insert(path.clone(), labels.len());
            }
            let depth = path.len();
            path.extend(labels);
            walk(&block.body, path, found);
            path.truncate(depth - 1);
        }
    }
    let mut found = LabelledBlocks::new();
    // Parsed with its objects of many entries in groups (see
    // `big_objects`): blocks are never inside one.
    if let Ok(parsed) = super::big_objects::parse(source, true) {
        walk(&parsed.body, &mut Vec::new(), &mut found);
    }
    found
}

/// The JSON pointer of `path`.
fn pointer_of(path: &[String]) -> String {
    path.iter().map(|segment| format!("/{}", segment.replace('~', "~0").replace('/', "~1"))).collect()
}

pub(super) fn hcl_body(
    map: &serde_json::Map<String, Value>,
    forms: &HostForms,
    path: &[String],
) -> Result<hcl::Body, String> {
    let labelled = &forms.labelled;
    let mut body = hcl::Body::builder();
    for (key, value) in map {
        let mut child_path = path.to_vec();
        child_path.push(key.clone());
        let identifier = hcl::Identifier::new(key.as_str()).map_err(|_| {
            format!(
                "\"{key}\" is not an HCL identifier, so it cannot name an attribute or a block (at {})",
                pointer_of(&child_path)
            )
        })?;
        match value {
            // Labelled blocks before, all of them gone: an empty block.
            Value::Object(children) if labelled.contains_key(&child_path) && children.is_empty() => {
                body = body.add_block(hcl::Block { identifier, labels: Vec::new(), body: hcl::Body::default() });
            }
            // Written as labelled blocks before: one block per entry.
            Value::Object(children) if labelled.contains_key(&child_path) => {
                for block in labelled_entries(&identifier, children, labelled[&child_path], &child_path, forms)? {
                    body = body.add_block(block);
                }
            }
            // A new top-level block type in a host writing labelled blocks:
            // nothing tells which of its keys are labels, so the save waits.
            Value::Object(children) if path.is_empty() && new_labelled_type(key, children, forms) => {
                return Err(format!(
                    "this document writes its blocks with labels, and HCL cannot tell from here which keys of the new `{key}` block are its labels: add it in the HCL text (at {})",
                    pointer_of(&child_path)
                ));
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
            // A block the host writes cannot take a key that is no
            // identifier: written as an attribute (`name = { … }`) it
            // would no longer be a block.
            Value::Object(children) if forms.blocks.contains(&child_path) && !forms.attributes.contains(&child_path) => {
                let key = children.keys().find(|key| !is_ident(key)).cloned().unwrap_or_default();
                let mut at = child_path.clone();
                at.push(key.clone());
                return Err(format!(
                    "\"{key}\" is not an HCL identifier, so it cannot name an attribute of the block {} (at {})",
                    child_path.join("."),
                    pointer_of(&at)
                ));
            }
            Value::Array(items)
                if forms.blocks.contains(&child_path) && !items.is_empty() && items.iter().all(Value::is_object) =>
            {
                let (index, key) = items
                    .iter()
                    .enumerate()
                    .find_map(|(index, item)| Some((index, item.as_object()?.keys().find(|key| !is_ident(key))?.clone())))
                    .unwrap_or_default();
                let mut at = child_path.clone();
                at.extend([index.to_string(), key.clone()]);
                return Err(format!(
                    "\"{key}\" is not an HCL identifier, so it cannot name an attribute of the block {} (at {})",
                    child_path.join("."),
                    pointer_of(&at)
                ));
            }
            _ => body = body.add_attribute(hcl::Attribute::new(identifier, hcl_expression_in(value, forms))),
        }
    }
    Ok(body.build())
}

/// Whether `key` (holding `children`) is a top-level block type the host
/// does not write, in a host writing its top-level blocks with labels,
/// whose entries could be labels (each a map): written as nested blocks,
/// that host's readers would likely reject it.
fn new_labelled_type(key: &str, children: &serde_json::Map<String, Value>, forms: &HostForms) -> bool {
    let path = vec![key.to_string()];
    !forms.labelled.contains_key(&path)
        && !forms.blocks.contains(&path)
        && !forms.attributes.contains(&path)
        && forms.labelled.keys().any(|labelled| labelled.len() == 1)
        && !children.is_empty()
        && children.values().all(Value::is_object)
}

/// Each top-level block type of `map` the host does not write that holds
/// what a labelled type of the host (gone from `map`) held: the type
/// renamed, written with that type's labels.
pub(super) fn renamed_labelled_types(forms: &mut HostForms, previous: &str, map: &serde_json::Map<String, Value>) {
    let gone: Vec<(String, usize)> = forms
        .labelled
        .iter()
        .filter(|(path, _)| path.len() == 1 && !map.contains_key(&path[0]))
        .map(|(path, depth)| (path[0].clone(), *depth))
        .collect();
    if gone.is_empty() {
        return;
    }
    let Ok(old) = hcl_parse(previous) else {
        return;
    };
    for (key, value) in map {
        let path = vec![key.clone()];
        if forms.labelled.contains_key(&path) {
            continue;
        }
        if let Some((_, depth)) = gone.iter().find(|(type_name, _)| old.get(type_name) == Some(value)) {
            forms.labelled.insert(path, *depth);
        }
    }
}

/// The blocks `identifier "label" … { body }` for a map written with `depth`
/// label levels: each entry at each level a block of its own
/// (`service "http" "web"` and `… "api"`); an entry whose value is
/// not a map of maps ends the labels there.
fn labelled_entries(
    identifier: &hcl::Identifier,
    map: &serde_json::Map<String, Value>,
    depth: usize,
    path: &[String],
    forms: &HostForms,
) -> Result<Vec<hcl::Block>, String> {
    let mut blocks = Vec::new();
    let mut labels = Vec::new();
    let mut body_path = path.to_vec();
    for (label, value) in map {
        labels.push(hcl::BlockLabel::String(label.clone()));
        body_path.push(label.clone());
        labelled_level(identifier, value, depth, (&mut labels, &mut body_path), forms, &mut blocks)?;
        labels.pop();
        body_path.pop();
    }
    Ok(blocks)
}

/// The blocks under `labels` (at `body_path`) whose value is `value`.
fn labelled_level(
    identifier: &hcl::Identifier,
    value: &Value,
    depth: usize,
    (labels, body_path): (&mut Vec<hcl::BlockLabel>, &mut Vec<String>),
    forms: &HostForms,
    blocks: &mut Vec<hcl::Block>,
) -> Result<(), String> {
    // Blocks written more than once with these labels (`provider "aws"`
    // twice): one block each, as `hcl-rs` reads them back to the list.
    if let Value::Array(items) = value
        && !labels.is_empty()
        && !items.is_empty()
        && items.iter().all(Value::is_object)
    {
        for item in items {
            let Value::Object(inner) = item else { continue };
            blocks.push(hcl::Block {
                identifier: identifier.clone(),
                labels: labels.clone(),
                body: hcl_body(inner, forms, body_path)?,
            });
        }
        return Ok(());
    }
    let Value::Object(inner) = value else {
        return Err(format!(
            "a labelled block holds a map of its attributes and blocks, or a list of them for blocks written more than once (at {})",
            pointer_of(body_path)
        ));
    };
    let deeper = labels.len() < depth && !inner.is_empty() && inner.values().all(Value::is_object);
    if !deeper {
        blocks.push(hcl::Block {
            identifier: identifier.clone(),
            labels: labels.clone(),
            body: hcl_body(inner, forms, body_path)?,
        });
        return Ok(());
    }
    for (label, child) in inner {
        labels.push(hcl::BlockLabel::String(label.clone()));
        body_path.push(label.clone());
        labelled_level(identifier, child, depth, (labels, body_path), forms, blocks)?;
        labels.pop();
        body_path.pop();
    }
    Ok(())
}

/// Whether `written` (a value as the host writes it) is a quoted string
/// holding a template sequence (an interpolation or directive not escaped).
pub(super) fn is_template(written: &str) -> bool {
    written.starts_with('"') && super::heredoc::has_sequence(written)
}

/// The text each value `source` writes as an expression or template reads
/// as, at any depth (in lists and objects too: `{ Env = var.env }`).
fn written_expressions(source: &str) -> std::collections::HashSet<String> {
    let pointers = super::expressions::expression_pointers(source);
    if pointers.is_empty() {
        return Default::default();
    }
    let Ok(value) = hcl_parse(source) else {
        return Default::default();
    };
    pointers.iter().filter_map(|pointer| value.pointer(pointer)?.as_str().map(str::to_string)).collect()
}

/// The expression a `${…}` string stands for (`${var.azs}` is `var.azs`).
pub(super) fn expression_of(text: &str) -> Option<hcl::Expression> {
    super::expressions::expression_text(text)?.parse::<hcl::Expression>().ok()
}

/// [`hcl_expression`], writing the strings the host writes as templates or
/// bare expressions that way again.
pub(super) fn hcl_expression_in(value: &Value, forms: &HostForms) -> hcl::Expression {
    let template = |text: &str| {
        hcl::Expression::TemplateExpr(Box::new(hcl::TemplateExpr::QuotedString(
            super::super::super::codec::non_finite::written(text).to_string(),
        )))
    };
    match value {
        Value::String(text) if forms.templates.contains(text) => template(text),
        Value::String(text) if forms.expressions.contains(text) => expression_of(text)
            .or_else(|| forms.heredocs.get(text).map(|heredoc| hcl::Expression::from(hcl::TemplateExpr::Heredoc(heredoc.clone()))))
            .unwrap_or_else(|| hcl_expression(value)),
        Value::Array(items) if !forms.templates.is_empty() || !forms.expressions.is_empty() => {
            hcl::Expression::Array(items.iter().map(|item| hcl_expression_in(item, forms)).collect())
        }
        Value::Object(map) if !forms.templates.is_empty() || !forms.expressions.is_empty() => {
            object_expression(map, |child| hcl_expression_in(child, forms))
        }
        _ => hcl_expression(value),
    }
}

/// A document value as an HCL expression; identifier keys stay unquoted.
pub(super) fn hcl_expression(value: &Value) -> hcl::Expression {
    use hcl::expr::Expression;
    match value {
        Value::Null => Expression::Null,
        Value::Bool(flag) => Expression::Bool(*flag),
        Value::Number(number) => hcl::to_expression(number).unwrap_or(Expression::Null),
        // Text holding a template marker is escaped here (see
        // `heredoc::quoted`).
        Value::String(text) => {
            let text = super::super::super::codec::non_finite::written(text);
            if text.contains("${") || text.contains("%{") {
                super::heredoc::string_expression(text)
            } else {
                Expression::String(text.to_string())
            }
        }
        Value::Array(items) => Expression::Array(items.iter().map(hcl_expression).collect()),
        Value::Object(map) => object_expression(map, hcl_expression),
    }
}

/// `map` as an object expression, each value written by `entry` (see
/// `super::splice` for one of many entries).
fn object_expression(map: &serde_json::Map<String, Value>, entry: impl Fn(&Value) -> hcl::Expression) -> hcl::Expression {
    use hcl::expr::{Expression, Object, ObjectKey};
    super::splice::object(map, object_key, &entry).unwrap_or_else(|| {
        Expression::Object(map.iter().map(|(key, value)| (object_key(key), entry(value))).collect::<Object<ObjectKey, Expression>>())
    })
}

/// An object's key: an identifier unquoted, anything else a string.
fn object_key(key: &str) -> hcl::expr::ObjectKey {
    match hcl::Identifier::new(key) {
        Ok(identifier) if bare_object_key(key) => hcl::expr::ObjectKey::Identifier(identifier),
        _ => hcl::expr::ObjectKey::Expression(hcl::Expression::String(key.to_string())),
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

/// The bytes of the repeated unlabelled block at `pointer` (`/s/1`, the
/// second `s { … }`), from its type to its `}`.
pub(super) fn repeated_block_lines(source: &str, pointer: &str) -> Option<std::ops::Range<usize>> {
    let wanted = pointer_segments(pointer);
    crate::editor::language::pointer_index(wanted.last()?)?;
    let scan = analyze(source, 0);
    let header = scan
        .headers
        .iter()
        .find(|header| header.labels.is_empty() && body_path(&scan, header.child) == wanted)?;
    let start = header.type_span.0;
    let mut depth = 0usize;
    for token in tokenize(&source[start..]) {
        match token.kind {
            Kind::Punct('{') => depth += 1,
            Kind::Punct('}') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(start..start + token.end);
                }
            }
            _ => {}
        }
    }
    None
}

/// Where `source` writes its block types and labels by path (each place's
/// span, and whether it is a label, which may be a string), and the paths
/// of its block types and attributes written in a body (identifiers).
struct KeyPlaces {
    renames: std::collections::HashMap<Vec<String>, Vec<Place>>,
    identifiers: std::collections::HashSet<Vec<String>>,
}

/// A key's place: its span, and whether it is a label.
type Place = ((usize, usize), bool);

/// The places of `source`'s keys: scanned once per batch over it.
fn key_places(source: &str) -> std::rc::Rc<KeyPlaces> {
    crate::editor::paths::batch_cached_as(crate::editor::work_count::Work::KeyScan, source, || {
        let scan = analyze(source, 0);
        let mut renames: std::collections::HashMap<Vec<String>, Vec<Place>> = std::collections::HashMap::new();
        let mut identifiers = std::collections::HashSet::new();
        for header in &scan.headers {
            let mut path = body_path(&scan, header.parent);
            path.push(header.block_type.clone());
            renames.entry(path.clone()).or_default().push((header.type_span, false));
            identifiers.insert(path.clone());
            for (label, span) in header.labels.iter().zip(&header.label_spans) {
                path.push(label.clone());
                renames.entry(path.clone()).or_default().push((*span, true));
            }
        }
        identifiers.extend(scan.attributes.iter().filter(|attribute| attribute.prefix.is_empty()).map(|attribute| attribute_path(&scan, attribute)));
        KeyPlaces { renames, identifiers }
    })
}

/// The edits renaming the block type or label at `pointer` `new_key` in
/// every block written with it.
pub(super) fn block_key_renames(source: &str, pointer: &str, new_key: &str) -> Option<Vec<crate::editor::language::KeyEdit>> {
    let wanted = pointer_segments(pointer);
    let places = key_places(source);
    let mut edits: Vec<Place> = places.renames.get(&wanted).cloned().unwrap_or_default();
    // A string (`"host name"`) only as a label.
    let string = new_key.starts_with('"');
    if edits.is_empty() || string && edits.iter().any(|(_, label)| !label) {
        return None;
    }
    edits.sort();
    edits.dedup();
    Some(
        edits
            .into_iter()
            .map(|((start, end), _)| {
                // A quoted label keeps its quotes around a bare new key.
                let quoted = source[start..end].starts_with('"') && !string;
                let (start, end) = if quoted { (start + 1, end - 1) } else { (start, end) };
                (start..end, new_key.to_string())
            })
            .collect(),
    )
}

/// Whether `pointer` names a block type or an attribute written in a body
/// of `source` (which HCL writes only as identifiers).
pub(super) fn names_block_or_attribute(source: &str, pointer: &str) -> bool {
    key_places(source).identifiers.contains(&pointer_segments(pointer))
}
