//! The documented ways a save with XML on one side reads back otherwise
//! than the view, told value by value: a failing save is in one of these
//! classes only when each value it reads otherwise is one of them, so an
//! allowance (see `known`) covers that value and no other difference.

use std::collections::HashSet;

use serde_json::Value;

use super::reference::{Both, same};
use crate::editor::Language;
use crate::editor::applies::scalar_text;
use crate::editor::bridge::links::HostPaths;
use crate::editor::bridge::paired;
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
    /// Which host value the save took each value from (see [`held`]);
    /// `None`: each at its own pointer.
    pub(super) paired: Option<&'a paired::Paired>,
    /// The document the view's text was written from (what the user
    /// typed, each scalar as its text); `None`: not known.
    pub(super) typed: Option<&'a Value>,
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
        .map(|inner| spelling(sides, &format!("{at}{inner}"), view.pointer(inner)?, listed))
        .collect()
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
/// view holds as `view`, if one does.
fn spelling(sides: &Sides<'_>, at: &str, view: &Value, listed: bool) -> Option<&'static str> {
    let saved = sides.saved.pointer(at)?;
    let before = held(sides, at);
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
        // An item no schema reads keeps the host's type (see `text_scalars`),
        // and so does a host scalar the view reads as a list's one item.
        (Some(before), _, _)
            if from_xml
                && kind(before) == kind(saved)
                && same_text(lone_item(view), saved)
                && saved_as_typed(sides, at, saved) =>
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

/// The host's value before the save that the saved value at `at` was
/// taken from.
///
/// Which host value that is, is the save's own record (`paired`, the
/// merge's pairing): a view typed whole carries no links, so only the
/// merge knows which host key a key it renamed stands for, and every other
/// value stays at its host pointer. The judge takes from the record only
/// that identity, and only where the host agrees it can be one (a renamed
/// key's host key is in the host's map there, its new name is not, and
/// the merged map no longer holds the old one); what the save wrote for
/// the value is still judged here, against the host's value and the
/// view's, by the documented spellings. So a wrong pairing can at worst
/// compare a value with another of the host's values the view renamed
/// away, never excuse a value the host did not hold in that form.
fn held<'a>(sides: &Sides<'a>, at: &str) -> Option<&'a Value> {
    let Some(paired) = sides.paired else {
        return sides.before.pointer(at);
    };
    // The saved value's place in the merged value: the host may read a
    // list of one the merge wrote as its item (an XML host), the walk of
    // the two pairs them (see `HostPaths`).
    let (merged, _) = HostPaths::between(&paired.merged, sides.saved).item(at);
    let (mut placed, mut host) = (String::new(), String::new());
    for segment in merged.split('/').skip(1) {
        let renamed = paired.renames.iter().find(|renamed| {
            renamed.at == placed
                && pointer_key(&renamed.key) == segment
                && renames_one(sides.before.pointer(&host), &paired.merged, renamed)
        });
        host.push('/');
        match renamed {
            Some(renamed) => host.push_str(&pointer_key(&renamed.from)),
            None => host.push_str(segment),
        }
        placed.push('/');
        placed.push_str(segment);
    }
    sides.before.pointer(&host)
}

/// Whether the host's map `host` and the merged value agree that the
/// merge's `renamed` can be a rename: the host held the old key and not
/// the new one, and the merged map holds the new one in its place.
fn renames_one(host: Option<&Value>, merged: &Value, renamed: &paired::Renamed) -> bool {
    let (Some(Value::Object(host)), Some(Value::Object(merged))) =
        (host, merged.pointer(&renamed.at))
    else {
        return false;
    };
    host.contains_key(&renamed.from)
        && !host.contains_key(&renamed.key)
        && merged.contains_key(&renamed.key)
        && !merged.contains_key(&renamed.from)
}

/// The class of a value saved in the host's type, by what the schema
/// makes of its two readings: the view's reading is kept only where the
/// schema takes it and refuses the host's type.
fn in_host_type(sides: &Sides<'_>, at: &str, view: &Value) -> Option<&'static str> {
    let saved = sides.saved.pointer(at)?;
    let as_viewed = errors_about(sides, at, Some(view));
    let as_saved = errors_about(sides, at, Some(saved));
    let without = errors_about(sides, at, None);
    // A reading is refused by an error at the value (or inside it), by one
    // anywhere that the other reading does not have (the two documents
    // differ only in the value: an `if` beside it reading it), or by one
    // above it (options of an object holding it) that goes with the value.
    let refused = |errors: &Errors, other: &Errors| {
        errors.iter().any(|error| {
            let (place, _) = error;
            !other.contains(error)
                || about(at, place)
                    && (place == at
                        || place.starts_with(&format!("{at}/"))
                        || without
                            .as_ref()
                            .is_some_and(|without| !without.contains(error)))
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

/// Whether an error at `place` is about the value at `at`: at it, inside
/// it or above it.
fn about(at: &str, place: &str) -> bool {
    place == at
        || place.starts_with(&format!("{at}/"))
        || at.starts_with(&format!("{place}/"))
        || place.is_empty()
}

/// The reference's errors for the saved document with `value` at `at`
/// (`None`: without the value, where an object holds it).
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
    let errors = sides
        .both
        .reference
        .iter_errors(&document)
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

/// Whether a string the save kept in the host's type is the text the user
/// typed (which the view reads as a number or boolean), not another
/// spelling of it the host held (`"08"` kept for `8` typed): where the
/// typed document holds a scalar at `at`.
fn saved_as_typed(sides: &Sides<'_>, at: &str, saved: &Value) -> bool {
    let (Some(typed), Value::String(text)) = (sides.typed, saved) else {
        return true;
    };
    match typed.pointer(at) {
        Some(typed) if !typed.is_object() && !typed.is_array() => {
            scalar_text(typed).trim() == text.trim()
        }
        _ => true,
    }
}

/// The scalar of a list of one scalar, through nested lists of one (the
/// view's reading of a lone element the schema wants a list, or a list of
/// lists, of), else `value`.
fn lone_item(value: &Value) -> &Value {
    let mut at = value;
    while let Value::Array(items) = at
        && let [item] = &items[..]
    {
        at = item;
    }
    if at.is_object() || at.is_array() {
        value
    } else {
        at
    }
}

/// Whether `left` and `right` are scalars of two types XML writes as the
/// same text (`"8"` and `8`, `"true"` and `true`, `"null"` and `null`).
fn same_text(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::String(text), other) | (other, Value::String(text)) => match other {
            Value::Number(_) => decimal(text.trim(), false).is_some_and(|read| same(&read, other)),
            Value::Bool(flag) => text.trim() == flag.to_string(),
            Value::Null => text.trim() == "null",
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
    /// an XML host from a JSON view), whose merge took no key as renamed.
    fn class(schema: &Value, into_xml: bool, sides: (Value, Value, Value)) -> Option<&'static str> {
        renamed(schema, into_xml, sides, &[])
    }

    /// [`class`] where the merge took each `(at, key, from)` as a rename:
    /// `key` of the map at `at` holds the host's `from`.
    fn renamed(
        schema: &Value,
        into_xml: bool,
        (before, view, saved): (Value, Value, Value),
        renames: &[(&str, &str, &str)],
    ) -> Option<&'static str> {
        let both = compile(schema).unwrap();
        let (host, edit) = if into_xml {
            (Language::XML, Language::JSON)
        } else {
            (Language::JSON, Language::XML)
        };
        let paired = paired::Paired {
            merged: saved.clone(),
            renames: renames
                .iter()
                .map(|&(at, key, from)| paired::Renamed {
                    at: at.to_string(),
                    key: key.to_string(),
                    from: from.to_string(),
                })
                .collect(),
        };
        explained(&Sides {
            both: &both,
            host,
            edit,
            before: &before,
            view: &view,
            saved: &saved,
            paired: Some(&paired),
            typed: None,
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
        let cut = (json!({"a": [0, 0]}), json!({"b": 0}), json!({"b": [0]}));
        assert_eq!(
            renamed(&any, false, cut.clone(), &[("", "b", "a")]),
            Some(KEPT_LIST)
        );
        // Not where the merge took no rename, nor one the host refutes
        // (a key the host does not hold, or one it already held).
        assert_eq!(class(&any, false, cut.clone()), None);
        assert_eq!(renamed(&any, false, cut.clone(), &[("", "b", "z")]), None);
        let held_both = (
            json!({"a": [0, 0], "b": 1}),
            json!({"b": 0}),
            json!({"b": [0]}),
        );
        assert_eq!(renamed(&any, false, held_both, &[("", "b", "a")]), None);
        // The same through two renames (a key and the one inside it).
        let twice = (
            json!({"name": {"b": [0]}}),
            json!({"x_one": {"port": 0}}),
            json!({"x_one": {"port": [0]}}),
        );
        assert_eq!(
            renamed(
                &any,
                false,
                twice,
                &[("", "x_one", "name"), ("/x_one", "port", "b")]
            ),
            Some(KEPT_LIST)
        );
        // The inner key under the renamed parent is the host's own when
        // the merge kept its name.
        let same_inner = (
            json!({"name": {"port": [0]}}),
            json!({"x_one": {"port": 0}}),
            json!({"x_one": {"port": [0]}}),
        );
        assert_eq!(
            renamed(&any, false, same_inner, &[("", "x_one", "name")]),
            Some(KEPT_LIST)
        );
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
        let text = (
            json!({"name": "true"}),
            json!({"a": true}),
            json!({"a": "true"}),
        );
        assert_eq!(
            renamed(&any, false, text, &[("", "a", "name")]),
            Some(KEPT_TEXT)
        );
        // Each scalar XML reads from text, at its own key and renamed.
        for (read, text) in [
            (json!(8), "8"),
            (json!(true), "true"),
            (json!(null), "null"),
        ] {
            let moved = (
                json!({"b": text}),
                json!({"a": read.clone()}),
                json!({"a": text}),
            );
            assert_eq!(
                renamed(&any, false, moved, &[("", "a", "b")]),
                Some(KEPT_TEXT),
                "{read}"
            );
            let own = (
                json!({"a": text}),
                json!({"a": read.clone(), "c": 1}),
                json!({"a": text, "c": 1}),
            );
            assert_eq!(class(&any, false, own), Some(KEPT_TEXT), "{read}");
            let other = (
                json!({"a": "nil"}),
                json!({"a": read.clone()}),
                json!({"a": "nil"}),
            );
            assert_eq!(class(&any, false, other), None, "{read}");
        }
        let new_key = (json!({"name": 1}), json!({"a": true}), json!({"a": "true"}));
        assert_eq!(renamed(&any, false, new_key, &[("", "a", "name")]), None);
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
                    paired: None,
                    typed: None,
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

    /// A real save from an XML view of a JSON host: the host's document
    /// before, the view's, the save's, and the merge's pairing.
    fn saved_from_xml(before: Value, view: Value) -> (Value, Value, Value, paired::Paired) {
        let both = compile(&json!({})).unwrap();
        let host_text = both.written(Language::JSON, &before).unwrap();
        let typed = both.written(Language::XML, &view).unwrap();
        let bridge = crate::editor::bridge::Bridge::new(
            Language::JSON,
            Language::XML,
            Some(both.editor.bundle.clone()),
        )
        .unwrap();
        paired::take();
        let saved = bridge.to_host(&typed, &host_text).unwrap();
        let paired = paired::take().expect("the save merged the view");
        let saved = both.read(Language::JSON, &saved).unwrap();
        let view = both.read(Language::XML, &typed).unwrap();
        (before, view, saved, paired)
    }

    /// The class the judge gives `saved` as the save of `view` over
    /// `before`, the merge having paired values as `paired` says.
    fn judged(
        before: &Value,
        view: &Value,
        saved: &Value,
        paired: &paired::Paired,
    ) -> Option<&'static str> {
        let both = compile(&json!({})).unwrap();
        explained(&Sides {
            both: &both,
            host: Language::JSON,
            edit: Language::XML,
            before,
            view,
            saved,
            paired: Some(paired),
            typed: None,
        })
    }

    /// The judge takes from the save's record only which host value a
    /// value came from: a save that writes anything but the documented
    /// spelling of that value (another item, another host value's text,
    /// the host's list twice), or a record the host refutes, is caught.
    #[test]
    fn a_save_wrong_beside_its_recorded_source_is_caught() {
        // A list renamed and cut to one: the host's list kept.
        let (before, view, saved, paired) = saved_from_xml(json!({"a": [0, 0]}), json!({"b": 0}));
        assert_eq!(saved, json!({"b": [0]}));
        assert_eq!(judged(&before, &view, &saved, &paired), Some(KEPT_LIST));
        for wrong in [
            json!({"b": [1]}),
            json!({"b": [0, 0]}),
            json!({"b": [0], "a": [0, 0]}),
            json!({"b": [[0]]}),
        ] {
            assert_eq!(judged(&before, &view, &wrong, &paired), None, "{wrong}");
        }
        // A record naming a host key the save still holds, or none the
        // host had, is no source.
        for from in ["b", "z"] {
            let mut told = paired.clone();
            told.renames[0].from = from.to_string();
            assert_eq!(judged(&before, &view, &saved, &told), None, "{from}");
        }

        // The host's text kept under a renamed key.
        let (before, view, saved, paired) = saved_from_xml(
            json!({"name": "true", "c": "false"}),
            json!({"a": true, "c": "false"}),
        );
        assert_eq!(saved, json!({"a": "true", "c": "false"}));
        assert_eq!(judged(&before, &view, &saved, &paired), Some(KEPT_TEXT));
        // Another host value's text there is no spelling of the source.
        let swapped = json!({"a": "false", "c": "false"});
        assert_eq!(judged(&before, &view, &swapped, &paired), None);

        // A number respelled, an element emptied: only the host's number
        // and empty text are their spellings.
        let (before, view, saved, paired) = saved_from_xml(json!({"n": 8}), json!({"n": "08"}));
        assert_eq!(judged(&before, &view, &saved, &paired), Some(RESPELLED));
        assert_eq!(judged(&before, &view, &json!({"n": 9}), &paired), None);
        let (before, view, saved, paired) = saved_from_xml(json!({"m": "a"}), json!({"m": {}}));
        assert_eq!(judged(&before, &view, &saved, &paired), Some(EMPTIED_TEXT));
        assert_eq!(judged(&before, &view, &json!({"m": "a"}), &paired), None);
    }
}
