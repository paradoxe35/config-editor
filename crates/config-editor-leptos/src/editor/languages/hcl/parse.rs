//! Parsing to a JSON value, and the source spans of keys and values.

use super::*;

/// References, interpolations, and function calls arrive as their
/// `${ ... }`-style source text; a heredoc without them reads as its text,
/// as a quoted string does (`$${x}` is `${x}`).
pub(super) fn hcl_parse(source: &str) -> Result<Value, ParseError> {
    // A leading byte order mark is no part of the document (as YAML, TOML
    // and XML read it); columns on its line count it.
    let bom = usize::from(source.starts_with('\u{FEFF}'));
    let text = source.strip_prefix('\u{FEFF}').unwrap_or(source);
    let read = || -> Result<Value, hcl::Error> {
        hcl::from_body(super::heredoc::parse_body(text)?).map(super::floats::read_whole_floats)
    };
    let value = read().map_err(|error| {
        // Only syntax errors carry a location; others fall back to the top.
        let (line, column) = match &error {
            hcl::Error::Parse(parse) => {
                let location = parse.location();
                let bom = if location.line() == 1 { bom } else { 0 };
                (location.line(), location.column() + bom)
            }
            _ => (1, 1),
        };
        let line_count = source.split('\n').count().max(1);
        let (line, column) = (line.clamp(1, line_count), column.max(1));
        ParseError {
            message: super::super::wide_integers::out_of_range(source, line, column).unwrap_or_else(|| readable_error(&error.to_string())),
            line: Some(line),
            column: Some(column),
        }
    })?;
    if let Some((offset, message)) = shadowed_name(source) {
        let (line, column) = line_column(source, offset);
        return Err(ParseError { message, line: Some(line), column: Some(column) });
    }
    let written = |pointer: &str| written_at(source, hcl_value_span(source, pointer)?);
    if let Some((pointer, text)) = super::super::wide_integers::first_past_64_bits(&value, &written) {
        let span = hcl_value_span(source, &pointer);
        return Err(ParseError {
            message: super::super::wide_integers::message(&text),
            line: span.map(|span| span.0),
            column: span.map(|span| span.1),
        });
    }
    Ok(value)
}

/// The text a span (line, first and end character columns) covers.
pub(super) fn written_at(source: &str, (line, start, end): (usize, usize, usize)) -> Option<String> {
    let text = source.lines().nth(line.checked_sub(1)?)?;
    Some(text.chars().skip(start.saturating_sub(1)).take(end.saturating_sub(start)).collect())
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

/// The block scan of a whole document by path: each attribute (the first
/// at a path) and the header word or label naming each block path.
struct Indexed {
    scan: std::rc::Rc<Scan>,
    attributes: HashMap<Vec<String>, usize>,
    headers: HashMap<Vec<String>, (usize, usize)>,
    /// The path of each block written once (its type, and its labels).
    lone_blocks: HashSet<Vec<String>>,
}

/// [`Indexed`] of `source`, read once for all the spans a batch of
/// problems asks for (see `paths::batch_cached`).
fn indexed(source: &str) -> std::rc::Rc<Indexed> {
    crate::editor::paths::batch_cached(source, || {
        let scan = analyze(source, 0);
        let mut attributes = HashMap::with_capacity(scan.attributes.len());
        for (at, attribute) in scan.attributes.iter().enumerate() {
            attributes.entry(attribute_path(&scan, attribute)).or_insert(at);
        }
        let mut headers = HashMap::new();
        let mut lone_blocks = HashSet::new();
        for header in &scan.headers {
            let mut base = body_path(&scan, header.parent);
            base.push(header.block_type.clone());
            if !scan.repeats(header) {
                let mut block = base.clone();
                block.extend(header.labels.iter().cloned());
                lone_blocks.insert(block);
            }
            for (level, span) in header.label_spans.iter().enumerate() {
                let mut prefix = base.clone();
                prefix.extend(header.labels[..=level].iter().cloned());
                headers.entry(prefix).or_insert(*span);
            }
            // A repeated block is an item of a list (`/b/1`): its header
            // names it, at its last label or its type.
            if scan.repeats(header) {
                let mut item = base.clone();
                item.extend(header.labels.iter().cloned());
                item.push(header.ordinal.to_string());
                headers.entry(item).or_insert(*header.label_spans.last().unwrap_or(&header.type_span));
            }
            headers.entry(base).or_insert(header.type_span);
        }
        Indexed { scan, attributes, headers, lone_blocks }
    })
}

/// Whether `path` names a block written once (see
/// `ConfigLanguage::lone_item`).
pub(super) fn hcl_lone_block(source: &str, path: &str) -> bool {
    indexed(source).lone_blocks.contains(&pointer_segments(path))
}

pub(super) fn hcl_key_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    hcl_key_spans(source, &[path.to_string()]).pop().flatten()
}

/// [`hcl_key_span`] for each of `paths`, reading the document once.
pub(super) fn hcl_key_spans(source: &str, paths: &[String]) -> Vec<Option<(usize, usize, usize)>> {
    let index = indexed(source);
    let wanted: Vec<Vec<String>> = paths.iter().map(|path| pointer_segments(path)).collect();
    let mut found: Vec<Option<(usize, usize)>> = wanted
        .iter()
        .map(|wanted| {
            if wanted.is_empty() {
                return None;
            }
            if let Some(at) = index.attributes.get(wanted) {
                return Some(index.scan.attributes[*at].key_span);
            }
            // A block type or label address (`/resource`, `/resource/aws_x`):
            // point at the header word or label token.
            index.headers.get(wanted).copied()
        })
        .collect();
    let missing: Vec<usize> = (0..wanted.len())
        .filter(|at| found[*at].is_none() && !wanted[*at].is_empty())
        .collect();
    for (at, key) in missing.iter().zip(inline_keys(source, &missing.iter().map(|at| wanted[*at].clone()).collect::<Vec<_>>())) {
        found[*at] = key.map(|(start, end, _)| (start, end));
    }
    found
        .into_iter()
        .map(|span| span.map(|(start, end)| span_between(source, start, end)))
        .collect()
}

/// Keys nested in inline containers (`dns = [{ wait_secs = 60 }]`), which
/// the block scan does not index, for each of `wanted`, reading the
/// statements once: `(key start, key end, value start)`.
fn inline_keys(source: &str, wanted: &[Vec<String>]) -> Vec<Option<(usize, usize, usize)>> {
    let mut found = vec![None; wanted.len()];
    if wanted.is_empty() {
        return found;
    }
    let mut by_path: HashMap<&[String], Vec<usize>> = HashMap::new();
    let mut tails: HashSet<&[String]> = HashSet::new();
    for (at, path) in wanted.iter().enumerate() {
        by_path.entry(path.as_slice()).or_default().push(at);
        for from in 0..=path.len() {
            tails.insert(&path[from..]);
        }
    }
    let mut missing = wanted.len();
    let segment = |segment: &SchemaPathSegment| match segment {
        SchemaPathSegment::Key(key) => key.clone(),
        SchemaPathSegment::ArrayItem(index) => index.to_string(),
    };
    for (start, end) in statements(source) {
        if missing == 0 {
            break;
        }
        // The path above the statement, from the block scan at its start
        // (read only for a statement holding a key some path ends with).
        let mut outer: Option<Vec<String>> = None;
        for (path, key) in inline_key_paths(&source[start..end], b'=') {
            let tail: Vec<String> = path.iter().map(segment).collect();
            if !tails.contains(tail.as_slice()) {
                continue;
            }
            let above = outer.get_or_insert_with(|| {
                let outer = analyze(source, start);
                let mut full = body_path(&outer, outer.snapshot.body);
                if outer.snapshot.in_map {
                    full.extend(outer.snapshot.value_prefix.iter().cloned());
                }
                full
            });
            let mut full = above.clone();
            full.extend(tail);
            for at in by_path.get(full.as_slice()).into_iter().flatten() {
                if found[*at].is_none() {
                    found[*at] = Some((start + key.start, start + key.end, start + key.after_separator));
                    missing -= 1;
                }
            }
        }
    }
    found
}

pub(super) fn hcl_value_span(source: &str, path: &str) -> Option<(usize, usize, usize)> {
    hcl_value_spans(source, &[path.to_string()]).pop().flatten()
}

/// [`hcl_value_span`] for each of `paths`, reading the document once.
pub(super) fn hcl_value_spans(source: &str, paths: &[String]) -> Vec<Option<(usize, usize, usize)>> {
    let index = indexed(source);
    let wanted: Vec<Vec<String>> = paths.iter().map(|path| pointer_segments(path)).collect();
    let missing: Vec<Vec<String>> = wanted
        .iter()
        .filter(|wanted| !index.attributes.contains_key(*wanted))
        .cloned()
        .collect();
    let mut inline = inline_keys(source, &missing).into_iter();
    wanted
        .iter()
        .map(|wanted| {
            let attribute = index.attributes.get(wanted).map(|at| index.scan.attributes[*at].value_span);
            let inline = if attribute.is_none() { inline.next().flatten() } else { None };
            span_in_scan(source, attribute, inline)
        })
        .collect()
}

/// The span of an attribute's value (`value_span`), else of the value after
/// an inline key (`inline`: the key's start, end, and where its value starts).
fn span_in_scan(
    source: &str,
    value_span: Option<(usize, usize)>,
    inline: Option<(usize, usize, usize)>,
) -> Option<(usize, usize, usize)> {
    let Some(value_span) = value_span else {
        let (_, _, value_start) = inline?;
        let start = value_start + (source[value_start..].len() - source[value_start..].trim_start().len());
        if source[start..].starts_with(['{', '[']) && !opens_for_expression(&source[start..]) {
            let (line, column) = line_column(source, start);
            return Some((line, column, column + 1));
        }
        let end = start + inline_value_len(&source[start..]);
        // Up to a comment after the value (`3 # three` in an object).
        let end = start + source[start..end].find([' ', '\t']).filter(|at| line_comment_start(source[start + at..end].trim_start()) == Some(0)).unwrap_or(end - start);
        return (end > start).then(|| span_between(source, start, end));
    };
    let (start, end) = value_span;
    if end <= start {
        return None;
    }
    if source[start..].starts_with(['{', '[']) && !opens_for_expression(&source[start..end]) {
        // A container is marked at its opening bracket, not its whole body
        // (a `for` expression in brackets is one value, read as its text).
        let (line, column) = line_column(source, start);
        return Some((line, column, column + 1));
    }
    Some(span_between(source, start, end))
}

/// Whether `text` starts with a `for` expression (`[for s in l : s]`,
/// `{ for k, v in m : k => v }`).
fn opens_for_expression(text: &str) -> bool {
    let inner = text[1..].trim_start();
    inner.strip_prefix("for").is_some_and(|rest| rest.starts_with(char::is_whitespace))
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
