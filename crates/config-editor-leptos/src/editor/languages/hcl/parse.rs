//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;

/// References, interpolations, and function calls arrive as their
/// `${ ... }`-style source text.
pub(super) fn hcl_parse(source: &str) -> Result<Value, ParseError> {
    let value = hcl::from_str::<Value>(source).map_err(|error| {
        // Only syntax errors carry a location; others fall back to the top.
        let (line, column) = match &error {
            hcl::Error::Parse(parse) => {
                let location = parse.location();
                (location.line(), location.column())
            }
            _ => (1, 1),
        };
        let line_count = source.split('\n').count().max(1);
        ParseError {
            message: readable_error(&error.to_string()),
            line: Some(line.clamp(1, line_count)),
            column: Some(column.max(1)),
        }
    })?;
    match shadowed_name(source) {
        Some((offset, message)) => {
            let (line, column) = line_column(source, offset);
            Err(ParseError { message, line: Some(line), column: Some(column) })
        }
        None => Ok(value),
    }
}

/// A name one body writes in two forms the parser cannot both keep (an
/// attribute and a block, or blocks with and without labels): it keeps one
/// and drops the other silently, so the second one is reported instead.
pub(super) fn shadowed_name(source: &str) -> Option<(usize, String)> {
    let scan = analyze(source, 0);
    // Each name a body writes: (body, name) -> (first form, its offset).
    let mut seen: HashMap<(usize, String), (&'static str, usize)> = HashMap::new();
    let mut found: Option<(usize, String)> = None;
    let mut forms: Vec<(usize, usize, String, &'static str)> = scan
        .attributes
        .iter()
        .filter(|attribute| attribute.prefix.is_empty())
        .map(|attribute| (attribute.key_span.0, attribute.body, attribute.name.clone(), "an attribute"))
        .collect();
    forms.extend(scan.headers.iter().map(|header| {
        let form = if header.labels.is_empty() { "a block without labels" } else { "a block with labels" };
        (header.type_span.0, header.parent, header.block_type.clone(), form)
    }));
    forms.sort_by_key(|(offset, ..)| *offset);
    for (offset, body, name, form) in forms {
        match seen.get(&(body, name.clone())) {
            Some((first, _)) if *first != form => {
                found = Some((
                    offset,
                    format!("`{name}` is written both as {first} and as {form}; only one of them would be read. Use one form."),
                ));
                break;
            }
            Some(_) => {}
            None => {
                seen.insert((body, name), (form, offset));
            }
        }
    }
    found
}

/// The parser's sentence without its ASCII-art source frame (`--> …`, `|`,
/// `^---`): the position is reported separately.
pub(super) fn readable_error(message: &str) -> String {
    let notes: Vec<&str> = message
        .lines()
        .filter_map(|line| line.trim().strip_prefix("= "))
        .collect();
    if notes.is_empty() {
        message.lines().next().unwrap_or(message).trim().to_string()
    } else {
        notes.join("; ")
    }
}

pub(super) fn span_between(source: &str, start: usize, end: usize) -> (usize, usize, usize) {
    let (line, column) = line_column(source, start);
    (
        line,
        column,
        column + source[start..end.min(source.len())].chars().count(),
    )
}

pub(super) fn hcl_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    let wanted = pointer_segments(path);
    if wanted.is_empty() {
        return None;
    }
    let scan = analyze(source, 0);
    if let Some(attribute) = scan
        .attributes
        .iter()
        .find(|attribute| attribute_path(&scan, attribute) == wanted)
    {
        return Some(span_between(
            source,
            attribute.key_span.0,
            attribute.key_span.1,
        ));
    }
    // A block type or label address (`/resource`, `/resource/aws_x`): point at
    // the header word or label token.
    if let Some((start, end)) = header_span(&scan, &wanted) {
        return Some(span_between(source, start, end));
    }
    inline_key(source, &wanted).map(|(start, end, _)| span_between(source, start, end))
}

/// A key nested in inline containers (`dns = [{ wait_secs = 60 }]`), which
/// the block scan does not index: `(key start, key end, value start)`.
pub(super) fn inline_key(source: &str, wanted: &[String]) -> Option<(usize, usize, usize)> {
    let segment = |segment: &SchemaPathSegment| match segment {
        SchemaPathSegment::Key(key) => key.clone(),
        SchemaPathSegment::ArrayItem(index) => index.to_string(),
    };
    for (start, end) in statements(source) {
        for (path, key) in inline_key_paths(&source[start..end], b'=') {
            let tail: Vec<String> = path.iter().map(segment).collect();
            if !wanted.ends_with(&tail) {
                continue;
            }
            // The path above the statement, from the block scan at its start.
            let outer = analyze(source, start);
            let mut full = body_path(&outer, outer.snapshot.body);
            if outer.snapshot.in_map {
                full.extend(outer.snapshot.value_prefix.iter().cloned());
            }
            full.extend(tail);
            if full == wanted {
                return Some((start + key.start, start + key.end, start + key.after_separator));
            }
        }
    }
    None
}

pub(super) fn hcl_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    hcl_value_spans(source, &[path.to_string()]).pop().flatten()
}

/// [`hcl_value_span`] for each of `paths`, reading the document once.
pub(super) fn hcl_value_spans(source: &str, paths: &[String]) -> Vec<Option<(usize, usize, usize)>> {
    let scan = analyze(source, 0);
    let attributes: Vec<(Vec<String>, (usize, usize))> = scan
        .attributes
        .iter()
        .map(|attribute| (attribute_path(&scan, attribute), attribute.value_span))
        .collect();
    paths
        .iter()
        .map(|path| span_in_scan(source, &attributes, &pointer_segments(path)))
        .collect()
}

fn span_in_scan(
    source: &str,
    attributes: &[(Vec<String>, (usize, usize))],
    wanted: &[String],
) -> Option<(usize, usize, usize)> {
    let Some((_, value_span)) = attributes
        .iter()
        .find(|(path, _)| path == wanted)
    else {
        let (_, _, value_start) = inline_key(source, wanted)?;
        let start = value_start + (source[value_start..].len() - source[value_start..].trim_start().len());
        if source[start..].starts_with(['{', '[']) {
            let (line, column) = line_column(source, start);
            return Some((line, column, column + 1));
        }
        let end = start + inline_value_len(&source[start..]);
        return (end > start).then(|| span_between(source, start, end));
    };
    let (start, end) = *value_span;
    if end <= start {
        return None;
    }
    if source[start..].starts_with(['{', '[']) {
        // A container is marked at its opening bracket, not its whole body.
        let (line, column) = line_column(source, start);
        return Some((line, column, column + 1));
    }
    Some(span_between(source, start, end))
}

/// Span of a block header word: the type identifier when the path names the
/// block type, or the label token when it names a label.
pub(super) fn header_span(scan: &Scan, wanted: &[String]) -> Option<(usize, usize)> {
    for header in &scan.headers {
        let mut base = body_path(scan, header.parent);
        base.push(header.block_type.clone());
        if base == wanted {
            return Some(header.type_span);
        }
        for (level, span) in header.label_spans.iter().enumerate() {
            let mut prefix = base.clone();
            prefix.extend(header.labels[..=level].iter().cloned());
            if prefix == wanted {
                return Some(*span);
            }
        }
    }
    None
}

pub(super) fn first_line_span(source: &str) -> Option<(usize, usize, usize)> {
    source.lines().enumerate().find_map(|(index, line)| {
        let start = line.find(|character: char| !character.is_whitespace())?;
        Some((
            index + 1,
            line[..start].chars().count() + 1,
            line.chars().count() + 1,
        ))
    })
}
