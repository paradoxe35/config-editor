//! How the host holds each node of a view, recorded where the view is
//! written from it: a view made from the host's text (a switch, a host
//! value arriving, a save), or a view converted from another one. The view
//! and the value it was written from hold the same document, so walking
//! the two together pairs each node of the view with the node it was
//! written from. A language that writes a list by repeating its key (XML)
//! writes a list of one as its item, and one that reads a value written
//! once as a list of one (an HCL block under a schema wanting a list) may
//! read it so where the other does not: the walk records which.

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::language::pointer_key;
use super::super::merge::array_leaves;

/// How the host holds a view's node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Holding {
    /// As the view holds it.
    #[default]
    AsShown,
    /// A list of one that the view writes as its item: what the view holds
    /// inside the node is under the list's `/0`.
    ListShownAlone,
}

/// Each node of a view, by its pointer there, and where the value it was
/// written from holds it, where the two differ.
#[derive(Clone, Debug, Default)]
pub(crate) struct HostPaths {
    by_view: HashMap<String, (String, Holding)>,
}

impl HostPaths {
    /// The nodes of `view` (a value read off a text written from `from`,
    /// or `from` read off a text written from it) paired with `from`'s.
    pub(crate) fn between(from: &Value, view: &Value) -> Self {
        let mut paths = HostPaths::default();
        paths.walk(from, view, &mut String::new(), &mut String::new());
        paths
    }

    /// The node `view` (a pointer of the view) is written from, and how it
    /// holds it: at the same pointer as shown where nothing is recorded.
    pub(crate) fn item(&self, view: &str) -> (String, Holding) {
        self.by_view
            .get(view)
            .cloned()
            .unwrap_or_else(|| (view.to_string(), Holding::AsShown))
    }

    /// The pointer of the value the view's node `view` writes: the item of
    /// a list of one it writes alone.
    pub(crate) fn value(&self, view: &str) -> String {
        let (pointer, held) = self.item(view);
        held.value_of(pointer)
    }

    fn note(&mut self, view: &str, from: &str, held: Holding) {
        if view != from || held != Holding::AsShown {
            self.by_view
                .entry(view.to_string())
                .or_insert_with(|| (from.to_string(), held));
        }
    }

    fn walk(&mut self, from: &Value, view: &Value, at: &mut String, shown: &mut String) {
        match (from, view) {
            (Value::Array(items), shown_value) if items.len() == 1 && !shown_value.is_array() => {
                self.note(shown, at, Holding::ListShownAlone);
                let length = at.len();
                at.push_str("/0");
                self.walk(&items[0], view, at, shown);
                at.truncate(length);
            }
            (from_value, Value::Array(items)) if items.len() == 1 && !from_value.is_array() => {
                self.note(shown, at, Holding::AsShown);
                let length = shown.len();
                shown.push_str("/0");
                self.walk(from, &items[0], at, shown);
                shown.truncate(length);
            }
            (Value::Object(held), Value::Object(map)) => {
                self.note(shown, at, Holding::AsShown);
                for (key, child) in map {
                    let Some(from_child) = held.get(key) else {
                        continue;
                    };
                    let (length, shown_length) = (at.len(), shown.len());
                    let key = pointer_key(key);
                    at.push('/');
                    at.push_str(&key);
                    shown.push('/');
                    shown.push_str(&key);
                    self.walk(from_child, child, at, shown);
                    at.truncate(length);
                    shown.truncate(shown_length);
                }
            }
            // A view that writes nested lists flat (XML repeats elements)
            // shows the host's leaves in order: each item is the leaf it
            // shows, not the host item at its index.
            (Value::Array(held), Value::Array(items))
                if held.iter().any(Value::is_array) && !items.iter().any(Value::is_array) =>
            {
                self.note(shown, at, Holding::AsShown);
                let mut leaves = Vec::new();
                array_leaves(held, &mut String::new(), &mut leaves);
                if leaves.len() != items.len() {
                    return;
                }
                for (index, ((pointer, leaf), item)) in leaves.into_iter().zip(items).enumerate() {
                    let (length, shown_length) = (at.len(), shown.len());
                    at.push_str(&pointer);
                    shown.push('/');
                    shown.push_str(&index.to_string());
                    self.walk(leaf, item, at, shown);
                    at.truncate(length);
                    shown.truncate(shown_length);
                }
            }
            (Value::Array(held), Value::Array(items)) => {
                self.note(shown, at, Holding::AsShown);
                for (index, (from_item, item)) in held.iter().zip(items).enumerate() {
                    let (length, shown_length) = (at.len(), shown.len());
                    let index = index.to_string();
                    at.push('/');
                    at.push_str(&index);
                    shown.push('/');
                    shown.push_str(&index);
                    self.walk(from_item, item, at, shown);
                    at.truncate(length);
                    shown.truncate(shown_length);
                }
            }
            _ => self.note(shown, at, Holding::AsShown),
        }
    }
}

impl Holding {
    /// The pointer of the value a node held so at `pointer` writes.
    pub(crate) fn value_of(self, pointer: String) -> String {
        match self {
            Holding::AsShown => pointer,
            Holding::ListShownAlone => pointer + "/0",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_list_of_one_written_as_its_item_names_the_list_and_its_item() {
        let paths = HostPaths::between(
            &json!({"a": [{"x": 1}], "b": 2}),
            &json!({"a": {"x": 1}, "b": 2}),
        );
        assert_eq!(
            paths.item("/a"),
            ("/a".to_string(), Holding::ListShownAlone)
        );
        assert_eq!(paths.value("/a"), "/a/0");
        assert_eq!(paths.item("/a/x").0, "/a/0/x");
        assert_eq!(paths.item("/b"), ("/b".to_string(), Holding::AsShown));
    }

    #[test]
    fn a_value_written_once_read_as_a_list_names_the_value() {
        let paths = HostPaths::between(&json!({"b": {"x": 1}}), &json!({"b": [{"x": 1}]}));
        assert_eq!(paths.item("/b").0, "/b");
        assert_eq!(paths.item("/b/0").0, "/b");
        assert_eq!(paths.item("/b/0/x").0, "/b/x");
    }

    #[test]
    fn a_nested_list_shown_flat_names_each_leaf() {
        for (host, flat, pairs) in [
            (
                json!({"m": [[1, 2], [[3], 4], 5]}),
                json!({"m": [1, 2, 3, 4, 5]}),
                &[
                    ("/m/0", "/m/0/0"),
                    ("/m/1", "/m/0/1"),
                    ("/m/2", "/m/1/0/0"),
                    ("/m/3", "/m/1/1"),
                    ("/m/4", "/m/2"),
                ][..],
            ),
            (
                json!({"m": [[1, 2]]}),
                json!({"m": [1, 2]}),
                &[("/m/0", "/m/0/0"), ("/m/1", "/m/0/1")][..],
            ),
            (
                json!({"m": [1, [{"k": 2}]]}),
                json!({"m": [1, {"k": 2}]}),
                &[("/m/0", "/m/0"), ("/m/1/k", "/m/1/0/k")][..],
            ),
        ] {
            let paths = HostPaths::between(&host, &flat);
            for (view, from) in pairs {
                assert_eq!(paths.value(view), *from, "{view} of {host}");
                assert_eq!(host.pointer(from), flat.pointer(view), "{view} of {host}");
            }
        }
    }

    #[test]
    fn a_list_of_one_holding_a_key_named_zero_is_walked_by_its_values() {
        let paths = HostPaths::between(&json!({"l": [{"0": 5}]}), &json!({"l": {"0": 5}}));
        assert_eq!(paths.item("/l/0").0, "/l/0/0");
    }
}
