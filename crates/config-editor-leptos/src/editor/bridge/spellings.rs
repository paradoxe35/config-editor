//! The one written list of ways a save may read back otherwise than the
//! view, with XML on one side. XML writes every scalar as text and a list
//! of one as its item, so one XML spelling stands for two values of the
//! other side (`8` and `"8"`, `[x]` and `x`). The save's last check
//! (`read_back`) and the random suites' judge both ask this list, value by
//! value: a value it does not name makes the save wait.

use serde_json::Value;

use crate::editor::decimal_text::decimal;
use crate::editor::language::{Repetition, pointer_key};

/// An XML view shows a list of one as its item where no schema says it is
/// a list (and a list cut to one item looks the same): the save keeps the
/// host's list there, its item told as any other value.
pub(crate) const KEPT_LIST: &str = "a save keeps the host's list where the XML view shows one item";
/// A list cut to one item in a view of an XML host is written as one
/// element, which the host reads as that item.
pub(crate) const CUT_TO_ONE: &str =
    "a save writes a list cut to one item as that item in an XML host";
/// An XML element emptied where the host holds text saves as the empty
/// text, where the view reads the empty element as an empty object; and
/// the host's `null`, which the view shows as an empty element, stays.
pub(crate) const EMPTIED_TEXT: &str =
    "a save writes an emptied XML element as the host's empty text";
/// A list holding lists, which an XML view shows flattened (XML writes no
/// list in a list), saved in the host's nesting with the view's items.
pub(crate) const NESTED_LISTS: &str =
    "a save keeps the host's nested lists where the XML view shows them flattened";
/// A changed scalar the host writes in its own form (a TOML datetime read
/// back normalized, `.50` as `.5`): the host's own reading of the typed
/// text (`SaveLayout::scalar_edit`).
pub(crate) const HOST_FORM: &str = "a save writes a changed scalar in the host's own form";
/// A number typed in another decimal spelling (`08`, `2.50`) on the XML
/// side: an XML view's text saved as the host's number, or a number typed
/// so in a view of an XML host saved as that text.
pub(crate) const RESPELLED: &str =
    "a save keeps a number typed in another spelling as its XML side writes it";
/// XML text the view reads as one type (`12`) saved in the host's type of
/// the same text (`"12"`), unless the schema takes the view's reading and
/// refuses the host's type.
pub(crate) const KEPT_TYPE: &str =
    "a save keeps the host's type of XML text the view reads as another type";

/// What the schema makes of the view's value at a place, put in the saved
/// document instead of the saved value: which of the two a part of the
/// schemas refuses where it takes the other (a part refusing both tells
/// nothing). No schema refuses nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Refused {
    pub(crate) view: bool,
    pub(crate) saved: bool,
}

/// A save and what it is compared with.
pub(crate) struct Sides<'a> {
    /// The view writes every scalar as text (XML).
    pub(crate) from_xml: bool,
    /// The host does.
    pub(crate) into_xml: bool,
    /// How the view writes a list by repeating its key (XML elements, HCL
    /// blocks), so a list of one shows as its item.
    pub(crate) view_repeats: Repetition,
    /// How the host does.
    pub(crate) host_repeats: Repetition,
    /// The host's own form of a changed scalar the view typed otherwise
    /// (a TOML datetime normalized), at a pointer; `None`: none.
    pub(crate) own_form: &'a dyn Fn(&str) -> Option<&'a Value>,
    /// The view's document, as its reader reads it.
    pub(crate) view: &'a Value,
    /// The host's document read back from the save.
    pub(crate) saved: &'a Value,
    /// The host's value before the save that the saved value at a pointer
    /// was taken from (see [`held`]).
    pub(crate) held: &'a dyn Fn(&str) -> Option<&'a Value>,
    /// The text the user typed for the scalar at a pointer of the view
    /// (`None`: not known).
    pub(crate) typed: &'a dyn Fn(&str) -> Option<String>,
    /// What the schema makes of the view's value at a pointer.
    pub(crate) refused: &'a dyn Fn(&str, &Value) -> Refused,
}

/// How the save reads against the view: `Ok(None)` where it reads as the
/// view, `Ok(Some(class))` where the list (or `left`) explains each value
/// it reads otherwise (the class of the first the list explains; `""`
/// when `left` explains them all), else `Err` with the first pointer it
/// does not explain. `left`: the values at the pointers it takes, which
/// the user left as the view showed them, keep the host's value.
pub(crate) fn explained(
    sides: &Sides<'_>,
    left: &dyn Fn(&str) -> bool,
) -> Result<Option<&'static str>, String> {
    let mut first = None;
    for at in differing(sides.saved, sides.view) {
        if left(&at) {
            first.get_or_insert("");
            continue;
        }
        match sides
            .view
            .pointer(&at)
            .and_then(|view| spelling(sides, &at, view, false))
        {
            Some(class) => {
                if first.is_none_or(str::is_empty) {
                    first = Some(class);
                }
            }
            None => return Err(at),
        }
    }
    Ok(first)
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
    differing(sides.saved.pointer(at)?, view)
        .iter()
        .map(|inner| spelling(sides, &format!("{at}{inner}"), view.pointer(inner)?, listed))
        .collect()
}

/// The pointers at which `saved` and `view` differ, each the outermost
/// where they differ in kind, keys or length.
pub(crate) fn differing(saved: &Value, view: &Value) -> Vec<String> {
    let mut out = Vec::new();
    differing_at(saved, view, &mut String::new(), &mut out);
    out
}

fn differing_at(saved: &Value, view: &Value, at: &mut String, out: &mut Vec<String>) {
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
                    (Some(saved), Some(view)) => differing_at(saved, view, at, out),
                    _ => out.push(at.clone()),
                }
                at.truncate(length);
            }
        }
        (Value::Array(saved), Value::Array(view)) if saved.len() == view.len() => {
            for (index, (saved, view)) in saved.iter().zip(view).enumerate() {
                at.push('/');
                at.push_str(&index.to_string());
                differing_at(saved, view, at, out);
                at.truncate(length);
            }
        }
        _ if same(saved, view) => {}
        _ => out.push(at.clone()),
    }
}

/// Whether two values are the same, a number by its value whatever its
/// form (`2` and `2.0`), an integer exactly.
pub(crate) fn same(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(left), Value::Number(right)) => match (left.as_i128(), right.as_i128()) {
            (Some(left), Some(right)) => left == right,
            _ => left.as_f64() == right.as_f64(),
        },
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len() && left.iter().zip(right).all(|(a, b)| same(a, b))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, a)| right.get(key).is_some_and(|b| same(a, b)))
        }
        _ => left == right,
    }
}

/// Which spelling explains the saved value at `at`, which the view holds
/// as `view`, if one does.
fn spelling(sides: &Sides<'_>, at: &str, view: &Value, listed: bool) -> Option<&'static str> {
    let saved = sides.saved.pointer(at)?;
    let before = (sides.held)(at);
    let (into_xml, from_xml) = (sides.into_xml, sides.from_xml);
    match (before, view, saved) {
        (_, _, _) if (sides.own_form)(at).is_some_and(|own| same(own, saved)) => Some(HOST_FORM),
        (Some(Value::Array(_)), _, Value::Array(items))
            if items.len() == 1 && !view.is_array() && repeats(sides.view_repeats, view) =>
        {
            classes_at(sides, &format!("{at}/0"), view, true).map(|_| KEPT_LIST)
        }
        (Some(Value::Array(_)), Value::Array(items), _)
            if repeats(sides.host_repeats, saved)
                && matches!(&items[..], [only] if same(only, saved)) =>
        {
            Some(CUT_TO_ONE)
        }
        (Some(Value::String(_)), Value::Object(map), Value::String(text))
            if from_xml && map.is_empty() && text.is_empty() =>
        {
            Some(EMPTIED_TEXT)
        }
        (Some(Value::Null), Value::Object(map), Value::Null) if from_xml && map.is_empty() => {
            Some(EMPTIED_TEXT)
        }
        (Some(Value::Array(held)), Value::Array(_), Value::Array(_))
            if sides.view_repeats == Repetition::Elements
                && held.iter().any(Value::is_array)
                && shown_alike(&flattened(saved), view) =>
        {
            Some(NESTED_LISTS)
        }
        (
            Some(Value::Number(_) | Value::Bool(_)),
            Value::String(text),
            Value::Number(_) | Value::Bool(_),
        ) if from_xml && kind(before.unwrap_or(saved)) == kind(saved) && respelled(text, saved) => {
            Some(RESPELLED)
        }
        // The text typed is the host's, the number it spells the view's:
        // only where the schema tells the two apart nowhere.
        (_, Value::Number(_), Value::String(text))
            if into_xml
                && decimal_spelling(text, view)
                && (sides.refused)(at, view) == Refused::default() =>
        {
            Some(RESPELLED)
        }
        // An item no schema reads keeps the host's type, and so does a
        // host scalar the view reads as a list's one item.
        (Some(before), _, _)
            if from_xml
                && kind(before) == kind(saved)
                && same_text(lone_item(view), saved)
                && saved_as_typed(sides, at, saved) =>
        {
            if listed {
                Some(KEPT_LIST)
            } else {
                let refused = (sides.refused)(at, view);
                (refused.view || !refused.saved).then_some(KEPT_TYPE)
            }
        }
        _ => None,
    }
}

/// The host's value before the save that the saved value at `at` was
/// taken from: the host's value at the pointer the merge's pairing names.
///
/// Which host value that is, is the merge's own record (`paired`): a
/// view typed whole carries no links, so only the merge knows which host
/// key a key it renamed stands for, or which host scalar a scalar left as
/// the editor wrote it at another index is, and every other value stays
/// at its host pointer. Only that identity is taken from the record, and only
/// where the host agrees it can be one (a renamed key's host key is in
/// the host's map there, its new name is not, and the merged map no longer
/// holds the old one); what the save wrote for the value is still judged
/// against the host's value and the view's. So a wrong pairing can at
/// worst compare a value with another of the host's values the view
/// renamed away, never excuse a value the host did not hold in that form.
pub(crate) fn held<'a>(
    before: &'a Value,
    paired: Option<&super::merge::Paired>,
    saved: &Value,
    at: &str,
) -> Option<&'a Value> {
    before.pointer(&held_at(before, paired, saved, at))
}

/// The pointer of the host's value [`held`] names.
pub(crate) fn held_at(
    before: &Value,
    paired: Option<&super::merge::Paired>,
    saved: &Value,
    at: &str,
) -> String {
    let Some(paired) = paired else {
        return at.to_string();
    };
    // The saved value's place in the merged value: the host may read a
    // list of one the merge wrote as its item (an XML host), the walk of
    // the two pairs them (see `HostPaths`).
    let (merged, _) = super::links::HostPaths::between(&paired.merged, saved).item(at);
    // A host scalar the save put at another index (an item typed before
    // it) is the host's value it was taken from, where the host holds a
    // scalar there.
    if let Some(moved) = paired.moved.iter().find(|moved| moved.at == merged)
        && before
            .pointer(&moved.from)
            .is_some_and(|held| !held.is_object() && !held.is_array())
    {
        return moved.from.clone();
    }
    let (mut placed, mut host) = (String::new(), String::new());
    for segment in merged.split('/').skip(1) {
        let renamed = paired.renames.iter().find(|renamed| {
            renamed.at == placed
                && pointer_key(&renamed.key) == segment
                && renames_one(before.pointer(&host), &paired.merged, renamed)
        });
        host.push('/');
        match renamed {
            Some(renamed) => host.push_str(&pointer_key(&renamed.from)),
            None => host.push_str(segment),
        }
        placed.push('/');
        placed.push_str(segment);
    }
    host
}

/// Whether the host's map `host` and the merged value agree that the
/// merge's `renamed` can be a rename: the host held the old key and not
/// the new one, and the merged map holds the new one in its place.
fn renames_one(host: Option<&Value>, merged: &Value, renamed: &super::merge::Renamed) -> bool {
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

/// Whether `text` spells the number or boolean `value` otherwise than its
/// plain form: in decimal (`08` for 8, `2.50` for 2.5), an integer in
/// base 16, 8 or 2 (`0x50`), a boolean in any case (`False`).
fn respelled(text: &str, value: &Value) -> bool {
    let text = text.trim();
    let spells = match value {
        Value::Bool(flag) => text.eq_ignore_ascii_case(&flag.to_string()),
        _ => {
            decimal_spelling(text, value)
                || radix_integer(text).is_some_and(|integer| same(&Value::from(integer), value))
        }
    };
    spells && text != scalar_plain(value)
}

/// A scalar as JSON writes it.
fn scalar_plain(value: &Value) -> String {
    value.to_string()
}

/// `value`, a list, with each list in it spliced in its place, deeply.
fn flattened(value: &Value) -> Value {
    fn into(value: &Value, out: &mut Vec<Value>) {
        match value {
            Value::Array(items) => items.iter().for_each(|item| into(item, out)),
            other => out.push(other.clone()),
        }
    }
    let mut out = Vec::new();
    into(value, &mut out);
    Value::Array(out)
}

/// Whether `saved` shows in an XML view as `view`: alike, a list of one
/// shown as its item (no schema saying it is a list), at any depth.
fn shown_alike(saved: &Value, view: &Value) -> bool {
    match (saved, view) {
        (Value::Array(items), _) if items.len() == 1 && !view.is_array() => {
            shown_alike(&items[0], view)
        }
        (Value::Array(saved), Value::Array(view)) => {
            saved.len() == view.len() && saved.iter().zip(view).all(|(a, b)| shown_alike(a, b))
        }
        (Value::Object(saved), Value::Object(view)) => {
            saved.len() == view.len()
                && saved
                    .iter()
                    .all(|(key, a)| view.get(key).is_some_and(|b| shown_alike(a, b)))
        }
        _ => same(saved, view),
    }
}

/// Whether a language repeating keys as `repetition` writes a list of
/// `item` by repeating its key, so a list of one item reads as that item.
fn repeats(repetition: Repetition, item: &Value) -> bool {
    match repetition {
        Repetition::Elements => true,
        Repetition::Blocks => item.is_object(),
        Repetition::None => false,
    }
}

/// The integer `text` writes in base 16, 8 or 2 (`0x50`, `-0o17`, `0b11`).
pub(crate) fn radix_integer(text: &str) -> Option<i64> {
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text.strip_prefix('+').unwrap_or(text)),
    };
    let (radix, digits) = match digits.get(..2)? {
        "0x" | "0X" => (16, digits.get(2..)?),
        "0o" | "0O" => (8, digits.get(2..)?),
        "0b" | "0B" => (2, digits.get(2..)?),
        _ => return None,
    };
    if digits.is_empty() || digits.starts_with(['+', '-']) {
        return None;
    }
    let magnitude = i64::from_str_radix(digits, radix).ok()?;
    Some(if negative { -magnitude } else { magnitude })
}

/// Whether `text` is `number` written in decimal, in any spelling.
fn decimal_spelling(text: &str, number: &Value) -> bool {
    decimal(text.trim(), false).is_some_and(|read| same(&read, number))
}

fn kind(value: &Value) -> u8 {
    match value {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        Value::Array(_) => 4,
        Value::Object(_) => 5,
    }
}

/// Whether a string the save kept in the host's type is the text the user
/// typed (which the view reads as a number or boolean), not another
/// spelling of it the host held (`"08"` kept for `8` typed), where the
/// typed text is known.
fn saved_as_typed(sides: &Sides<'_>, at: &str, saved: &Value) -> bool {
    let Value::String(text) = saved else {
        return true;
    };
    (sides.typed)(at).is_none_or(|typed| typed.trim() == text.trim())
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
    use super::*;
    use serde_json::json;

    /// The class the list gives a save from a view of a typed host
    /// (`from_xml`: an XML view; else a view of an XML host) where the host
    /// held `before`, the view reads `view` and the save reads `saved`;
    /// `own`: the host's own form of the one changed value; `refused`: what
    /// the schema makes of the view's value.
    fn class(
        from_xml: bool,
        (before, view, saved): (Value, Value, Value),
        own: Option<Value>,
        refused: Refused,
    ) -> Result<Option<&'static str>, String> {
        let held = |at: &str| before.pointer(at);
        let own_form = |_: &str| own.as_ref();
        let refused = |_: &str, _: &Value| refused;
        let (xml, typed) = (Repetition::Elements, Repetition::None);
        explained(
            &Sides {
                from_xml,
                into_xml: !from_xml,
                view: &view,
                saved: &saved,
                view_repeats: if from_xml { xml } else { typed },
                host_repeats: if from_xml { typed } else { xml },
                held: &held,
                typed: &|_| None,
                own_form: &own_form,
                refused: &refused,
            },
            &|_| false,
        )
    }

    fn xml(sides: (Value, Value, Value)) -> Option<&'static str> {
        class(true, sides, None, Refused::default()).ok().flatten()
    }

    /// Each spelling explains its one shape, and the same shape a step off
    /// (another value, another kind, no XML side) is not explained.
    #[test]
    fn each_spelling_explains_its_shape_and_no_neighbour() {
        // Respelled numbers and booleans, from an XML view's text.
        for (text, saved) in [
            ("08", json!(8)),
            ("2.50", json!(2.5)),
            ("0x50", json!(80)),
            ("-0o17", json!(-15)),
            ("False", json!(false)),
        ] {
            let sides = (json!({"a": saved}), json!({"a": text}), json!({"a": saved}));
            assert_eq!(xml(sides), Some(RESPELLED), "{text}");
            let other = (json!({"a": saved}), json!({"a": text}), json!({"a": 7}));
            assert_eq!(xml(other), None, "{text}");
        }
        // A number typed `4.10` over an XML host, kept as typed: only where
        // the schema tells the two apart nowhere.
        let typed = (json!({"a": 4.1}), json!({"a": 4.1}), json!({"a": "4.10"}));
        let into = |refused| class(false, typed.clone(), None, refused).ok().flatten();
        assert_eq!(into(Refused::default()), Some(RESPELLED));
        let refused = Refused {
            view: true,
            saved: false,
        };
        assert_eq!(into(refused), None);
        // The host's own form of a changed value (a TOML datetime).
        let datetime = (
            json!({"f": "1979-05-27T00:32:00Z"}),
            json!({"f": "1979-05-27T00:32:00.50Z"}),
            json!({"f": "1979-05-27T00:32:00.5Z"}),
        );
        let own = Some(json!("1979-05-27T00:32:00.5Z"));
        let refused = Refused::default();
        assert_eq!(
            class(false, datetime.clone(), own, refused),
            Ok(Some(HOST_FORM))
        );
        assert!(class(false, datetime, None, refused).is_err());
        // An emptied element over the host's text, and the host's null.
        let emptied = (json!({"m": "a"}), json!({"m": {}}), json!({"m": ""}));
        assert_eq!(xml(emptied), Some(EMPTIED_TEXT));
        let null = (json!({"m": null}), json!({"m": {}}), json!({"m": null}));
        assert_eq!(xml(null), Some(EMPTIED_TEXT));
        let other = (json!({"m": 1}), json!({"m": {}}), json!({"m": null}));
        assert_eq!(xml(other), None);
        // Nested lists the view shows flattened, a list of one inside an
        // item shown as its item.
        let nested = (
            json!({"l": [["a", "b"], "c"]}),
            json!({"l": ["a", "B", "c", {"k": 1}]}),
            json!({"l": [["a", "B"], "c", [{"k": 1}]]}),
        );
        assert_eq!(xml(nested), Some(NESTED_LISTS));
        let reordered = (
            json!({"l": [["a", "b"], "c"]}),
            json!({"l": ["a", "b", "c"]}),
            json!({"l": [["b", "a"], "c"]}),
        );
        assert_eq!(xml(reordered), None);
        // The same text in the host's type, unless the schema prefers the
        // view's reading.
        let text = (json!({"a": "1"}), json!({"a": 12}), json!({"a": "12"}));
        assert_eq!(xml(text.clone()), Some(KEPT_TYPE));
        let prefers_view = Refused {
            view: false,
            saved: true,
        };
        assert!(class(true, text.clone(), None, prefers_view).is_err());
        // None of the XML spellings between two typed languages' values.
        let (before, view, saved) = text;
        let held = |at: &str| before.pointer(at);
        let typed_sides = Sides {
            from_xml: false,
            into_xml: false,
            view: &view,
            saved: &saved,
            view_repeats: Repetition::None,
            host_repeats: Repetition::None,
            held: &held,
            typed: &|_| None,
            own_form: &|_| None,
            refused: &|_, _| Refused::default(),
        };
        assert!(explained(&typed_sides, &|_| false).is_err());
    }
}
