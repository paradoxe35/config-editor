//! Which tag (YAML `!Sub`) a save would drop: each tag of the host's text
//! stays on its value, where the value stays, where it moved to (a list
//! reordered), or goes with it (an item removed).

use serde_json::Value;

/// The first of the host's tags `before` (each a pointer and its tag, in
/// the text that read `old`) that the saved text's tags `after` (it reads
/// `target`) no longer carry on a value the edit kept.
pub(super) fn first_lost<'a>(
    before: &'a [(String, String)],
    after: &[(String, String)],
    old: &Value,
    target: &Value,
) -> Option<&'a (String, String)> {
    before.iter().find(|(pointer, tag)| {
        let tagged = |at: &String| after.iter().any(|(known, kept)| known == at && kept == tag);
        if tagged(pointer) {
            return false;
        }
        let Some(value) = old.pointer(pointer) else {
            return target.pointer(pointer).is_some();
        };
        // Moved: the tag is on it in its new place.
        if after
            .iter()
            .any(|(known, kept)| kept == tag && target.pointer(known) == Some(value))
        {
            return false;
        }
        // Still among its siblings (it moved without its tag), or its place
        // still there holding no other sibling of before (edited in place).
        let (parent, _) = pointer.rsplit_once('/').unwrap_or(("", ""));
        let siblings = |document: &Value| -> Vec<Value> {
            match document.pointer(parent) {
                Some(Value::Array(items)) => items.clone(),
                Some(Value::Object(map)) => map.values().cloned().collect(),
                _ => Vec::new(),
            }
        };
        let (old_siblings, new_siblings) = (siblings(old), siblings(target));
        let still_there = new_siblings.contains(value);
        let edited_in_place = target
            .pointer(pointer)
            .is_some_and(|now| !old_siblings.contains(now));
        still_there || edited_in_place
    })
}
