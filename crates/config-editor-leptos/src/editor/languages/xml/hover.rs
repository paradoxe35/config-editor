//! Which key the pointer is over.

use super::*;

/// The element or attribute name under `column` (a character index) on a
/// single line, with its starting column. Comment-only lines yield `None`.
pub(super) fn hover_name(line: &str, column: usize) -> Option<(String, usize)> {
    let tokens = tokenize(line);
    for token in &tokens {
        match token.kind {
            Kind::OpenTag { name_end, .. } => {
                let start = token.start + 1;
                if start >= name_end {
                    continue;
                }
                let start_column = line[..start].chars().count();
                let end_column = line[..name_end.min(line.len())].chars().count();
                if (start_column..end_column).contains(&column) {
                    return Some((decode_entities(&line[start..name_end]), start_column));
                }
                for inner in tokenize_tag_inside(line, token) {
                    if !matches!(inner.kind, Kind::Text { quoted: false }) {
                        continue;
                    }
                    let start_column = line[..inner.start].chars().count();
                    let end_column = line[..inner.end].chars().count();
                    if (start_column..end_column).contains(&column) {
                        // Markup is no key (`declarations`). With the line
                        // alone, `xsi` is taken as the schema-instance
                        // prefix it is bound to as a rule.
                        let name = &line[inner.start..inner.end];
                        let markup = super::declarations::Markup::of(line).holds(name) || name.starts_with("xsi:");
                        return (!markup).then(|| (name.to_string(), start_column));
                    }
                }
            }
            Kind::EndTag => {
                let name = token_name(line, token);
                if name.is_empty() {
                    continue;
                }
                let start = token.start + 2;
                let start_column = line[..start].chars().count();
                let end_column = start_column
                    + line[start..]
                        .chars()
                        .take_while(|character| !matches!(character, '>' | '/' | ' ' | '\t'))
                        .count();
                if (start_column..end_column).contains(&column) {
                    return Some((name, start_column));
                }
            }
            Kind::Comment => return None,
            _ => {}
        }
    }
    None
}

/// Byte offset within `line` where the hover context is read: just past the
/// first `>` after the name at `column`, so one line holding a whole subtree
/// resolves the hover under the right path.
pub(super) fn hover_context_end(line: &str, column: usize) -> usize {
    let byte = line
        .char_indices()
        .nth(column)
        .map_or(line.len(), |(index, _)| index);
    let name_start = hover_name(line, column).map_or(byte, |(_, start_column)| {
        line.char_indices()
            .nth(start_column)
            .map_or(line.len(), |(index, _)| index)
    });
    let name_end = line[name_start..]
        .find(|character: char| matches!(character, '>' | '/' | '=') || character.is_whitespace())
        .map_or(line.len(), |relative| name_start + relative);
    // An attribute (`name="…"`) resolves inside its quotes, where its value
    // is addressed.
    let rest = &line[name_end..];
    let after_equals = rest.trim_start().strip_prefix('=').map(str::trim_start);
    match after_equals {
        Some(value) if value.starts_with(['"', '\'']) => line.len() - value.len() + 1,
        // An element whose start tag carries attributes: read just past its
        // name, where the context is the element itself.
        _ if rest.starts_with([' ', '\t']) && !rest.trim_start().starts_with(['>', '/']) => name_end + 1,
        _ => name_end,
    }
}

/// The document path of the element whose start-tag (or end-tag) name is
/// at `column` (characters, from 0) of line `line_index`, its name and
/// start column there. An end tag is its element's, as its start tag.
pub(super) fn element_hover_path(source: &str, line_index: usize, column: usize) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
    let line = crate::editor::paths::split_line(source, line_index)?;
    let line_start = crate::editor::paths::line_start(source, line_index);
    let byte = line_start + line.char_indices().nth(column).map_or(line.len(), |(at, _)| at);
    let walked = super::scan::walked(source);
    let read;
    let (tokens, plan): (&[Token], &PathPlan) = match &walked {
        Some(walked) => (&walked.tokens, &walked.plan),
        None => {
            let tokens = tokenize(source);
            let plan = plan_paths(source, &tokens);
            read = (tokens, plan);
            (&read.0, &read.1)
        }
    };
    if let Some(found) = attribute_hover_path(source, tokens, plan, byte, walked.is_some()) {
        return Some(found);
    }
    let hovered = |index: usize, token: &Token| match token.kind {
        Kind::OpenTag { name_end, .. } if token.start < byte && byte < name_end => Some((index, token.start + 1)),
        Kind::EndTag => {
            let start = token.start + 2;
            let name_end = start + token_name(source, token).len();
            (start <= byte && byte < name_end).then_some((index, start))
        }
        _ => None,
    };
    let (hovered, name_start) = match &walked {
        // Tokens do not overlap: only the last one starting before the
        // place can hold it, found by halving.
        Some(_) => {
            let index = tokens.partition_point(|token| token.start < byte).checked_sub(1)?;
            hovered(index, &tokens[index])?
        }
        None => tokens.iter().enumerate().find_map(|(index, token)| hovered(index, token))?,
    };
    let token = match (tokens[hovered].kind, &walked) {
        (Kind::EndTag, Some(walked)) => walked.opening_of(source, hovered)?,
        (Kind::EndTag, None) => opening_of(source, tokens, hovered)?,
        _ => &tokens[hovered],
    };
    let path = plan.path_of(token.start)?;
    if path.is_empty() {
        return None;
    }
    let segments = path
        .iter()
        .map(|segment| match segment {
            Seg::Key(key) => SchemaPathSegment::Key(key.clone()),
            Seg::Index(index) => SchemaPathSegment::ArrayItem(*index),
        })
        .collect();
    let name = decode_entities(&token_name(source, token));
    let start_column = source[line_start..name_start].chars().count();
    Some((segments, name, start_column))
}

/// [`element_hover_path`] on an attribute's name: the attribute's path, its
/// key `@name` where a child element has the name too.
fn attribute_hover_path(
    source: &str,
    tokens: &[Token],
    plan: &PathPlan,
    byte: usize,
    halving: bool,
) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
    let in_attributes = |token: &&Token| matches!(token.kind, Kind::OpenTag { name_end, .. } if name_end <= byte && byte < token.end);
    // Tokens do not overlap: with `halving`, only the last one starting
    // before the place is looked at.
    let open = if halving {
        tokens[..tokens.partition_point(|token| token.start < byte)].last().filter(in_attributes)?
    } else {
        tokens.iter().find(in_attributes)?
    };
    let name = tokenize_tag_inside(source, open).into_iter().find(|inner| {
        matches!(inner.kind, Kind::Text { quoted: false })
            && inner.start <= byte
            && byte < inner.end
            && is_attr_name(source, open, *inner)
    })?;
    // Markup is no key: no schema describes it.
    if plan.markup.holds(&source[name.start..name.end]) {
        return None;
    }
    let written = &source[name.start..name.end];
    let mut segments: Vec<SchemaPathSegment> = plan
        .path_of(open.start)?
        .iter()
        .map(|segment| match segment {
            Seg::Key(key) => SchemaPathSegment::Key(key.clone()),
            Seg::Index(index) => SchemaPathSegment::ArrayItem(*index),
        })
        .collect();
    segments.push(SchemaPathSegment::Key(plan.attribute_key(open.start, written)));
    let line_start = source[..name.start].rfind('\n').map_or(0, |at| at + 1);
    Some((segments, written.to_string(), source[line_start..name.start].chars().count()))
}

/// The start tag the end tag `tokens[end]` closes, matched as the path
/// plan matches them (see `plan_text`): the innermost open element of its
/// name.
fn opening_of<'a>(source: &str, tokens: &'a [Token], end: usize) -> Option<&'a Token> {
    let mut open: Vec<(&Token, String)> = Vec::new();
    for token in &tokens[..end] {
        match token.kind {
            Kind::OpenTag { .. } => {
                let name = token_name(source, token);
                if !name.is_empty() && !opens_nothing(source, token) {
                    open.push((token, name));
                }
            }
            Kind::EndTag => {
                let name = token_name(source, token);
                match open.iter().rposition(|(_, open)| *open == name) {
                    Some(position) => open.truncate(position),
                    None => {
                        open.pop();
                    }
                }
            }
            _ => {}
        }
    }
    let name = token_name(source, &tokens[end]);
    open.iter().rev().find(|(_, open)| *open == name).map(|(token, _)| *token)
}
