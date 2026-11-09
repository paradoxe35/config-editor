//! Structural edits a save can make in place, keeping the host's style and
//! comments where a line patch would move things: a renamed key keeps its
//! line (`udp.buffer = 3 # udp` → `udp_renamed.buffer = 3 # udp`), and a
//! scalar that became a container is written as an inline value where the
//! scalar was (`listen = 1 # port` → `listen = { inner = 1 } # port`).
//! Each is kept only if it reads back as the new document.

use super::super::language::{ConfigLanguage, Span};
use super::{byte_range, escape, reads_as, through_long_quote};
use serde_json::Value;

/// `previous` with renamed keys renamed where they are written, when `new`
/// is `old` with only keys renamed (same values, same positions).
pub(in super::super) fn patch_renames(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    let mut renames = Vec::new();
    renamed_keys(old, new, String::new(), &mut renames)?;
    if renames.is_empty() {
        return None;
    }
    let patched = renamed(language, previous, &renames)?;
    reads_as(language, &patched, schema, new).then_some(patched)
}

/// `previous` with every rename made where the language writes the key
/// (its [`key_renames`](ConfigLanguage::key_renames), else the key's span),
/// all placed from one read of `previous` (one at a time, each read the
/// text so far again): renamed keys hold no other renamed key, so each is
/// where `previous` writes it. `None` when one cannot be made, or two
/// would write over each other.
fn renamed(
    language: &dyn ConfigLanguage,
    previous: &str,
    renames: &[(String, String, String)],
) -> Option<String> {
    let pointers: Vec<String> = renames
        .iter()
        .map(|(pointer, ..)| pointer.clone())
        .collect();
    let mut edits = super::super::paths::with_spans(language, previous, &pointers, || {
        let mut edits = Vec::with_capacity(renames.len());
        for (pointer, old_key, new_key) in renames {
            let written = language.written_key(previous, pointer, new_key)?;
            match language.key_renames(previous, pointer, &written) {
                Some(own) => edits.extend(own),
                None => edits.push(key_span_edit(
                    language, previous, pointer, old_key, &written,
                )?),
            }
        }
        Some(edits)
    })?;
    edits.sort_by_key(|(range, _)| (range.start, range.end));
    edits.dedup();
    if edits.windows(2).any(|pair| pair[0].0.end > pair[1].0.start) {
        return None;
    }
    Some(super::super::language::with_edits(previous, edits))
}

/// The bytes of `source` a rename at the key's span replaces (quotes
/// around it stay), and their new text.
fn key_span_edit(
    language: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    old_key: &str,
    new_key: &str,
) -> Option<(std::ops::Range<usize>, String)> {
    let span = super::super::paths::memo_key_span(language, source, pointer)?;
    let range = byte_range(source, span)?;
    let written = &source[range.clone()];
    let flow = super::super::flow_syntax::Flow::of(language);
    let quote = written.chars().next().filter(|quote| {
        flow.quote(*quote).is_some()
            && written.len() >= 2 * quote.len_utf8()
            && written.ends_with(*quote)
    });
    let inner = match quote {
        Some(quote) => range.start + quote.len_utf8()..range.end - quote.len_utf8(),
        None => range.clone(),
    };
    if source[inner.clone()] != *old_key {
        return None;
    }
    let text = match quote {
        // A bare new key in the host's quotes.
        Some(quote) if !flow.starts_quoted(new_key) => format!("{quote}{new_key}{quote}"),
        _ => new_key.to_string(),
    };
    Some((range, text))
}

/// `new` with each key it renames from `old` (in the same place, holding
/// the same value) spelled as `old` spells it, whatever else differs: what
/// a save compares where the place of a value matters and a rename does
/// not move it (an alias inside a renamed key still reads its anchor).
pub(in super::super) fn with_renames_undone(old: &Value, new: &Value) -> Value {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) if left.len() == right.len() => Value::Object(
            left.iter()
                .zip(right)
                .map(|((old_key, old_value), (new_key, new_value))| {
                    let renamed = old_key != new_key
                        && old_value == new_value
                        && !left.contains_key(new_key)
                        && !right.contains_key(old_key);
                    if renamed {
                        (old_key.clone(), new_value.clone())
                    } else {
                        (
                            new_key.clone(),
                            with_renames_undone(
                                old.get(new_key).unwrap_or(&Value::Null),
                                new_value,
                            ),
                        )
                    }
                })
                .collect(),
        ),
        (Value::Object(left), Value::Object(right)) => Value::Object(
            right
                .iter()
                .map(|(key, value)| match left.get(key) {
                    Some(was) => (key.clone(), with_renames_undone(was, value)),
                    None => (key.clone(), value.clone()),
                })
                .collect(),
        ),
        (Value::Array(left), Value::Array(right)) => Value::Array(
            right
                .iter()
                .enumerate()
                .map(|(index, item)| match left.get(index) {
                    Some(was) => with_renames_undone(was, item),
                    None => item.clone(),
                })
                .collect(),
        ),
        _ => new.clone(),
    }
}

/// Collects `(old pointer, old key, new key)` for each key `new` renames;
/// `None` when anything else differs.
fn renamed_keys(
    old: &Value,
    new: &Value,
    pointer: String,
    out: &mut Vec<(String, String, String)>,
) -> Option<()> {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) if left.len() == right.len() => {
            for ((old_key, old_value), (new_key, new_value)) in left.iter().zip(right) {
                let child = format!("{pointer}/{}", escape(old_key));
                if old_key != new_key {
                    if old_value != new_value
                        || left.contains_key(new_key)
                        || right.contains_key(old_key)
                    {
                        return None;
                    }
                    out.push((child, old_key.clone(), new_key.clone()));
                } else {
                    renamed_keys(old_value, new_value, child, out)?;
                }
            }
            Some(())
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (old_item, new_item)) in left.iter().zip(right).enumerate() {
                renamed_keys(old_item, new_item, format!("{pointer}/{index}"), out)?;
            }
            Some(())
        }
        _ => (old == new).then_some(()),
    }
}

/// `previous` with a scalar that became a container written as an inline
/// value in its place, when that is the only change.
pub(in super::super) fn patch_scalar_to_container(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    let pointer = only_change(old, new, String::new())?;
    let value = new.pointer(&pointer)?;
    let was_scalar = old
        .pointer(&pointer)
        .is_some_and(|before| !before.is_object() && !before.is_array());
    if !was_scalar || !(value.is_object() || value.is_array()) {
        return None;
    }
    let text = language.format_value(value);
    if text.contains('\n') {
        return None;
    }
    let span: Span = language.value_span(previous, &pointer)?;
    let mut range = byte_range(previous, span)?;
    range.end = through_long_quote(language, previous, range.clone());
    let mut patched = previous.to_string();
    patched.replace_range(range, &text);
    reads_as(language, &patched, schema, new).then_some(patched)
}

/// The pointer of the one value `new` changes, keys and lengths kept.
fn only_change(old: &Value, new: &Value, pointer: String) -> Option<String> {
    if old == new {
        return None;
    }
    match (old, new) {
        (Value::Object(left), Value::Object(right))
            if left.len() == right.len() && left.keys().all(|key| right.contains_key(key)) =>
        {
            let mut differing = left.keys().filter(|key| left[*key] != right[*key]);
            let key = differing.next()?;
            if differing.next().is_some() {
                return None;
            }
            only_change(
                &left[key],
                &right[key],
                format!("{pointer}/{}", escape(key)),
            )
        }
        _ => Some(pointer),
    }
}
