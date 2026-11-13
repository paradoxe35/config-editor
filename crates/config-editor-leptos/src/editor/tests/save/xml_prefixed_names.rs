//! Keys with a prefix (`x:attr`) renamed or added in another view of an
//! XML host: the model keys elements by their local name, so a prefixed
//! key is an attribute, and a save writes it
//! as one where its prefix is declared, or waits naming the prefix. An
//! element renamed to one waits too: it would become an attribute
//! (hard-bar QA `D23.editing-1`: an attribute renamed `y:attr` was written
//! as a child element, the text moved, and an element renamed `x:child` was
//! dropped, both saves accepted).

use crate::editor::Language;
use crate::editor::save_matrix::{ALL, report, save_value};
use serde_json::Value;

/// `value` with the key `from` of the map at `pointer` renamed `to`, in
/// its place.
fn renamed(value: &mut Value, pointer: &str, from: &str, to: &str) {
    let Some(map) = value.pointer_mut(pointer).and_then(Value::as_object_mut) else {
        return;
    };
    *map = std::mem::take(map)
        .into_iter()
        .map(|(key, held)| (if key == from { to.to_string() } else { key }, held))
        .collect();
}

/// What a save must do: write this text, or wait with a reason holding
/// this.
enum Wanted {
    Saved(&'static str),
    Waits(&'static str),
}

/// A label, the host, the map, the key renamed, its new name, the save.
type Case = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    Wanted,
);

const CASES: [Case; 6] = [
    (
        "an attribute renamed to an undeclared prefix",
        "<r xmlns:x=\"urn:a\"><b x:attr=\"2\">t</b></r>\n",
        "/r/b",
        "x:attr",
        "y:attr",
        Wanted::Waits("the prefix y of y:attr is bound to no namespace"),
    ),
    (
        "an attribute renamed to a declared prefix",
        "<r xmlns:x=\"urn:a\" xmlns:y=\"urn:b\"><b x:attr=\"2\">t</b></r>\n",
        "/r/b",
        "x:attr",
        "y:attr",
        Wanted::Saved("<r xmlns:x=\"urn:a\" xmlns:y=\"urn:b\"><b y:attr=\"2\">t</b></r>\n"),
    ),
    (
        "an attribute over lines renamed to a declared prefix",
        "<r xmlns:x=\"urn:a\" xmlns:y=\"urn:b\">\n  <b x:attr=\"2\">t</b> <!-- c -->\n</r>\n",
        "/r/b",
        "x:attr",
        "y:attr",
        Wanted::Saved(
            "<r xmlns:x=\"urn:a\" xmlns:y=\"urn:b\">\n  <b y:attr=\"2\">t</b> <!-- c -->\n</r>\n",
        ),
    ),
    (
        "an attribute given a prefix",
        "<r xmlns:x=\"urn:a\"><b attr=\"2\">t</b></r>\n",
        "/r/b",
        "attr",
        "x:attr",
        Wanted::Saved("<r xmlns:x=\"urn:a\"><b x:attr=\"2\">t</b></r>\n"),
    ),
    (
        "an element given a prefix: it would be an attribute",
        "<r xmlns:x=\"urn:a\">\n  <child>1</child>\n</r>\n",
        "/r",
        "child",
        "x:child",
        Wanted::Waits("XML keeps a key with a prefix (x:child) only as an attribute"),
    ),
    (
        "an element given a prefix and another name",
        "<r xmlns:x=\"urn:a\">\n  <child>1</child>\n</r>\n",
        "/r",
        "child",
        "x:other",
        Wanted::Waits("XML keeps a key with a prefix (x:other) only as an attribute"),
    ),
];

#[test]
fn a_prefixed_name_is_written_as_an_attribute_where_its_prefix_is_declared() {
    let mut failures = Vec::new();
    for (label, host, pointer, from, to, wanted) in CASES {
        for edit in ALL.into_iter().filter(|edit| *edit != Language::XML) {
            let saved = save_value(Language::XML, edit, host, &|value| {
                renamed(value, pointer, from, to);
            });
            match (&wanted, saved) {
                (Wanted::Saved(text), Ok(saved)) if saved == *text => {}
                (Wanted::Waits(reason), Err(error)) if error.contains(reason) => {}
                // HCL names no attribute of a block `x:child`.
                (Wanted::Waits(_), Err(error))
                    if edit == Language::HCL && error.contains("is not an HCL identifier") => {}
                (_, other) => failures.push(format!("{label}: via {edit}: {other:?}")),
            }
        }
    }
    report(failures);
}
