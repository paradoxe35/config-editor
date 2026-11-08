//! Which key the pointer is over.

use super::*;

/// The mapping key under `column` (a character index) on a single line: the
/// attribute name before `=`. Block header words yield `None`: hover context
/// reads values, and headers open no value. Comment-only lines yield `None`.
pub(super) fn key_at_column(line: &str, column: usize) -> Option<(String, usize)> {
    let code = code_before_comment(line)?;
    let byte = byte_at_column(line, column);
    if let Some(key) = inline_key_at(code, b'=', byte) {
        let raw = line[key.start..key.end].trim_matches('"');
        return (!raw.is_empty()).then(|| (raw.to_string(), line[..key.start].chars().count()));
    }
    // A block header (`letsencrypt {`, `services "web" {`): its type word or
    // a label.
    let header = code.trim_end().strip_suffix('{')?;
    let mut start = 0;
    for word in header.split_whitespace() {
        let at = start + header[start..].find(word)?;
        start = at + word.len();
        if (at..start).contains(&byte) {
            let name = word.trim_matches('"');
            return (!name.is_empty() && !name.contains('='))
                .then(|| (name.to_string(), line[..at + usize::from(word.starts_with('"'))].chars().count()));
        }
    }
    None
}

/// The path named by the block header word under `column` (a character
/// index) of line `line_index`: its type (`services` in `services "web" {`)
/// names a key of the enclosing body, each label the object below; the last
/// word of a repeated block (`b` of a second `b {`) names its item
/// (`/b/1`), as the document lists it.
pub(super) fn header_path(source: &str, line_index: usize, column: usize) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
    let line = source.split('\n').nth(line_index)?;
    let line_start: usize = source.split('\n').take(line_index).map(|line| line.len() + 1).sum();
    let indent = line.len() - line.trim_start().len();
    if line.trim().is_empty() {
        return None;
    }
    let scan = analyze(source, line_start);
    let header = scan.headers.iter().find(|header| header.type_span.0 == line_start + indent)?;
    let char_column = |byte: usize| line[..byte.saturating_sub(line_start).min(line.len())].chars().count();
    let names = std::iter::once((&header.block_type, header.type_span)).chain(header.labels.iter().zip(header.label_spans.iter().copied()));
    let (index, (name, (start, _))) = names
        .enumerate()
        .find(|(_, (_, (start, end)))| column >= char_column(*start) && column < char_column(*end))?;
    let mut path: Vec<SchemaPathSegment> = body_path(&scan, header.parent)
        .into_iter()
        .map(|segment| match segment.parse::<usize>() {
            Ok(index) => SchemaPathSegment::ArrayItem(index),
            Err(_) => SchemaPathSegment::Key(segment),
        })
        .collect();
    path.push(SchemaPathSegment::Key(header.block_type.clone()));
    path.extend(header.labels[..index].iter().cloned().map(SchemaPathSegment::Key));
    if index == header.labels.len() && scan.repeats(header) {
        path.push(SchemaPathSegment::ArrayItem(header.ordinal));
    }
    // A quoted label starts inside its quote.
    let quoted = usize::from(line[start - line_start..].starts_with('"'));
    Some((path, name.clone(), char_column(start) + quoted))
}

/// The byte offset of character `column` (0-based) in `line`.
pub(super) fn byte_at_column(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index)
}
