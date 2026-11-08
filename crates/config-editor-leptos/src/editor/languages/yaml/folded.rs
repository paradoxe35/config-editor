//! Quoted and plain scalars written over lines (`"l1\n  l2"`), edited in
//! place: the line the edit falls on changes, the line breaks stay.

use super::*;

/// How a scalar over lines is quoted.
#[derive(Clone, Copy, PartialEq)]
enum Style {
    Double,
    Single,
    Plain,
}

/// One line's share of the value: where it is written in the scalar, and
/// whether its line break is escaped (`\` at its end, double quotes only).
struct Piece {
    range: std::ops::Range<usize>,
    escaped: bool,
}

/// `written` (a quoted or plain scalar folded over lines, as the host wrote
/// it) made to read as `text` by rewriting only the line the edit falls on,
/// when the edit stays inside one line's share of the value: its other lines,
/// their indentation and its breaks stay. `None` otherwise (the caller writes
/// the value whole).
pub(super) fn kept_lines(language: &YamlBehavior, written: &str, text: &str) -> Option<String> {
    let style = match written.chars().next()? {
        '"' => Style::Double,
        '\'' => Style::Single,
        '|' | '>' | '[' | '{' | '&' | '*' | '!' => return None,
        _ => Style::Plain,
    };
    let quoted = style != Style::Plain;
    if quoted && (written.len() < 2 || !written.ends_with(written.chars().next()?)) {
        return None;
    }
    let reads = |candidate: &str| match language.parse(&format!("k: {candidate}\n")).ok()? {
        Value::Object(map) => map.get("k").and_then(Value::as_str).map(str::to_string),
        _ => None,
    };
    let old = reads(written)?;
    let pieces = pieces(written, style)?;
    // Each piece's value and the text that joins it to the next (a space for
    // a folded break, a line feed per blank line, nothing after an escaped
    // break).
    let mut values: Vec<String> = Vec::new();
    let mut joins: Vec<String> = Vec::new();
    let mut blank = 0;
    let last = pieces.len() - 1;
    for (index, piece) in pieces.iter().enumerate() {
        let content = &written[piece.range.clone()];
        if content.is_empty() {
            if index == 0 || index == last {
                return None;
            }
            blank += 1;
            continue;
        }
        if let Some(previous) = pieces[..index].iter().rev().find(|piece| !piece.range.is_empty()) {
            joins.push(match (previous.escaped, blank) {
                (true, 0) => String::new(),
                (true, _) => return None,
                (false, 0) => " ".to_string(),
                (false, count) => "\n".repeat(count),
            });
        }
        blank = 0;
        values.push(match style {
            Style::Double => reads(&format!("\"{content}\""))?,
            Style::Single => content.replace("''", "'"),
            Style::Plain => content.to_string(),
        });
    }
    let kept: Vec<&Piece> = pieces.iter().filter(|piece| !piece.range.is_empty()).collect();
    let mut joined = values[0].clone();
    let mut starts = vec![0];
    for (value, join) in values[1..].iter().zip(&joins) {
        joined.push_str(join);
        starts.push(joined.len());
        joined.push_str(value);
    }
    if joined != old {
        return None;
    }
    // What changed: the text between the common start and the common end.
    let prefix = common_prefix(&old, text);
    let suffix = common_prefix_rev(&old[prefix..], &text[prefix..]);
    let changed_end = old.len() - suffix;
    let index = (0..values.len())
        .find(|index| starts[*index] <= prefix && changed_end <= starts[*index] + values[*index].len())?;
    let start = starts[index];
    let end = (start + values[index].len() + text.len()).checked_sub(old.len())?;
    let value = text.get(start..end)?;
    if value.contains(['\n', '\r']) {
        return None;
    }
    let content = match style {
        Style::Double => {
            let quoted = super::super::escapes::like_written(
                yaml_quoted(value),
                written,
                super::super::escapes::Astral::Long,
            );
            quoted.strip_prefix('"')?.strip_suffix('"')?.to_string()
        }
        Style::Single => value.replace('\'', "''"),
        Style::Plain => value.to_string(),
    };
    let range = kept[index].range.clone();
    let rewritten = format!("{}{content}{}", &written[..range.start], &written[range.end..]);
    (reads(&rewritten).as_deref() == Some(text)).then_some(rewritten)
}

/// Each line's share of `written`: after the opening quote and before the
/// closing one, without the indentation and the spaces a break folds.
fn pieces(written: &str, style: Style) -> Option<Vec<Piece>> {
    let quoted = style != Style::Plain;
    let mut pieces = Vec::new();
    let mut offset = 0;
    let lines: Vec<&str> = written.split('\n').collect();
    if lines.len() < 2 {
        return None;
    }
    let last = lines.len() - 1;
    for (index, line) in lines.iter().enumerate() {
        let mut start = offset;
        let mut end = offset + line.len();
        if index == 0 && quoted {
            start += 1;
        }
        if index == last && quoted {
            end -= 1;
        }
        let body = &written[start..end];
        let backslashes = body.len() - body.trim_end_matches('\\').len();
        let escaped = style == Style::Double && index < last && backslashes % 2 == 1;
        // The break folds the spaces around it; inside the quotes the first
        // line's leading and the last line's trailing ones are text.
        if escaped {
            end -= 1;
        } else if index < last || !quoted {
            end = start + body.trim_end_matches([' ', '\t']).len();
        }
        if index > 0 || !quoted {
            start = end - written[start..end].trim_start_matches([' ', '\t']).len();
        }
        pieces.push(Piece {
            range: start..end.max(start),
            escaped,
        });
        offset += line.len() + 1;
    }
    Some(pieces)
}

/// The byte length of the longest common start of `a` and `b`, on a
/// character boundary.
fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices()
        .zip(b.chars())
        .find(|((_, left), right)| left != right)
        .map_or(a.len().min(b.len()), |((at, _), _)| at)
        .min(b.len())
}

/// The byte length of the longest common end of `a` and `b`, on a
/// character boundary.
fn common_prefix_rev(a: &str, b: &str) -> usize {
    a.chars()
        .rev()
        .zip(b.chars().rev())
        .take_while(|(left, right)| left == right)
        .map(|(left, _)| left.len_utf8())
        .sum()
}
