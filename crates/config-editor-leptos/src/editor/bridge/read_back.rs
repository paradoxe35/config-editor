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
    for (pointer, text) in saved.as_text {
        if let (Some(slot), Some(number)) = (read.pointer_mut(pointer), target.pointer(pointer))
            && slot.as_str() == Some(text.as_str())
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
        return out_of_order(&read, target, String::new()).map(Mismatch::Order);
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
    out_of_order(edited, seen, String::new()).is_some()
}

/// The pointer of the first key `read` holds in another place than `like`
/// does among the keys both hold, depth first; `None` when every map's
/// keys are in `like`'s order.
fn out_of_order(read: &Value, like: &Value, pointer: String) -> Option<String> {
    match (read, like) {
        (Value::Object(read), Value::Object(like)) => {
            let read_keys = read.keys().filter(|key| like.contains_key(*key));
            let like_keys = like.keys().filter(|key| read.contains_key(*key));
            if let Some((key, _)) = like_keys.zip(read_keys).find(|(left, right)| left != right) {
                return Some(format!("{pointer}/{}", escape(key)));
            }
            like.iter().find_map(|(key, child)| {
                let read = read.get(key)?;
                out_of_order(read, child, format!("{pointer}/{}", escape(key)))
            })
        }
        (Value::Array(read), Value::Array(like)) => {
            read.iter()
                .zip(like)
                .enumerate()
                .find_map(|(index, (read, like))| {
                    out_of_order(read, like, format!("{pointer}/{index}"))
                })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::out_of_order;
    use serde_json::json;

    #[test]
    fn the_first_key_out_of_its_place_is_named() {
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
