//! The host value with the user's edits applied, from what the edit
//! language showed of it.

use serde_json::Value;

/// The host value with the user's edits applied: where the edit equals what
/// the edit language showed, the host's original value is kept; elsewhere
/// the edit wins. Keys and items added or removed follow the edit.
///
/// `unshown` are the pointers of host values the edit language could not
/// show exactly (XML has no empty or nested lists). An edit inside one of
/// them that cannot be mapped back onto the host's value is ambiguous: the
/// `Err` names where, and the save waits rather than write the view's
/// approximation over the host's data.
pub(super) fn merge_changes(
    host: &Value,
    seen: &Value,
    edited: &Value,
    unshown: &[String],
) -> Result<Value, String> {
    merge_at(host, seen, edited, "", unshown)
}

fn merge_at(
    host: &Value,
    seen: &Value,
    edited: &Value,
    at: &str,
    unshown: &[String],
) -> Result<Value, String> {
    if seen == edited {
        return Ok(host.clone());
    }
    let child = |key: &str| format!("{at}/{}", key.replace('~', "~0").replace('/', "~1"));
    // The edit replaces the host's value whole: only where the view showed
    // all of it.
    let replaced = || {
        let hidden = (host.is_array() || host.is_object())
            && unshown
                .iter()
                .any(|pointer| pointer == at || pointer.starts_with(&format!("{at}/")));
        if hidden {
            Err(if at.is_empty() {
                "/".to_string()
            } else {
                at.to_string()
            })
        } else {
            Ok(edited.clone())
        }
    };
    match (host, seen, edited) {
        (Value::Object(host), Value::Object(seen), Value::Object(edited)) => {
            // In the host's key order (the edit language may order keys its
            // own way: TOML puts plain keys before tables).
            // A renamed key: gone from the edit, and a new key holding what
            // the edit showed for it. It keeps its place and the host's own
            // value (which the edit language may not show exactly).
            let mut renamed_to: Vec<(&String, &String)> = Vec::new();
            for (key, value) in edited {
                if host.contains_key(key) {
                    continue;
                }
                let from = host.keys().find(|old| {
                    !edited.contains_key(*old)
                        && seen.get(*old) == Some(value)
                        && !renamed_to.iter().any(|(taken, _)| taken == old)
                });
                if let Some(from) = from {
                    renamed_to.push((from, key));
                }
            }
            let mut entries: Vec<(String, Value)> = Vec::new();
            for (key, host_value) in host {
                match (seen.get(key), edited.get(key)) {
                    (Some(seen_value), Some(value)) => {
                        let merged = merge_at(host_value, seen_value, value, &child(key), unshown)?;
                        entries.push((key.clone(), merged));
                    }
                    // Renamed by the user.
                    (Some(_), None)
                        if let Some((_, to)) = renamed_to.iter().find(|(from, _)| *from == key) =>
                    {
                        entries.push(((*to).clone(), host_value.clone()));
                    }
                    // Removed by the user.
                    (Some(_), None) => {}
                    // Re-added after the edit language could not show it, or
                    // never shown: the edit wins, else the host's stays.
                    (None, Some(value)) => entries.push((key.clone(), value.clone())),
                    (None, None) => entries.push((key.clone(), host_value.clone())),
                }
            }
            // A new key goes after the key before it in the edit, so a renamed
            // key keeps its place.
            let mut previous: Option<&String> = None;
            for (key, value) in edited {
                let renamed = renamed_to.iter().any(|(_, to)| *to == key);
                if !host.contains_key(key) && !renamed {
                    let at = previous
                        .and_then(|before| entries.iter().position(|(name, _)| name == before))
                        .map_or(0, |index| index + 1);
                    entries.insert(at, (key.clone(), value.clone()));
                }
                previous = Some(key);
            }
            Ok(Value::Object(entries.into_iter().collect()))
        }
        (Value::Array(host), Value::Array(seen), Value::Array(edited))
            if host.len() == seen.len() && seen.len() == edited.len() =>
        {
            let mut items = Vec::with_capacity(host.len());
            for (index, ((host, seen), edited)) in host.iter().zip(seen).zip(edited).enumerate() {
                items.push(merge_at(
                    host,
                    seen,
                    edited,
                    &child(&index.to_string()),
                    unshown,
                )?);
            }
            Ok(Value::Array(items))
        }
        // A view that writes nested lists flat (XML repeats elements): the
        // view's items are the host's leaves in order, so an item edited
        // there is merged into its leaf and the nesting stays.
        (Value::Array(host_items), Value::Array(seen), Value::Array(edited))
            if seen.len() == edited.len() =>
        {
            let mut leaves = Vec::new();
            array_leaves(host_items, &mut String::new(), &mut leaves);
            if leaves.len() != seen.len() {
                return replaced();
            }
            let mut merged = host.clone();
            for ((pointer, leaf), (seen, edited)) in leaves.iter().zip(seen.iter().zip(edited)) {
                if seen != edited
                    && let Some(slot) = merged.pointer_mut(pointer)
                {
                    *slot = merge_at(leaf, seen, edited, &format!("{at}{pointer}"), unshown)?;
                }
            }
            Ok(merged)
        }
        // A one-item list the view shows as its lone item (XML).
        (Value::Array(host), seen, edited)
            if host.len() == 1 && !seen.is_array() && !edited.is_array() =>
        {
            Ok(Value::Array(vec![merge_at(
                &host[0],
                seen,
                edited,
                &child("0"),
                unshown,
            )?]))
        }
        _ => replaced(),
    }
}

/// The non-list items of `items`, nested lists read through, with their
/// pointers relative to `items`.
fn array_leaves<'a>(items: &'a [Value], prefix: &mut String, out: &mut Vec<(String, &'a Value)>) {
    for (index, item) in items.iter().enumerate() {
        let length = prefix.len();
        prefix.push_str(&format!("/{index}"));
        match item {
            Value::Array(nested) => array_leaves(nested, prefix, out),
            leaf => out.push((prefix.clone(), leaf)),
        }
        prefix.truncate(length);
    }
}
