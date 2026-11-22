//! The judge's side of the documented ways a save with XML on one side
//! reads back otherwise than the view: the list itself is the save's own
//! (`bridge::spellings`), asked here with the reference validator telling
//! what the schema makes of a value, so the save and the judge allow the
//! same spellings and the judge's schema answers stay its own. A failing
//! save is in one of these classes only when each value it reads
//! otherwise is one of them, so an allowance (see `known`) covers that
//! value and no other difference.

use std::collections::HashSet;

use serde_json::Value;

use super::reference::Both;
use crate::editor::Language;
use crate::editor::applies::scalar_text;
use crate::editor::bridge::paired;
use crate::editor::bridge::spellings::{self, Refused};
pub(super) use crate::editor::bridge::spellings::{
    CUT_TO_ONE, EMPTIED_TEXT, KEPT_LIST, KEPT_TYPE, RESPELLED,
};
use crate::editor::language::unescaped_key;

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
    /// Which host value the save took each value from (see
    /// `spellings::held`); `None`: each at its own pointer.
    pub(super) paired: Option<&'a paired::Paired>,
    /// The document the view's text was written from (what the user
    /// typed, each scalar as its text); `None`: not known.
    pub(super) typed: Option<&'a Value>,
}

/// The class of the save when XML's spelling explains each value it reads
/// otherwise than the view: the first such value's.
pub(super) fn explained(sides: &Sides<'_>) -> Option<&'static str> {
    // An item of a list the view made of a host scalar (its element
    // written again) was taken from that scalar: the save keeps its host
    // type unless the schema there prefers the view's reading, as for a
    // scalar left where it was.
    let held = |at: &str| {
        spellings::held(sides.before, sides.paired, sides.saved, at).or_else(|| {
            let (list, index) = at.rsplit_once('/')?;
            index.parse::<usize>().ok()?;
            spellings::held(sides.before, sides.paired, sides.saved, list)
                .filter(|scalar| !scalar.is_object() && !scalar.is_array())
        })
    };
    let typed = |at: &str| {
        sides
            .typed?
            .pointer(at)
            .filter(|typed| !typed.is_object() && !typed.is_array())
            .map(scalar_text)
    };
    let refused = |at: &str, view: &Value| refused(sides, at, view);
    spellings::explained(
        &spellings::Sides {
            from_xml: sides.edit == Language::XML,
            into_xml: sides.host == Language::XML,
            view_repeats: sides.edit.behavior().repetition(),
            host_repeats: sides.host.behavior().repetition(),
            own_form: &|_| None,
            view: sides.view,
            saved: sides.saved,
            held: &held,
            typed: &typed,
            refused: &refused,
        },
        &|_| false,
    )
    .ok()
    .flatten()
}

/// What the reference makes of the view's value at `at` in the saved
/// document in place of the saved value. A reading is refused by an error
/// at the value (or inside it), by one anywhere that the other reading
/// does not have (the two documents differ only in the value: an `if`
/// beside it reading it), or by one above it (options of an object holding
/// it) that goes with the value.
fn refused(sides: &Sides<'_>, at: &str, view: &Value) -> Refused {
    let Some(saved) = sides.saved.pointer(at) else {
        return Refused::default();
    };
    let as_viewed = errors_about(sides, at, Some(view)).unwrap_or_default();
    let as_saved = errors_about(sides, at, Some(saved)).unwrap_or_default();
    let without = errors_about(sides, at, None);
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
    Refused {
        view: refused(&as_viewed, &as_saved),
        saved: refused(&as_saved, &as_viewed),
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
            moved: Vec::new(),
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
        assert_eq!(typed(json!({"type": "string"})), Some(KEPT_TYPE));
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
            Some(KEPT_TYPE)
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
                Some(KEPT_TYPE),
                "{read}"
            );
            let own = (
                json!({"a": text}),
                json!({"a": read.clone(), "c": 1}),
                json!({"a": text, "c": 1}),
            );
            assert_eq!(class(&any, false, own), Some(KEPT_TYPE), "{read}");
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
        assert_eq!(judged(&before, &view, &saved, &paired), Some(KEPT_TYPE));
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
