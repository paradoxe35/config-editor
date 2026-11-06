//! Structural edits a save can make in place, keeping the host's style and
//! comments where a line patch would move things: a renamed key keeps its
//! line (`udp.buffer = 3 # udp` → `udp_renamed.buffer = 3 # udp`), and a
//! scalar that became a container is written as an inline value where the
//! scalar was (`listen = 1 # port` → `listen = { inner = 1 } # port`).
//! Each is kept only if it reads back as the new document.

use super::super::language::{ConfigLanguage, Span};
use super::{byte_range, escape, read};
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
    // Each rename on the text so far: a renamed key's siblings and
    // children keep their pointers (only equal values are renamed).
    let mut patched = previous.to_string();
    for (pointer, old_key, new_key) in renames {
        if !plain_key(&new_key) {
            return None;
        }
        patched = match language.rename_key(&patched, &pointer, &new_key) {
            Some(renamed) => renamed,
            None => rename_at_key_span(language, &patched, &pointer, &old_key, &new_key)?,
        };
    }
    (read(language, &patched, schema).ok().as_ref() == Some(new)).then_some(patched)
}

/// `source` with the key at `pointer` written `new_key`, found by the
/// language's key span; quotes around it stay.
fn rename_at_key_span(
    language: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    old_key: &str,
    new_key: &str,
) -> Option<String> {
    let range = byte_range(source, language.key_span(source, pointer)?)?;
    let written = &source[range.clone()];
    let inner = match written.chars().next()? {
        quote @ ('"' | '\'') if written.len() >= 2 && written.ends_with(quote) => {
            range.start + 1..range.end - 1
        }
        _ => range,
    };
    if source[inner.clone()] != *old_key {
        return None;
    }
    let mut renamed = source.to_string();
    renamed.replace_range(inner, new_key);
    Some(renamed)
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

/// A key every language writes bare.
fn plain_key(key: &str) -> bool {
    let mut characters = key.chars();
    characters
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
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
    let range = byte_range(previous, span)?;
    let mut patched = previous.to_string();
    patched.replace_range(range, &text);
    (read(language, &patched, schema).ok().as_ref() == Some(new)).then_some(patched)
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
