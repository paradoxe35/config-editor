//! The last check of every Edit-as save: the host's text, as the save
//! proposes it, read once by the host's own reader under the schema, holds
//! the document as the view reads it, value by value. The steps before only
//! propose a text; none of them decides what is acceptable. A value may read
//! otherwise only where the user left it as the view showed it (the host's
//! value kept), or where the one written list of XML spellings
//! (`spellings`) names it; anything else makes the save wait.

use super::super::codec;
use super::super::language::{ConfigLanguage, Repetition, pointer_key};
use super::merge::Paired;
use super::schema_readings;
use super::spellings::{self, Refused, Sides};
use serde_json::Value;

/// What a save's text is compared with.
pub(super) struct Expected<'a> {
    /// The view's reading, each scalar the user left as the editor wrote
    /// it holding its host value, keys spelled as the host spells them.
    pub(super) view: &'a Value,
    /// The view's reading of the host's text before the edit (what it
    /// showed), spelled alike; `None`: not known.
    pub(super) seen: Option<&'a Value>,
    /// The host's reading before the save.
    pub(super) old: &'a Value,
    /// Which host value the merge took each value from.
    pub(super) paired: Option<&'a Paired>,
    /// The text the user typed for the scalar at a pointer.
    pub(super) typed: &'a dyn Fn(&str) -> Option<String>,
    /// The view writes every scalar as text (XML).
    pub(super) from_xml: bool,
    /// How the view writes a list by repeating its key.
    pub(super) view_repeats: Repetition,
    /// The changed scalars the host writes in its own form, by pointer.
    pub(super) own_forms: &'a std::collections::HashMap<String, Value>,
    /// The keys in the order the user moved them to, when they moved any
    /// among those the view showed.
    pub(super) order: Option<&'a Value>,
}

/// Where `text` (a save's proposed host text) reads back otherwise than
/// `expected` says: the first value it reads differently that is neither
/// left as the view showed it nor a documented spelling, or, when the user
/// moved keys, the first key the host holds in another order. `None` when
/// it reads back as the view holds it.
pub(super) fn first_mismatch(
    host: &dyn ConfigLanguage,
    text: &str,
    expected: &Expected<'_>,
    shape: Option<&Value>,
) -> Option<Mismatch> {
    #[cfg(test)]
    READS.with(|reads| reads.set(reads.get() + 1));
    let Ok(read) = codec::read(host, text, shape) else {
        return Some(Mismatch::Value("/".to_string()));
    };
    let read = host.spelled_like(text, read, expected.view);
    let held = |at: &str| spellings::held(expected.old, expected.paired, &read, at);
    let refused = |at: &str, view: &Value| refused(shape, &read, at, view);
    let own_form = |at: &str| expected.own_forms.get(at);
    let sides = Sides {
        from_xml: expected.from_xml,
        into_xml: host.scalars_are_text(),
        view_repeats: expected.view_repeats,
        host_repeats: host.repetition(),
        own_form: &own_form,
        view: expected.view,
        saved: &read,
        held: &held,
        typed: expected.typed,
        refused: &refused,
    };
    let left = |at: &str| left_as_shown(expected, &read, at);
    if let Err(pointer) = spellings::explained(&sides, &left) {
        return Some(Mismatch::Value(if pointer.is_empty() {
            "/".to_string()
        } else {
            pointer
        }));
    }
    if let Some(order) = expected.order {
        // A key the host writes in another map (one a YAML merge brings
        // in) has no place in this one.
        let written = |pointer: &str| codec::written_in_its_map(host, text, pointer);
        return out_of_order(&read, order, String::new(), &written).map(Mismatch::Order);
    }
    None
}

/// Whether the user left the value at `at` as the view showed it, and the
/// save keeps the host's value there.
fn left_as_shown(expected: &Expected<'_>, read: &Value, at: &str) -> bool {
    // Where the host held it: under its old name, for a key renamed.
    let source = spellings::held_at(expected.old, expected.paired, read, at);
    let shown = expected.seen.and_then(|seen| seen.pointer(&source));
    let now = expected.view.pointer(at);
    let kept = read.pointer(at).zip(expected.old.pointer(&source));
    let unchanged = match (shown, now) {
        (Some(shown), Some(now)) => spellings::same(shown, now),
        // A value the view could not show (XML writes no empty list).
        (None, None) => expected.seen.is_some(),
        _ => false,
    };
    unchanged && kept.is_some_and(|(read, old)| spellings::same(read, old))
}

/// What the schema makes of `view` at `at` in the saved document `read`
/// in place of the saved value: the one question the save's proposal asks
/// too (`schema_readings::prefers`), each way round.
fn refused(shape: Option<&Value>, read: &Value, at: &str, view: &Value) -> Refused {
    let (Some(schema), Some(saved)) = (shape, read.pointer(at)) else {
        return Refused::default();
    };
    let mut with_view = read.clone();
    if let Some(slot) = with_view.pointer_mut(at) {
        *slot = view.clone();
    }
    Refused {
        view: schema_readings::prefers(&with_view, at, saved, schema),
        saved: schema_readings::prefers(read, at, view, schema),
    }
}

#[cfg(test)]
thread_local! {
    /// How many times a save's last check read a proposed text.
    pub(super) static READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How the host reads a save back otherwise than the view holds it.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Mismatch {
    /// The value at the pointer reads otherwise (or is missing, or added).
    Value(String),
    /// The key at the pointer is not where the view moved it: the host's
    /// writer puts it elsewhere (XML writes an element's attributes before
    /// its text and children).
    Order(String),
}

/// Whether `edited` holds keys of one of `seen`'s maps in another order
/// among themselves (a key renamed or added is no move).
pub(super) fn reorders(seen: &Value, edited: &Value) -> bool {
    out_of_order(edited, seen, String::new(), &|_| true).is_some()
}

/// The pointer of the first key `read` holds in another place than `like`
/// does among the keys both hold that are `written` in their map (by
/// pointer), depth first; `None` when every map's keys are in `like`'s
/// order. Of a map's keys out of order, the first `like` moved among the
/// others (not one of those that kept their order).
fn out_of_order(
    read: &Value,
    like: &Value,
    pointer: String,
    written: &dyn Fn(&str) -> bool,
) -> Option<String> {
    let child = |key: &str| format!("{pointer}/{}", pointer_key(key));
    match (read, like) {
        (Value::Object(read), Value::Object(like)) => {
            let read_keys = read.keys().filter(|key| like.contains_key(*key));
            let like_keys = like.keys().filter(|key| read.contains_key(*key));
            if !like_keys.clone().eq(read_keys.clone()) {
                // Only then is each key's place looked up.
                let placed = |key: &&String| written(&child(key));
                let read_keys: Vec<&String> = read_keys.filter(placed).collect();
                let like_keys: Vec<&String> = like_keys.filter(placed).collect();
                if like_keys != read_keys {
                    return Some(child(moved_key(&like_keys, &read_keys)));
                }
            }
            like.iter()
                .find_map(|(key, like)| out_of_order(read.get(key)?, like, child(key), written))
        }
        (Value::Array(read), Value::Array(like)) => {
            read.iter()
                .zip(like)
                .enumerate()
                .find_map(|(index, (read, like))| {
                    out_of_order(read, like, format!("{pointer}/{index}"), written)
                })
        }
        _ => None,
    }
}

/// The most keys of one map whose moved key is told apart from those that
/// kept their order (the table grows with their square).
const MOVED_KEY_LIMIT: usize = 400;

/// Of `like`'s keys, which `read` holds in another order, the first one
/// not among the longest run both hold in the same order: the one moved.
/// Past [`MOVED_KEY_LIMIT`], the first one out of its place.
fn moved_key<'a>(like: &[&'a String], read: &[&'a String]) -> &'a String {
    let first_apart = || {
        like.iter()
            .zip(read)
            .find(|(left, right)| left != right)
            .map_or(like[0], |(left, _)| *left)
    };
    if like.len() > MOVED_KEY_LIMIT {
        return first_apart();
    }
    let kept = super::merge::longest_common(like, read);
    like.iter()
        .find(|key| !kept.contains(key))
        .copied()
        .unwrap_or_else(first_apart)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::READS;
    use crate::editor::Language;
    use crate::editor::bridge::Bridge;
    use crate::editor::schema::build_schema;

    /// Each save that reaches its last check reads the proposed text there
    /// exactly once: the check asks the host's reader, it does not read the
    /// text again per value it compares (P5). A matrix of hosts and views,
    /// with and without a schema, each a value edit that saves.
    #[test]
    fn the_last_check_reads_the_proposed_text_once_per_save() {
        let schema = std::sync::Arc::new(
            build_schema(r#"{"properties":{"port":{"type":"integer"},"name":{"type":"string"}}}"#)
                .unwrap(),
        );
        let doc = json!({"name": "web", "port": 80, "tags": ["a", "b"]});
        for &host in Language::ALL {
            for &edit in Language::ALL {
                for schema in [None, Some(schema.clone())] {
                    let Some(bridge) = Bridge::new(host, edit, schema.clone()) else {
                        continue;
                    };
                    let Ok(host_text) =
                        crate::editor::codec::render(&doc, host.behavior(), None, None)
                    else {
                        continue;
                    };
                    let shown = bridge.to_edit(&host_text.text).unwrap().text;
                    let edited = shown.replacen("80", "81", 1);
                    assert_ne!(edited, shown, "{host} as {edit}: {shown}");
                    READS.with(|reads| reads.set(0));
                    let saved = bridge.to_host(&edited, &host_text.text);
                    assert!(saved.is_ok(), "{host} as {edit}: {saved:?}");
                    assert_eq!(READS.with(std::cell::Cell::get), 1, "{host} as {edit}");
                }
            }
        }
    }

    #[test]
    fn the_first_key_out_of_its_place_is_named() {
        let out_of_order =
            |read: &_, like: &_, pointer| super::out_of_order(read, like, pointer, &|_| true);
        let like = json!({"r": {"s": {"#text": "t", "a": 3}, "u": 1}});
        let read = json!({"r": {"s": {"a": 3, "#text": "t"}, "u": 1}});
        assert_eq!(
            out_of_order(&read, &like, String::new()).as_deref(),
            Some("/r/s/#text")
        );
        assert_eq!(out_of_order(&like, &like, String::new()), None);
        // Keys only one side holds are no order.
        let read = json!({"r": {"s": {"#text": "t", "b": 0, "a": 3}, "u": 1}});
        assert_eq!(out_of_order(&read, &like, String::new()), None);
        let like = json!([{"a/b": 1, "c": 2}]);
        let read = json!([{"c": 2, "a/b": 1}]);
        assert_eq!(
            out_of_order(&read, &like, String::new()).as_deref(),
            Some("/0/a~1b")
        );
    }
}
