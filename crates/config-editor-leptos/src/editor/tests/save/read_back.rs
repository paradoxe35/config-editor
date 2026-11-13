//! A save is the host's text only when it reads back as the view holds the
//! document, keys in the order the user moved them to; otherwise it waits
//! with a problem on the value (hard-bar QA `D25.editing-1`: an XML
//! attribute moved below the element's text, or two attributes swapped,
//! showed the move in the view while the host stayed as it was, and the
//! save was accepted).

use crate::editor::Language;
use crate::editor::codec;
use crate::editor::save_matrix::{ALL, report, save_value};
use serde_json::Value;

/// `value` with the keys of the map at `pointer` in `order` (keys left out
/// keep their place after them).
fn ordered(value: &mut Value, pointer: &str, order: &[&str]) {
    let Some(map) = value.pointer_mut(pointer).and_then(Value::as_object_mut) else {
        return;
    };
    let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
    entries.sort_by_key(|(key, _)| {
        order
            .iter()
            .position(|other| other == key)
            .unwrap_or(order.len())
    });
    map.extend(entries);
}

/// A label, the host and its text, the map and the order the view puts
/// its keys in.
type Case = (
    &'static str,
    Language,
    &'static str,
    &'static str,
    &'static [&'static str],
);

const CASES: [Case; 6] = [
    (
        "an attribute moved below the element's text",
        Language::XML,
        "<r xmlns:x=\"urn:a\" a=\"1\"><s x:attr=\"3\">t</s><u>v</u></r>\n",
        "/r/s",
        &["#text", "x:attr"],
    ),
    (
        "an attribute moved below the element's text, over lines",
        Language::XML,
        "<r>\n  <s k=\"3\">t</s>\n  <u>v</u>\n</r>\n",
        "/r/s",
        &["#text", "k"],
    ),
    (
        "two attributes swapped",
        Language::XML,
        "<r xmlns:n=\"urn:x\" n:a=\"1\" b=\"2\"><s>v</s></r>\n",
        "/r",
        &["xmlns:n", "b", "n:a"],
    ),
    (
        "an attribute moved below a child element",
        Language::XML,
        "<r>\n  <c a=\"1\">\n    <s>v</s>\n  </c>\n</r>\n",
        "/r/c",
        &["s", "a"],
    ),
    (
        "a key moved below a table",
        Language::TOML,
        "a = 1\n\n[t]\nb = 2\n",
        "",
        &["t", "a"],
    ),
    (
        "a key moved below a nested table",
        Language::TOML,
        "[s]\na = 1\n\n[s.t]\nb = 2\n",
        "/s",
        &["t", "a"],
    ),
];

#[test]
fn a_move_the_host_cannot_write_waits_and_one_it_can_is_saved() {
    let mut failures = Vec::new();
    for (label, host, text, pointer, order) in CASES {
        let mut wanted = host.parse(text).expect("the host parses");
        ordered(&mut wanted, pointer, order);
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            let shown = |value: &mut Value| ordered(value, pointer, order);
            match save_value(host, edit, text, &shown) {
                Ok(saved) => {
                    let read = codec::read(host.behavior(), &saved, None).expect("reads");
                    if read != wanted || !codec::same_order(&read, &wanted) {
                        failures.push(format!(
                            "{label}: via {edit}: saved what reads back otherwise:\n{saved}"
                        ));
                    }
                }
                // A view that cannot show the document, or a save that
                // waits: on a value.
                Err(error) if error.contains("(at /") || error.contains("pointer: ") => {}
                Err(error) => failures.push(format!("{label}: via {edit}: {error}")),
            }
        }
    }
    report(failures);
}

/// The waiting save names the key moved, and says why.
#[test]
fn a_move_that_waits_names_the_key_and_the_reason() {
    let text = "<r xmlns:x=\"urn:a\" a=\"1\"><s x:attr=\"3\">t</s><u>v</u></r>\n";
    for edit in [Language::YAML, Language::JSON, Language::TOML] {
        let saved = save_value(Language::XML, edit, text, &|value| {
            ordered(value, "/r/s", &["#text", "x:attr"]);
        });
        let error = saved.expect_err("the move waits");
        assert!(
            error.contains("another place than the view moved it to")
                && error.contains("(at /r/s/#text)"),
            "via {edit}: {error}"
        );
    }
}

/// Tables a view puts in another order move in a TOML host, each with its
/// comments and the sub-tables written after it (they were saved as the
/// host was, the order dropped).
#[test]
fn tables_reordered_in_a_view_move_in_a_toml_host() {
    let mut failures = Vec::new();
    let host = "# head\n\n# about a\n[a]\nx = 1 # one\n\n[a.s]\nz = 1\n\n[b]\ny = 2\n";
    let expected = "# head\n\n[b]\ny = 2\n\n# about a\n[a]\nx = 1 # one\n\n[a.s]\nz = 1\n";
    for edit in ALL.into_iter().filter(|edit| *edit != Language::TOML) {
        match save_value(Language::TOML, edit, host, &|value| {
            ordered(value, "", &["b", "a"])
        }) {
            Ok(saved) if saved == expected => {}
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    report(failures);
}
