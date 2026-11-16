//! The documented ways a save with XML on one side reads back otherwise
//! than the view, told value by value: a failing save is in one of these
//! classes only when each value it reads otherwise is one of them, so an
//! allowance (see `known`) covers that value and no other difference.

use std::collections::HashSet;

use serde_json::Value;

use super::reference::{Both, same};
use crate::editor::Language;
use crate::editor::decimal_text::decimal;
use crate::editor::language::{pointer_key, unescaped_key};

/// An XML view shows a list of one as its item where no schema says it is
/// a list (and a list cut to one item looks the same), and its save keeps
/// the host's list there, its item told as any other value.
pub(super) const KEPT_LIST: &str = "a save keeps the host's list where the XML view shows one item";
/// A list cut to one item in a view of an XML host is written as one
/// element, which the host reads as that item.
pub(super) const CUT_TO_ONE: &str =
    "a save writes a list cut to one item as that item in an XML host";
/// An XML element emptied where the host holds text saves as the empty
/// text, where the view reads the empty element as an empty object.
pub(super) const EMPTIED_TEXT: &str =
    "a save writes an emptied XML element as the host's empty text";
/// A number typed in an XML view in another spelling (`08`, `2.50`) where
/// the host holds a number saves as that number.
pub(super) const RESPELLED: &str =
    "a save keeps the host's number for a number typed in another spelling";
/// XML text the view reads as one type saves in the host's type where the
/// schema takes the host's type (`"1"` made `12` stays `"12"`).
pub(super) const KEPT_TEXT: &str =
    "a save keeps the host's text where the XML view's text reads as another type";
/// XML text the schema refuses read either way (as the view reads it, and
/// in the host's type), or does not tell apart, keeps the host's type: the
/// view's reading is kept only where the schema takes it and refuses the
/// host's type.
pub(super) const KEPT_TYPE: &str =
    "a save keeps the host's type of a value the schema takes in no reading";

/// A save and what it is compared with.
pub(super) struct Sides<'a> {
    pub(super) both: &'a Both,
    pub(super) host: Language,
    pub(super) edit: Language,
    /// The host's document before the save.
    pub(super) before: &'a Value,
    /// The view's document.
    pub(super) view: &'a Value,
    /// The host's document read back from the save.
    pub(super) saved: &'a Value,
}

/// The class of the save when XML's spelling explains each value it reads
/// otherwise than the view: the first such value's.
pub(super) fn explained(sides: &Sides<'_>) -> Option<&'static str> {
    classes_at(sides, "", sides.view, false)?.first().copied()
}

/// The class of each value the saved value at `at` holds otherwise than
/// `view`, the view's value there; `None` when one has none. `listed`: the
/// value is (in) the item of a list the save kept, which no schema reads.
fn classes_at(
    sides: &Sides<'_>,
    at: &str,
    view: &Value,
    listed: bool,
) -> Option<Vec<&'static str>> {
    let mut pointers = Vec::new();
    differing(
        sides.saved.pointer(at)?,
        view,
        &mut String::new(),
        &mut pointers,
    );
    pointers
        .iter()
        .map(|inner| {
            let shown = Shown {
                value: view.pointer(inner)?,
                parent: inner
                    .rsplit_once('/')
                    .and_then(|(parent, _)| view.pointer(parent)),
            };
            spelling(sides, &format!("{at}{inner}"), shown, listed)
        })
        .collect()
}

/// A value of the view, and the value holding it.
#[derive(Clone, Copy)]
struct Shown<'a> {
    value: &'a Value,
    parent: Option<&'a Value>,
}

/// The pointers at which `saved` and `view` differ, each the outermost
/// where they differ in kind, keys or length.
fn differing(saved: &Value, view: &Value, at: &mut String, out: &mut Vec<String>) {
    let length = at.len();
    match (saved, view) {
        (Value::Object(saved), Value::Object(view)) => {
            let keys = saved
                .keys()
                .chain(view.keys().filter(|key| !saved.contains_key(*key)));
            for key in keys {
                at.push('/');
                at.push_str(&pointer_key(key));
                match (saved.get(key), view.get(key)) {
                    (Some(saved), Some(view)) => differing(saved, view, at, out),
                    _ => out.push(at.clone()),
                }
                at.truncate(length);
            }
        }
        (Value::Array(saved), Value::Array(view)) if saved.len() == view.len() => {
            for (index, (saved, view)) in saved.iter().zip(view).enumerate() {
                at.push('/');
                at.push_str(&index.to_string());
                differing(saved, view, at, out);
                at.truncate(length);
            }
        }
        _ if same(saved, view) => {}
        _ => out.push(at.clone()),
    }
}

/// Which documented spelling explains the saved value at `at`, which the
/// view holds as `shown`, if one does.
fn spelling(sides: &Sides<'_>, at: &str, shown: Shown<'_>, listed: bool) -> Option<&'static str> {
    let view = shown.value;
    let saved = sides.saved.pointer(at)?;
    let before = held(sides, at, shown.parent, saved);
    let into_xml = sides.host == Language::XML;
    let from_xml = sides.edit == Language::XML;
    match (before, view, saved) {
        (Some(Value::Array(_)), _, Value::Array(items))
            if from_xml && items.len() == 1 && !view.is_array() =>
        {
            classes_at(sides, &format!("{at}/0"), view, true).map(|_| KEPT_LIST)
        }
        (Some(Value::Array(_)), Value::Array(items), _)
            if into_xml && matches!(&items[..], [only] if same(only, saved)) =>
        {
            Some(CUT_TO_ONE)
        }
        (Some(Value::String(_)), Value::Object(map), Value::String(text))
            if from_xml && map.is_empty() && text.is_empty() =>
        {
            Some(EMPTIED_TEXT)
        }
        (Some(Value::Number(_)), Value::String(text), Value::Number(_))
            if from_xml && respelled(text, saved) =>
        {
            Some(RESPELLED)
        }
        // An item no schema reads keeps the host's type (see `text_scalars`).
        (Some(before), _, _)
            if from_xml && kind(before) == kind(saved) && same_text(view, saved) =>
        {
            if listed {
                Some(KEPT_LIST)
            } else {
                in_host_type(sides, at, view)
            }
        }
        _ => None,
    }
}

/// The host's value at `at` before the save, or, where the view renamed a
/// key to it, the value of that key (one of the same object the view's
/// object `shown` no longer has) of the saved value's kind.
fn held<'a>(
    sides: &Sides<'a>,
    at: &str,
    shown: Option<&Value>,
    saved: &Value,
) -> Option<&'a Value> {
    if let Some(before) = sides.before.pointer(at) {
        return Some(before);
    }
    let (parent, _) = at.rsplit_once('/')?;
    match (sides.before.pointer(parent), shown) {
        (Some(Value::Object(held)), Some(Value::Object(shown))) => held
            .iter()
            .find(|(key, value)| kind(value) == kind(saved) && !shown.contains_key(*key))
            .map(|(_, value)| value),
        _ => None,
    }
}

/// The class of a value saved in the host's type, by what the schema
/// makes of its two readings: the view's reading is kept only where the
/// schema takes it and refuses the host's type.
fn in_host_type(sides: &Sides<'_>, at: &str, view: &Value) -> Option<&'static str> {
    let saved = sides.saved.pointer(at)?;
    let as_viewed = errors_about(sides, at, Some(view));
    let as_saved = errors_about(sides, at, Some(saved));
    let without = errors_about(sides, at, None);
    // A reading is refused by an error at the value (or inside it), or by
    // one above it (options of an object holding it) that the other
    // reading does not have, or that goes with the value.
    let refused = |errors: &Errors, other: &Errors| {
        errors.iter().any(|error| {
            let (place, _) = error;
            place == at
                || place.starts_with(&format!("{at}/"))
                || !other.contains(error)
                || without
                    .as_ref()
                    .is_some_and(|without| !without.contains(error))
        })
    };
    let (as_viewed, as_saved) = (as_viewed.unwrap_or_default(), as_saved.unwrap_or_default());
    match (
        refused(&as_viewed, &as_saved),
        refused(&as_saved, &as_viewed),
    ) {
        // The schema takes the view's reading and not the host's type: the
        // save should have kept the reading.
        (false, true) => None,
        (true, true) => Some(KEPT_TYPE),
        (_, false) if saved.is_string() => Some(KEPT_TEXT),
        (_, false) => Some(KEPT_TYPE),
    }
}

/// Errors as place and keyword.
type Errors = HashSet<(String, String)>;

/// The reference's errors for the saved document with `value` at `at`
/// (`None`: without the value, where an object holds it), at the value,
/// inside it or above it.
fn errors_about(sides: &Sides<'_>, at: &str, value: Option<&Value>) -> Option<Errors> {
    let mut document = sides.saved.clone();
    match value {
        Some(value) => *document.pointer_mut(at)? = value.clone(),
        None => {
            let (parent, key) = at.rsplit_once('/')?;
            let Value::Object(map) = document.pointer_mut(parent)? else {
                return None;
            };
            map.shift_remove(&unescaped_key(key))?;
        }
    }
    let about = |place: &str| {
        place == at
            || place.starts_with(&format!("{at}/"))
            || at.starts_with(&format!("{place}/"))
            || place.is_empty()
    };
    let errors = sides
        .both
        .reference
        .iter_errors(&document)
        .filter(|error| about(error.instance_path().as_str()))
        .map(|error| {
            let place = error.instance_path().as_str().to_string();
            (place, error.kind().keyword().to_string())
        })
        .collect();
    Some(errors)
}

/// Whether `text` spells the number `number` otherwise than its plain
/// form (`08` for 8, `2.50` for 2.5).
fn respelled(text: &str, number: &Value) -> bool {
    let text = text.trim();
    let plain = number.to_string();
    decimal(text, false).is_some_and(|read| same(&read, number)) && text != plain
}

fn kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Whether `left` and `right` are scalars of two types XML writes as the
/// same text (`"8"` and `8`, `"true"` and `true`).
fn same_text(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(text), other) | (other, Value::String(text)) => match other {
            Value::Number(_) => decimal(text.trim(), false).is_some_and(|read| same(&read, other)),
            Value::Bool(flag) => text.trim() == flag.to_string(),
            _ => false,
        },
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::super::reference::compile;
    use super::*;

    /// The class of a save from an XML view of a JSON host (`into_xml`: of
    /// an XML host from a JSON view).
    fn class(
        schema: &Value,
        into_xml: bool,
        (before, view, saved): (Value, Value, Value),
    ) -> Option<&'static str> {
        let both = compile(schema).unwrap();
        let (host, edit) = if into_xml {
            (Language::XML, Language::JSON)
        } else {
            (Language::JSON, Language::XML)
        };
        explained(&Sides {
            both: &both,
            host,
            edit,
            before: &before,
            view: &view,
            saved: &saved,
        })
    }

    /// Each documented spelling explains its one value, and a save
    /// reading otherwise anywhere else, or in another shape, has no class.
    #[test]
    fn each_spelling_covers_exactly_its_value() {
        let any = json!({});
        let list =
            |view: Value, saved: Value| class(&any, false, (json!({"a": ["x"]}), view, saved));
        assert_eq!(
            list(json!({"a": "y"}), json!({"a": ["y"]})),
            Some(KEPT_LIST)
        );
        assert_eq!(list(json!({"a": 8}), json!({"a": ["8"]})), Some(KEPT_LIST));
        assert_eq!(
            list(json!({"a": "y", "b": 1}), json!({"a": ["y"], "b": 2})),
            None
        );
        // A list renamed and cut to one item; a list typed over a string.
        let renamed = (json!({"a": [0, 0]}), json!({"b": 0}), json!({"b": [0]}));
        assert_eq!(class(&any, false, renamed), Some(KEPT_LIST));
        let over_text = (json!({"a": "x"}), json!({"a": "y"}), json!({"a": ["y"]}));
        assert_eq!(class(&any, false, over_text), None);

        let cut = |before: Value| class(&any, true, (before, json!({"c": [0]}), json!({"c": 0})));
        assert_eq!(cut(json!({"c": [0, 0]})), Some(CUT_TO_ONE));
        assert_eq!(cut(json!({"c": "0"})), None);

        let emptied =
            |before: Value| class(&any, false, (before, json!({"m": {}}), json!({"m": ""})));
        assert_eq!(emptied(json!({"m": "a"})), Some(EMPTIED_TEXT));
        assert_eq!(emptied(json!({"m": {"k": 1}})), None);

        let number = |view: Value, saved: Value| class(&any, false, (json!({"a": 0}), view, saved));
        assert_eq!(number(json!({"a": "08"}), json!({"a": 8})), Some(RESPELLED));
        assert_eq!(
            number(json!({"a": "2.50"}), json!({"a": 2.5})),
            Some(RESPELLED)
        );
        assert_eq!(number(json!({"a": "08"}), json!({"a": 9})), None);

        // A string kept where the schema takes it, or takes neither
        // reading; not where it takes only the view's.
        let typed = |schema: Value| {
            let schema = json!({"properties": {"a": schema}});
            class(
                &schema,
                false,
                (json!({"a": "x"}), json!({"a": 8}), json!({"a": "8"})),
            )
        };
        assert_eq!(typed(json!({"type": "string"})), Some(KEPT_TEXT));
        assert_eq!(
            typed(json!({"type": "string", "enum": ["q"]})),
            Some(KEPT_TYPE)
        );
        assert_eq!(typed(json!({"type": "integer"})), None);
        // The host's string under a key the view renamed.
        let renamed = (
            json!({"name": "true"}),
            json!({"a": true}),
            json!({"a": "true"}),
        );
        assert_eq!(class(&any, false, renamed), Some(KEPT_TEXT));
        let new_key = (json!({"name": 1}), json!({"a": true}), json!({"a": "true"}));
        assert_eq!(class(&any, false, new_key), None);
        assert_eq!(
            typed(json!({"oneOf": [{"type": "integer"}, {"type": "boolean"}]})),
            None
        );
    }

    /// A list cut to one item in an XML view of any other host stays a
    /// list in the file: the kept list explains that value, where the host
    /// held a list and the view is XML, and nothing else.
    #[test]
    fn a_list_cut_to_one_in_an_xml_view_is_kept_for_every_host_and_only_there() {
        let any = json!({});
        let both = compile(&any).unwrap();
        let class =
            |host: Language, edit: Language, (before, view, saved): (Value, Value, Value)| {
                explained(&Sides {
                    both: &both,
                    host,
                    edit,
                    before: &before,
                    view: &view,
                    saved: &saved,
                })
            };
        let hosts = Language::ALL
            .iter()
            .copied()
            .filter(|language| *language != Language::XML);
        for host in hosts {
            let xml = |sides| class(host, Language::XML, sides);
            // Cut to one, at the top or nested, the item kept or edited.
            let cut = (
                json!({"mode": ["x", "t"]}),
                json!({"mode": "x"}),
                json!({"mode": ["x"]}),
            );
            assert_eq!(xml(cut.clone()), Some(KEPT_LIST), "{host}");
            let nested = (
                json!({"a": {"mode": [1, 2, 3]}}),
                json!({"a": {"mode": 2}}),
                json!({"a": {"mode": [2]}}),
            );
            assert_eq!(xml(nested), Some(KEPT_LIST), "{host}");
            let edited = (
                json!({"mode": ["x", "t"]}),
                json!({"mode": "y"}),
                json!({"mode": ["y"]}),
            );
            assert_eq!(xml(edited), Some(KEPT_LIST), "{host}");
            // Not where the view is not XML.
            for edit in Language::ALL.iter().copied() {
                if edit != Language::XML && edit != host {
                    assert_eq!(class(host, edit, cut.clone()), None, "{host} from {edit}");
                }
            }
            // Not where the host held no list there.
            let over_text = (
                json!({"mode": "x"}),
                json!({"mode": "x"}),
                json!({"mode": ["x"]}),
            );
            assert_eq!(xml(over_text), None, "{host}");
            // Not for a list kept whole, nor one whose item reads otherwise.
            let whole = (
                json!({"mode": ["x", "t"]}),
                json!({"mode": "x"}),
                json!({"mode": ["x", "t"]}),
            );
            assert_eq!(xml(whole), None, "{host}");
            let other_item = (
                json!({"mode": ["x", "t"]}),
                json!({"mode": "x"}),
                json!({"mode": ["t"]}),
            );
            assert_eq!(xml(other_item), None, "{host}");
            // Not where another value reads otherwise too.
            let beside = (
                json!({"mode": ["x", "t"], "b": 1}),
                json!({"mode": "x", "b": 1}),
                json!({"mode": ["x"], "b": 2}),
            );
            assert_eq!(xml(beside), None, "{host}");
        }
    }
}
