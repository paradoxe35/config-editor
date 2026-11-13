//! An edit inside one line that changes one scalar value and nothing else
//! (see [`ConfigLanguage::edits_one_scalar`]), told by reading that line on
//! its own, before and after: both read as one value under the same keys.
//!
//! Read alone, a line says what it holds where it is too, as long as the
//! edit adds and removes nothing that means something over lines: a
//! language names those characters (quotes and escapes, which a string over
//! lines would end or continue on; the brackets and commas of a container
//! written over lines) and the closers of its comments and sections over
//! lines, whose count on the line must stay the same. Inside a multi-line
//! string or comment, such an edit changes that text and nothing else.

use std::ops::Range;

use serde_json::Value;

use super::super::language::ConfigLanguage;

/// How one language reads a line on its own.
pub(super) struct LineRules {
    /// The line as a text of its own (see [`Alone`]); `None` for a line
    /// that cannot be read alone.
    pub(super) alone: fn(&str) -> Option<Alone>,
    /// Characters the edit may neither add nor remove.
    pub(super) forbidden: &'static [char],
    /// Texts whose count on the line the edit may not change (`*/`, `-->`).
    pub(super) closers: &'static [&'static str],
}

/// A line made a text of its own.
pub(super) struct Alone {
    /// How many of the line's bytes are cut off its start (an indent,
    /// YAML's `- `).
    pub(super) cut: usize,
    pub(super) text: String,
    /// Whether the line is a list item, which may hold a scalar under no
    /// key; any other line holds one under a key.
    pub(super) item: bool,
}

/// The bytes of the line of `source` holding `range` when replacing
/// `range` with `inserted` changes one scalar there (by `rules`), else
/// `None`.
pub(super) fn one_scalar_line(
    language: &(impl ConfigLanguage + ?Sized),
    source: &str,
    range: Range<usize>,
    inserted: &str,
    rules: &LineRules,
) -> Option<Range<usize>> {
    let removed = source.get(range.clone())?;
    let touches = |text: &str| {
        text.chars().any(|character| {
            matches!(character, '\n' | '\r') || rules.forbidden.contains(&character)
        })
    };
    if touches(removed) || touches(inserted) {
        return None;
    }
    let start = source[..range.start].rfind('\n').map_or(0, |at| at + 1);
    let end = source[range.end..]
        .find('\n')
        .map_or(source.len(), |at| range.end + at);
    let old = &source[start..end];
    let new = format!(
        "{}{inserted}{}",
        &old[..range.start - start],
        &old[range.end - start..]
    );
    let count = |line: &str, closer: &str| line.matches(closer).count();
    if rules
        .closers
        .iter()
        .any(|closer| count(old, closer) != count(&new, closer))
    {
        return None;
    }
    let (old_alone, new_alone) = ((rules.alone)(old)?, (rules.alone)(&new)?);
    if old_alone.cut != new_alone.cut
        || old_alone.item != new_alone.item
        || range.start - start < old_alone.cut
    {
        return None;
    }
    let read = |text: &str| language.parse(text).ok();
    let (old_value, new_value) = (read(&old_alone.text)?, read(&new_alone.text)?);
    let (old_keys, new_keys) = (leaf_keys(&old_value)?, leaf_keys(&new_value)?);
    let held = !old_keys.is_empty() || old_alone.item;
    (held && old_keys == new_keys).then_some(start..end)
}

/// The keys leading to the one scalar `document` holds, when it holds only
/// that (each object on the way has one entry).
fn leaf_keys(document: &Value) -> Option<Vec<&str>> {
    let mut keys = Vec::new();
    let mut at = document;
    loop {
        match at {
            Value::Object(map) if map.len() == 1 => {
                let (key, value) = map.iter().next()?;
                keys.push(key.as_str());
                at = value;
            }
            Value::Object(_) | Value::Array(_) => return None,
            _ => return Some(keys),
        }
    }
}

/// The line without its indent, which is cut off.
#[cfg_attr(
    not(any(feature = "toml", feature = "hcl", feature = "xml")),
    allow(dead_code, reason = "only TOML, HCL and XML read a line so")
)]
pub(super) fn dedented(line: &str) -> Option<Alone> {
    let text = line.trim_start();
    Some(Alone {
        cut: line.len() - text.len(),
        text: text.to_string(),
        item: false,
    })
}
