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
    // Each middle line: (its code without the comma, its comment).
    let mut old: Vec<(String, String, bool)> = Vec::new();
    for line in &lines[1..lines.len() - 1] {
        if line.trim().is_empty() {
            continue;
        }
        let code_end = comment_start(language, line);
        let code = line[..code_end].trim();
        // A comment-only line, or several items on one line: not this shape.
        if code.is_empty() || code.trim_end_matches(',').contains(',') {
            return None;
        }
        old.push((
            code.trim_end_matches(',').to_string(),
            line[code_end..].trim().to_string(),
            false,
        ));
    }
    // One line per item, each the item it was: not a list of objects or of
    // items spread over lines.
    let one_per_line = old.len() == before.len()
        && old.iter().zip(before).all(|((code, _, _), item)| {
            !item.is_object()
                && !item.is_array()
                && comparable(language, code) == comparable(language, &language.format_value(item))
        });
    if !one_per_line {
        return None;
    }
    let indent: String = lines
        .get(1)
        .map(|line| {
            line.chars()
                .take_while(|character| matches!(character, ' ' | '\t'))
                .collect()
        })
        .unwrap_or_default();
    // A comma after the last item too, when the host writes one there.
    let last_item = lines[lines.len() - 2];
    let trailing_comma = last_item[..comment_start(language, last_item)]
        .trim_end()
        .ends_with(',');
    let mut out = vec![first.to_string()];
    for (index, item) in items.iter().enumerate() {
        let written = language.format_value(item);
        let reused = old.iter_mut().find(|(code, _, used)| {
            !used && comparable(language, code) == comparable(language, &written)
        });
        let (code, comment) = match reused {
            Some((code, comment, used)) => {
                *used = true;
                (code.clone(), comment.clone())
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
    out.push(last.to_string());
    Some(out.join("\n"))
}
