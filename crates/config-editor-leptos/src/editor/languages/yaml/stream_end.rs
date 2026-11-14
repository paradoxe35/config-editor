//! A block scalar (`|`, `>`) whose last content line ends the text with no
//! line break after it holds no final line break: YAML's chomping keeps
//! (`clip`, `keep`) only a line break that is written, and the end of the
//! stream is none (spec 8.1.1.2, `b-chomped-last`). The parser reads one
//! there all the same; it is taken off the value read.

use serde_json::Value;

use super::editing::{block_scalar_bodies, code_before_comment};

/// `value` (read from `source`) without the line break the parser gave a
/// block scalar ending the text on a content line with no break after it.
pub(super) fn without_unwritten_break(source: &str, mut value: Value) -> Value {
    if source.ends_with('\n') {
        return value;
    }
    let last_line = source.rsplit('\n').next().unwrap_or_default();
    if last_line.trim().is_empty() || last_line.trim_start().starts_with('#') {
        return value;
    }
    let lines = source.split('\n').count();
    let ends_a_block = block_scalar_bodies(source)
        .last()
        .is_some_and(|body| body.end == lines);
    if !ends_a_block && !(value.is_string() && root_block(source)) {
        return value;
    }
    if let Some(last) = last_string(&mut value)
        && last.ends_with('\n')
    {
        last.pop();
    }
    value
}

/// Whether the document `source` is a block scalar itself (`|` or `>`
/// opening its first line of code, after a `---` and properties).
fn root_block(source: &str) -> bool {
    source
        .lines()
        .map(|line| code_before_comment(line).trim())
        .find(|code| !code.is_empty() && *code != "---")
        .and_then(|code| {
            code.split_whitespace()
                .find(|token| *token != "---" && !token.starts_with(['&', '!']))
        })
        .is_some_and(|token| token.starts_with(['|', '>']))
}

/// The string written last in `value` (its last entry's, down to a leaf).
fn last_string(value: &mut Value) -> Option<&mut String> {
    match value {
        Value::String(text) => Some(text),
        Value::Array(items) => items.last_mut().and_then(last_string),
        Value::Object(map) => map.values_mut().next_back().and_then(last_string),
        _ => None,
    }
}

#[cfg(test)]
#[path = "stream_end_tests.rs"]
mod tests;
