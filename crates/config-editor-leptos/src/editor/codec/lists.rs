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
