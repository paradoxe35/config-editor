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
    if lines.len() < 2
        || first.trim() != "[" && !first.trim_end().ends_with('[')
        || last.trim() != "]"
    {
        return None;
    }
    // Each item line: the comment lines right above it (they go where it
    // goes), its code without the comma, its trailing comment.
    struct Line {
        above: Vec<String>,
        code: String,
        comment: String,
        used: bool,
    }
    let mut old: Vec<Line> = Vec::new();
    let mut above: Vec<String> = Vec::new();
    let mut last_code_line = None;
    for line in &lines[1..lines.len() - 1] {
        if line.trim().is_empty() {
            continue;
        }
        let code_end = comment_start(language, line);
        let code = line[..code_end].trim();
        // A comment of its own: it belongs to the item below it.
        if code.is_empty() {
            above.push(line.to_string());
            continue;
        }
        // Several items on one line: not this shape.
        if code.trim_end_matches(',').contains(',') {
            return None;
        }
        last_code_line = Some(*line);
        old.push(Line {
            above: std::mem::take(&mut above),
            code: code.trim_end_matches(',').to_string(),
            comment: line[code_end..].trim().to_string(),
            used: false,
        });
    }
    // Comments after the last item stay at the end.
    let closing_comments = above;
    // One line per item, each the item it was: not a list of objects or of
    // items spread over lines.
    let one_per_line = old.len() == before.len()
        && old.iter().zip(before).all(|(line, item)| {
            !item.is_object()
                && !item.is_array()
                && comparable(language, &line.code)
                    == comparable(language, &language.format_value(item))
        });
    if !one_per_line {
        return None;
    }
    let indent: String = lines
        .iter()
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|character| matches!(character, ' ' | '\t'))
                .collect()
        })
        .unwrap_or_default();
    // A comma after the last item too, when the host writes one there.
    let trailing_comma = last_code_line.is_some_and(|line| {
        line[..comment_start(language, line)]
            .trim_end()
            .ends_with(',')
    });
    let mut out = vec![first.to_string()];
    for (index, item) in items.iter().enumerate() {
        let written = language.format_value(item);
        let reused = old.iter_mut().find(|line| {
            !line.used && comparable(language, &line.code) == comparable(language, &written)
        });
        let (code, comment) = match reused {
            Some(line) => {
                line.used = true;
                out.extend(line.above.iter().cloned());
                (line.code.clone(), line.comment.clone())
            }
            None => (written, String::new()),
        };
        let comma = if index + 1 < items.len() || trailing_comma {
            ","
        } else {
            ""
        };
        let comment = if comment.is_empty() {
            String::new()
        } else {
            format!(" {comment}")
        };
        out.push(format!("{indent}{code}{comma}{comment}"));
    }
    out.extend(closing_comments);
    out.push(last.to_string());
    Some(out.join("\n"))
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
    if lines.len() < 2 || !first.trim_end().ends_with('{') || last.trim() != "}" {
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
                let bare = !key.is_empty()
                    && key.chars().all(|character| {
                        character.is_alphanumeric() || matches!(character, '_' | '-')
                    });
                let name = if bare && !model.head.starts_with(['"', '\'']) {
                    key.clone()
                } else {
                    serde_json::to_string(key).ok()?
                };
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
