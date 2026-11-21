//! What every random protect run holds to, and the model telling an
//! ordinary edit from one a protection must refuse.
//!
//! A document's values are flattened to `(path, leaf)` pairs, list
//! indices left out (an item added above a protected item moves it, and
//! it is still the same item; XML writes a list of one as an element).
//! The protected pairs: a read-only path's value at that very path (a
//! path names a place); the schema's `readOnly` on a list's items, every
//! item; else those at the field's path (a list item's: those holding
//! its value). An edit after which the text parses and they are as they
//! were, and every other pair is kept, is ordinary. On screen, a text that
//! reads a protected value otherwise is safe only while the editor holds
//! the change (the save waits with its reason) and nothing of it reached
//! the host.

use serde_json::Value;

use super::cases::{Lock, NEW_SECRET, TOKEN};
use super::mount::Setup;
use crate::editor::Language;

/// The kinds of failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(in crate::editor) enum Check {
    /// A protected value changed (or went) in a text the host heard or
    /// saved, or on screen.
    LockChanged,
    /// A placeholder with other text beside it in its value.
    Glued,
    /// A secret's new value quoted in a problem.
    Leak,
    /// An ordinary edit refused.
    OverRefusal,
    /// An ordinary edit taken, but the text is not what it should be.
    Differs,
    Panic,
}

/// `value` flattened (see the module).
pub(super) fn flatten(value: &Value) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    walk(value, String::new(), &mut pairs);
    pairs.sort();
    pairs
}

fn walk(value: &Value, path: String, pairs: &mut Vec<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                walk(value, format!("{path}/{key}"), pairs);
            }
        }
        Value::Array(items) => {
            for item in items {
                walk(item, path.clone(), pairs);
            }
        }
        Value::String(text) => pairs.push((path, text.clone())),
        other => pairs.push((path, other.to_string())),
    }
}

/// The protected field's path, list indices left out.
fn unindexed(pointer: &str) -> String {
    pointer
        .split('/')
        .filter(|key| key.is_empty() || !key.chars().all(|c| c.is_ascii_digit()))
        .collect::<Vec<_>>()
        .join("/")
}

/// The protected pairs of `parsed` (flattened `pairs`; see the module).
fn protected(
    setup: &Setup,
    value: &str,
    parsed: &Value,
    pairs: &[(String, String)],
) -> Vec<(String, String)> {
    let pointer = setup.case().pointer;
    let path = unindexed(pointer);
    let in_list = path != pointer;
    if setup.lock == Lock::Path {
        // XML reads a list of one as the element itself.
        let at = parsed
            .pointer(pointer)
            .or_else(|| parsed.pointer(pointer.strip_suffix("/0")?));
        return at
            .map(|at| {
                flatten(at)
                    .into_iter()
                    .map(|(_, leaf)| (pointer.to_string(), leaf))
                    .collect()
            })
            .unwrap_or_default();
    }
    pairs
        .iter()
        .filter(|(at, leaf)| {
            *at == path && (!in_list || setup.lock == Lock::Schema || leaf == value)
        })
        .cloned()
        .collect()
}

/// Whether `pairs` keeps every pair of `before` (as many times).
fn keeps(before: &[(String, String)], after: &[(String, String)]) -> bool {
    let mut left = after.to_vec();
    before
        .iter()
        .all(|pair| match left.iter().position(|kept| kept == pair) {
            Some(at) => {
                left.remove(at);
                true
            }
            None => false,
        })
}

/// Whether editing `before` to `after` (in `language`) is ordinary: both
/// parse, the protected pairs are as they were and, unless `removes`
/// (a line cut), every other pair is kept. Where a host range is, the
/// caller judges (`acts::writes_in_range`): of alike lines, only the
/// editor knows which one the range followed.
pub(super) fn ordinary(
    setup: &Setup,
    language: Language,
    value: &str,
    before: &str,
    after: &str,
    removes: bool,
) -> bool {
    let (Ok(old), Ok(new)) = (language.parse(before), language.parse(after)) else {
        return false;
    };
    let (old_pairs, new_pairs) = (flatten(&old), flatten(&new));
    let before_protected = protected(setup, value, &old, &old_pairs);
    if before_protected != protected(setup, value, &new, &new_pairs) || before_protected.is_empty()
    {
        return false;
    }
    // A field outside a list stays at its very path (a second `db` in
    // XML makes a list of both, and moves it).
    let pointer = setup.case().pointer;
    if unindexed(pointer) == pointer && old.pointer(pointer) != new.pointer(pointer) {
        return false;
    }
    if !removes && !keeps(&old_pairs, &new_pairs) {
        return false;
    }
    true
}

/// Checks `text` (in `language`): the protected value is unchanged
/// (`original`: the protected pairs as mounted; a secret may take a new
/// value), and no value holds a placeholder with other text.
pub(super) fn safe(
    setup: &Setup,
    language: Language,
    value: &str,
    original: &[(String, String)],
    text: &str,
) -> Result<(), (Check, String)> {
    let Ok(parsed) = language.parse(text) else {
        return Ok(());
    };
    let pairs = flatten(&parsed);
    if let Some((at, leaf)) = pairs
        .iter()
        .find(|(_, leaf)| leaf.contains(TOKEN) && leaf != TOKEN)
    {
        return Err((Check::Glued, format!("{at} = {leaf:?} in {text:?}")));
    }
    let now = protected(setup, value, &parsed, &pairs);
    // A list's items may be copied or added beside it: each protected one
    // stays. A host range protects its text: a copy of it elsewhere is new
    // text, free while the reading it protects stays.
    let pointer = setup.case().pointer;
    let strict =
        setup.lock == Lock::Path || (unindexed(pointer) == pointer && setup.lock != Lock::Range);
    let kept = if strict {
        now == original
    } else {
        keeps(original, &now)
    };
    if setup.lock != Lock::Secret && !kept {
        return Err((
            Check::LockChanged,
            format!("{original:?} became {now:?} in {text:?}"),
        ));
    }
    Ok(())
}

/// The protected pairs of `text` as mounted.
pub(super) fn original(
    setup: &Setup,
    language: Language,
    value: &str,
    text: &str,
) -> Vec<(String, String)> {
    language
        .parse(text)
        .map(|parsed| protected(setup, value, &parsed, &flatten(&parsed)))
        .unwrap_or_default()
}

/// Whether `text` (in `language`) may hold the secret's new value as a
/// secret's: a secret is a value at a field the schema marks, so it is one
/// while the text does not parse (a syntax message may quote any of it),
/// while a value holding it stands at the field's path or at a field
/// holding it (a broken holder read as one string), or while a key holds
/// it. A value whose key was renamed to one no schema marks
/// (`passwo1rd`) is no secret's, as the schema fuzz's secrets judge it.
pub(super) fn holds_new_secret(setup: &Setup, language: Language, text: &str) -> bool {
    if !text.contains(NEW_SECRET) {
        return false;
    }
    let Ok(parsed) = language.parse(text) else {
        return true;
    };
    let field = unindexed(setup.case().pointer);
    let holds = |path: &str| field == path || field.starts_with(&format!("{path}/"));
    in_a_key(&parsed)
        || flatten(&parsed)
            .iter()
            .any(|(path, leaf)| leaf.contains(NEW_SECRET) && holds(path))
}

/// Whether a key of `value` holds the secret's new value.
fn in_a_key(value: &Value) -> bool {
    match value {
        Value::Object(map) => map
            .iter()
            .any(|(key, child)| key.contains(NEW_SECRET) || in_a_key(child)),
        Value::Array(items) => items.iter().any(in_a_key),
        _ => false,
    }
}

/// Whether a problem message quotes the secret's new value.
pub(super) fn leaks(messages: &[String]) -> Option<String> {
    messages
        .iter()
        .find(|message| message.contains(NEW_SECRET))
        .cloned()
}
