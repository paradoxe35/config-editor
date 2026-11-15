//! The last check of every Edit-as save: the host's text, as the save
//! leaves it, read back by the host's own reader, holds the document as the
//! view holds it. Each step of a save checks its own part (a patch reads
//! back as its target), but only this one checks what is written: a step
//! that patches one thing and leaves another (an order the host's writer
//! puts its own way, a key written in another kind of place) shows here,
//! and the save waits instead of writing what the host reads otherwise.

use super::super::codec::{self, escape};
use super::super::language::ConfigLanguage;
use serde_json::Value;

/// The host's text a save writes.
pub(super) struct Saved<'a> {
    pub(super) text: &'a str,
    /// The numbers a host whose values are all text holds as the text they
    /// were typed in (`4.10`), by pointer: read back, that text is the
    /// number.
    pub(super) as_text: &'a [(String, String)],
}

/// Where `saved` reads back otherwise than `target` (the document as the
/// view holds it, in the host's types): the first value it reads
/// differently, or, when the user `moved` keys in the view, the first key
/// the host holds in another order than `target`. `None` when it reads
/// back as the view holds it.
pub(super) fn first_mismatch(
    host: &dyn ConfigLanguage,
    saved: &Saved<'_>,
    target: &Value,
    moved: bool,
    shape: Option<&Value>,
) -> Option<Mismatch> {
    let Ok(read) = codec::read(host, saved.text, shape) else {
        return Some(Mismatch::Value("/".to_string()));
    };
    let mut read = host.spelled_like(saved.text, read, target);
    // Only a text a reader takes for the number it stands for.
    for (pointer, text) in saved.as_text {
        if let (Some(slot), Some(number)) = (read.pointer_mut(pointer), target.pointer(pointer))
            && slot.as_str() == Some(text.as_str())
            && super::typed_spellings::text_reads_as_number(text, number)
        {
            *slot = number.clone();
        }
    }
    let mut differing = Vec::new();
    codec::differences(target, &read, String::new(), &mut differing);
    if let Some(pointer) = differing.into_iter().next() {
        return Some(Mismatch::Value(pointer));
    }
    if moved {
        // A key the host writes in another map (one a YAML merge brings
        // in) has no place in this one.
        let written = |pointer: &str| codec::written_in_its_map(host, saved.text, pointer);
        return out_of_order(&read, target, String::new(), &written).map(Mismatch::Order);
    }
    None
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
    let child = |key: &str| format!("{pointer}/{}", escape(key));
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
