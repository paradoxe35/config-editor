//! A list the host writes one item per line (`allow = [` … `]`), edited in
//! place: kept items keep their lines (and comments), new ones get a line
//! of their own in the same style, removed ones lose theirs.

use super::super::language::ConfigLanguage;
use super::{comment_start, comparable};
use serde_json::Value;

/// `text` (the host's `[` … `]`, over several lines) holding `items`, or
/// `None` when it is not written one item per line.
pub(super) fn rewrite_list_lines(
    language: &dyn ConfigLanguage,
    text: &str,
    before: &[Value],
    items: &[Value],
) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (first, last) = (lines.first()?, lines.last()?);
    // The opening line may end with a comment (`[  # ports`).
    let first_code = first[..comment_start(language, first)].trim_end();
    if lines.len() < 2 || !first_code.ends_with('[') || last.trim() != "]" {
        return None;
    }
    // Each item line: the comment lines right above it (they go where it
    // goes), the line itself, its code without the comma, whether a comma
    // ends it.
    struct Line<'a> {
        above: Vec<&'a str>,
        line: &'a str,
        code: &'a str,
        comma: bool,
        used: bool,
    }
    let mut old: Vec<Line<'_>> = Vec::new();
    let mut above: Vec<&str> = Vec::new();
    for line in &lines[1..lines.len() - 1] {
        if line.trim().is_empty() {
            continue;
        }
        let code = line[..comment_start(language, line)].trim();
        // A comment of its own: it belongs to the item below it.
        if code.is_empty() {
            above.push(line);
            continue;
        }
        let comma = code.ends_with(',');
        let code = code.trim_end_matches(',').trim_end();
        // Several items on one line, or one spread over lines: not this shape.
        if top_level_commas(code) > 0 || !balanced(code) {
            return None;
        }
        old.push(Line {
            above: std::mem::take(&mut above),
            line,
            code,
            comma,
            used: false,
        });
    }
    // Comments after the last item stay at the end.
    let closing_comments = above;
    // One line per item, each the item it was (a one-line object or list
    // item is trusted by its place; the read-back checks it).
    let one_per_line = old.len() == before.len()
        && old.iter().zip(before).all(|(line, item)| {
            item.is_object()
                || item.is_array()
                || comparable(language, line.code)
                    == comparable(language, &language.format_value(item))
        });
    if !one_per_line {
        return None;
    }
    let indent: String = old
        .first()
        .map(|line| {
            line.line
                .chars()
                .take_while(|character| matches!(character, ' ' | '\t'))
                .collect()
        })
        .unwrap_or_default();
    // A comma after the last item too, when the host writes one there.
    let trailing_comma = old.last().is_some_and(|line| line.comma);
    let mut out = vec![first.to_string()];
    for (index, item) in items.iter().enumerate() {
        let wants_comma = index + 1 < items.len() || trailing_comma;
        // The line of an equal item kept as written, its comment and its
        // spacing too.
        let reused = (0..old.len()).find(|at| !old[*at].used && before[*at] == *item);
        match reused {
            Some(at) => {
                let line = &mut old[at];
                line.used = true;
                out.extend(line.above.iter().map(|above| above.to_string()));
                if line.comma == wants_comma {
                    out.push(line.line.to_string());
                } else {
                    let comment_at = comment_start(language, line.line);
                    let comment = &line.line[comment_at..];
                    let code_end = line.line[..comment_at].trim_end().len();
                    let gap = &line.line[code_end..comment_at];
                    let lead = &line.line[..line.line.len() - line.line.trim_start().len()];
                    let comma = if wants_comma { "," } else { "" };
                    let gap = if comment.is_empty() {
                        ""
                    } else if gap.is_empty() {
                        " "
                    } else {
                        gap
                    };
                    out.push(format!("{lead}{}{comma}{gap}{comment}", line.code));
                }
            }
            None => {
                let written = language.format_value(item);
                if written.contains('\n') {
                    return None;
                }
                let comma = if wants_comma { "," } else { "" };
                out.push(format!("{indent}{written}{comma}"));
            }
        }
    }
    out.extend(closing_comments.iter().map(|line| line.to_string()));
    out.push(last.to_string());
    Some(out.join("\n"))
}

/// Commas outside brackets and quotes in `code`.
fn top_level_commas(code: &str) -> usize {
    let (mut depth, mut quote, mut commas) = (0i32, None, 0);
    for character in code.chars() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '{' | '[' | '(') => depth += 1,
            (None, '}' | ']' | ')') => depth -= 1,
            (None, ',') if depth == 0 => commas += 1,
            _ => {}
        }
    }
    commas
}

/// Whether `code` closes every bracket it opens.
fn balanced(code: &str) -> bool {
    let (mut depth, mut quote) = (0i32, None);
    for character in code.chars() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(character),
            (None, '{' | '[' | '(') => depth += 1,
            (None, '}' | ']' | ')') => depth -= 1,
            _ => {}
        }
    }
    depth == 0
}

/// `text` (the host's `{` … `}`, one entry per line, as a YAML flow map or
/// an HCL object over lines) holding `entries`, or `None` when it is not
/// written that way. Kept entries keep their lines (and comments), changed
/// ones their key and comment, new ones get a line like their siblings'.
pub(super) fn rewrite_map_lines(
    language: &dyn ConfigLanguage,
    text: &str,
    before: &serde_json::Map<String, Value>,
    entries: &serde_json::Map<String, Value>,
) -> Option<String> {
    let lines: Vec<&str> = text.split('\n').collect();
    let (first, last) = (lines.first()?, lines.last()?);
    let first_code = first[..comment_start(language, first)].trim_end();
    if lines.len() < 2 || !first_code.ends_with('{') || last.trim() != "}" {
        return None;
    }
    // Each entry line: the comment lines above it, the line, its key, the
    // text up to its value, its value, whether a comma ends it.
    struct Entry<'a> {
        above: Vec<&'a str>,
        line: &'a str,
        key: String,
        head: &'a str,
        value: &'a str,
        comma: bool,
    }
    let mut old: Vec<Entry<'_>> = Vec::new();
    let mut above = Vec::new();
    for line in &lines[1..lines.len() - 1] {
        if line.trim().is_empty() {
            continue;
        }
        let code = line[..comment_start(language, line)].trim();
        if code.is_empty() {
            above.push(*line);
            continue;
        }
        let comma = code.ends_with(',');
        let code = code.trim_end_matches(',').trim_end();
        let separator = code.find([':', '='])?;
        let key = code[..separator]
            .trim()
            .trim_matches(['"', '\''])
            .to_string();
        let value_start = separator
            + 1
            + (code[separator + 1..].len() - code[separator + 1..].trim_start().len());
        let value = &code[value_start..];
        // An entry spread over lines, or several on one: not this shape.
        if value.is_empty()
            || value.ends_with(['{', '['])
            || value.contains(", ") && !value.starts_with(['"', '\''])
        {
            return None;
        }
        old.push(Entry {
            above: std::mem::take(&mut above),
            line,
            key,
            head: &code[..value_start],
            value,
            comma,
        });
    }
    let closing_comments = above;
    if old.len() != before.len() || !old.iter().all(|entry| before.contains_key(&entry.key)) {
        return None;
    }
    let commas = old
        .iter()
        .take(old.len().saturating_sub(1))
        .any(|entry| entry.comma);
    let trailing = old.last().is_some_and(|entry| entry.comma);
    let indent: String = old
        .first()?
        .line
        .chars()
        .take_while(|character| matches!(character, ' ' | '\t'))
        .collect();
    let model = old.first()?;
    let mut out = vec![first.to_string()];
    for (index, (key, value)) in entries.iter().enumerate() {
        let wants_comma = commas && (index + 1 < entries.len() || trailing);
        let comma = if wants_comma { "," } else { "" };
        match old.iter().find(|entry| entry.key == *key) {
            Some(entry) => {
                out.extend(entry.above.iter().map(|line| line.to_string()));
                let unchanged = before.get(key) == Some(value);
                if unchanged && entry.comma == wants_comma {
                    out.push(entry.line.to_string());
                    continue;
                }
                let written = if unchanged {
                    entry.value.to_string()
                } else {
                    language.format_value_like(value, entry.value)
                };
                if written.contains('\n') {
                    return None;
                }
                let line = entry.line;
                let comment_at = comment_start(language, line);
                let code_end = line[..comment_at].trim_end().len();
                let gap = &line[code_end..comment_at];
                let comment = &line[comment_at..];
                let gap = if comment.is_empty() {
                    ""
                } else if gap.is_empty() {
                    " "
                } else {
                    gap
                };
                let lead = &line[..line.len() - line.trim_start().len()];
                out.push(format!(
                    "{lead}{}{written}{comma}{gap}{comment}",
                    entry.head
                ));
            }
            None => {
                let separator =
                    &model.head[model.head.trim_end_matches([' ', '\t', ':', '=']).len()..];
                let name = super::key_like(language, key, model.head)?;
                let written = if value.is_object() || value.is_array() {
                    language.format_value(value)
                } else {
                    language.format_value_like(value, model.value)
                };
                if written.contains('\n') {
                    return None;
                }
                out.push(format!("{indent}{name}{separator}{written}{comma}"));
            }
        }
    }
    out.extend(closing_comments.iter().map(|line| line.to_string()));
    out.push(last.to_string());
    Some(out.join("\n"))
}
