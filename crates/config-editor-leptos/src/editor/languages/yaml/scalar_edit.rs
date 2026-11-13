//! An edit changing one YAML scalar and nothing else (see
//! `ConfigLanguage::edits_one_scalar`): a `key: value` line (or a `- `
//! item) read alone, as `scalar_lines` does, and two things only the text
//! around it tells. The line below may not be indented past the line's
//! key (it would continue a plain scalar, or hold its entries); and neither
//! the line nor a node holding it may carry an anchor (`&name`), since an
//! alias reads the value elsewhere too (`<<: *defaults`).

use std::ops::Range;

use super::super::scalar_lines::{Alone, LineRules, one_scalar_line};
use super::YamlBehavior;

/// What a YAML scalar edit may neither add nor remove: quotes and escapes
/// (a string over lines), a comment, anchors, aliases, tags and block
/// scalar headers, a flow collection's brackets and commas, and tabs.
const RULES: LineRules = LineRules {
    alone,
    forbidden: &[
        '"', '\'', '\\', '#', '&', '*', '!', '|', '>', ',', '[', ']', '{', '}', '\t',
    ],
    closers: &[],
};

/// Whether replacing `range` of `source` with `inserted` changes one scalar.
pub(super) fn edits_one_scalar(
    behavior: &YamlBehavior,
    source: &str,
    range: Range<usize>,
    inserted: &str,
) -> bool {
    let Some(line) = one_scalar_line(behavior, source, range, inserted, &RULES) else {
        return false;
    };
    let text = &source[line.clone()];
    let Some(Alone { cut: column, .. }) = alone(text) else {
        return false;
    };
    if !block_written(&text[column..]) {
        return false;
    }
    let below = source[line.end..]
        .split('\n')
        .skip(1)
        .find(|next| holds_code(next));
    if below.is_some_and(|next| indent(next) > column) {
        return false;
    }
    !anchored(&source[..line.start], text)
}

/// The line without its indent and the `- ` of the items it opens.
fn alone(line: &str) -> Option<Alone> {
    let mut cut = indent(line);
    let mut item = false;
    while let Some(rest) = line[cut..].strip_prefix("- ") {
        item = true;
        cut = line.len() - rest.trim_start_matches(' ').len();
    }
    Some(Alone {
        cut,
        text: line[cut..].to_string(),
        item,
    })
}

/// Whether `line` or a line holding it in `above` (the text before it)
/// carries an anchor: by indent, each line less indented than the last one
/// found, or as indented and opening it as an item.
fn anchored(above: &str, line: &str) -> bool {
    if line.contains('&') {
        return true;
    }
    let mut depth = indent(line);
    let mut item = line[depth..].starts_with("- ");
    for parent in above.lines().rev().filter(|parent| holds_code(parent)) {
        let at = indent(parent);
        let opens = parent[at..].starts_with("- ");
        if at < depth || (at == depth && item && !opens) {
            if parent.contains('&') {
                return true;
            }
            (depth, item) = (at, opens);
            if depth == 0 && !item {
                return false;
            }
        }
    }
    false
}

/// Whether `entry` (a line past its indent and dashes) is written in block
/// form: no quote, bracket or comma but a quoted value's own. A line ending
/// a string or a flow collection written over lines (`  b: c"`,
/// `  b: two}`) reads alone otherwise than where it is.
fn block_written(entry: &str) -> bool {
    let value = value_start(entry).map_or(entry, |start| &entry[start..]);
    let code = if value.starts_with(['"', '\'']) {
        &entry[..entry.len() - value.len()]
    } else {
        entry
    };
    !code.contains(['"', '\'', '[', ']', '{', '}', ','])
}

/// Where the value of `entry` starts: past the first `: ` (or a `:` ending
/// the line) and the blanks after it; `None` for a lone value (an item).
fn value_start(entry: &str) -> Option<usize> {
    let colon = entry
        .match_indices(':')
        .map(|(at, _)| at)
        .find(|at| matches!(entry.as_bytes().get(at + 1), None | Some(b' ')))?;
    let rest = &entry[colon + 1..];
    Some(entry.len() - rest.trim_start_matches(' ').len())
}

/// Whether `line` holds more than blanks and a comment.
fn holds_code(line: &str) -> bool {
    let code = line.trim_start();
    !code.is_empty() && !code.starts_with('#')
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}
