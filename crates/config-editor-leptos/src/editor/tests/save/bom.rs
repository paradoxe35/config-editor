//! A leading byte order mark is no part of the document: a save from
//! another view keeps it (places in a marked text are read at the public
//! entry points: `tests/byte_order_mark_tools.rs`).

use crate::editor::Language;
use crate::editor::save_matrix::{ALL, report, save_value, slot};
use serde_json::{Value, json};

const BOM: char = '\u{FEFF}';

/// One small document per language, its first key commented.
fn sample(language: Language) -> &'static str {
    [
        (Language::YAML, "a: 1 # c\nb:\n  c: 2\n"),
        (
            Language::JSON,
            "{\n  \"a\": 1,\n  \"b\": {\n    \"c\": 2\n  }\n}\n",
        ),
        (Language::TOML, "a = 1 # c\n\n[b]\nc = 2\n"),
        (Language::HCL, "a = 1 # c\n\nb {\n  c = 2\n}\n"),
    ]
    .into_iter()
    .find(|(known, _)| *known == language)
    .map_or(
        "<r>\n  <a>1</a> <!-- c -->\n  <b>\n    <c>2</c>\n  </b>\n</r>\n",
        |(_, text)| text,
    )
}

fn first_key(language: Language) -> &'static str {
    if language == Language::XML {
        "/r/a"
    } else {
        "/a"
    }
}

type Change = fn(&mut Value, &str);

fn edit(value: &mut Value, pointer: &str) {
    *slot(value, pointer) = json!(10);
}

fn parent<'a>(value: &'a mut Value, pointer: &str) -> &'a mut serde_json::Map<String, Value> {
    let (parent, _) = pointer.rsplit_once('/').unwrap();
    let parent = if parent.is_empty() {
        value
    } else {
        slot(value, parent)
    };
    parent.as_object_mut().unwrap()
}

fn remove(value: &mut Value, pointer: &str) {
    let key = pointer.rsplit('/').next().unwrap();
    parent(value, pointer).shift_remove(key);
}

fn rename(value: &mut Value, pointer: &str) {
    let key = pointer.rsplit('/').next().unwrap();
    let object = parent(value, pointer);
    *object = std::mem::take(object)
        .into_iter()
        .map(|(name, held)| (if name == key { "z".to_string() } else { name }, held))
        .collect();
}

fn add_before(value: &mut Value, pointer: &str) {
    parent(value, pointer).shift_insert(0, "n".to_string(), json!(5));
}

/// The first key edited, renamed, removed, or a key added before it, from
/// every other view, saves as it does without the mark, the mark kept.
#[test]
fn a_save_from_another_view_keeps_a_byte_order_mark_and_the_comments() {
    let mut failures = Vec::new();
    let changes: [(&str, Change); 4] = [
        ("edit", edit),
        ("remove", remove),
        ("rename", rename),
        ("add", add_before),
    ];
    for host in ALL {
        let plain = sample(host);
        let marked = format!("{BOM}{plain}");
        for (label, change) in changes {
            let change = |value: &mut Value| change(value, first_key(host));
            for view in ALL.into_iter().filter(|view| *view != host) {
                let expected =
                    save_value(host, view, plain, &change).map(|saved| format!("{BOM}{saved}"));
                let saved = save_value(host, view, &marked, &change);
                if expected.is_err() || saved != expected {
                    failures.push(format!(
                        "{label} {host} via {view}:\n--- expected\n{expected:?}\n--- saved\n{saved:?}"
                    ));
                }
            }
        }
    }
    report(failures);
}
