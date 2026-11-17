//! An item added to or removed from a list its host writes
//! inline (a TOML array, a YAML flow list, an HCL list), on one line or
//! one item per line, changes only that item: the others keep their
//! text (`0x1bb`, `'C:\b'`, `var.a`) and the list its punctuation (a
//! trailing comma, the closing bracket on its own line), from every view.

use crate::editor::save_matrix::{from_every_view, report};
use serde_json::{Value, json};

use super::super::languages::Language;

/// The list at `key` with `edit` made to it.
fn at(key: &'static str, edit: impl Fn(&mut Vec<Value>)) -> impl Fn(&mut Value) {
    move |document: &mut Value| {
        if let Some(Value::Array(items)) = document.get_mut(key) {
            edit(items);
        }
    }
}

/// An edit of the document, boxed.
type Edit = Box<dyn Fn(&mut Value)>;

struct Case {
    host: Language,
    key: &'static str,
    /// The host's list, then the list once each edit is made.
    lists: [&'static str; 5],
    /// What precedes and follows the list's text in the host.
    frame: (&'static str, &'static str),
}

const CASES: &[Case] = &[
    Case {
        host: Language::TOML,
        key: "ports",
        lists: [
            "[0x50, 0x1bb, 0o17]",
            "[0x50, 99, 0x1bb, 0o17]",
            "[99, 0x50, 0x1bb, 0o17]",
            "[0x50, 0x1bb, 0o17, 99]",
            "[0x1bb, 0o17]",
        ],
        frame: ("ports = ", "\nk = 1\n"),
    },
    Case {
        host: Language::TOML,
        key: "ports",
        lists: [
            "[\n  0x50,\n  0x1bb,\n  0o17,\n]",
            "[\n  0x50,\n  99,\n  0x1bb,\n  0o17,\n]",
            "[\n  99,\n  0x50,\n  0x1bb,\n  0o17,\n]",
            "[\n  0x50,\n  0x1bb,\n  0o17,\n  99,\n]",
            "[\n  0x1bb,\n  0o17,\n]",
        ],
        frame: ("ports = ", "\nk = 1\n"),
    },
    Case {
        host: Language::TOML,
        key: "paths",
        lists: [
            "['C:\\a', 'C:\\b', 'C:\\c']",
            "['C:\\a', 99, 'C:\\b', 'C:\\c']",
            "[99, 'C:\\a', 'C:\\b', 'C:\\c']",
            "['C:\\a', 'C:\\b', 'C:\\c', 99]",
            "['C:\\b', 'C:\\c']",
        ],
        frame: ("paths = ", "\nk = 1\n"),
    },
    Case {
        host: Language::YAML,
        key: "ports",
        lists: [
            "[0x50, 0x1BB, 0o17]",
            "[0x50, 99, 0x1BB, 0o17]",
            "[99, 0x50, 0x1BB, 0o17]",
            "[0x50, 0x1BB, 0o17, 99]",
            "[0x1BB, 0o17]",
        ],
        frame: ("ports: ", "\nk: 1\n"),
    },
    Case {
        host: Language::YAML,
        key: "paths",
        lists: [
            "['C:\\a', 'C:\\b', 'C:\\c']",
            "['C:\\a', 99, 'C:\\b', 'C:\\c']",
            "[99, 'C:\\a', 'C:\\b', 'C:\\c']",
            "['C:\\a', 'C:\\b', 'C:\\c', 99]",
            "['C:\\b', 'C:\\c']",
        ],
        frame: ("paths: ", "\nk: 1\n"),
    },
    Case {
        host: Language::HCL,
        key: "names",
        lists: [
            "[\"a\\u00e9\", \"b\", \"c\"]",
            "[\"a\\u00e9\", 99, \"b\", \"c\"]",
            "[99, \"a\\u00e9\", \"b\", \"c\"]",
            "[\"a\\u00e9\", \"b\", \"c\", 99]",
            "[\"b\", \"c\"]",
        ],
        frame: ("names = ", "\nk = 1\n"),
    },
];

#[test]
fn an_item_added_or_removed_leaves_the_other_items_as_written() {
    let mut failures = Vec::new();
    for case in CASES {
        let text = |list: &str| format!("{}{list}{}", case.frame.0, case.frame.1);
        let edits: [(&str, Edit); 4] = [
            (
                "added between",
                Box::new(at(case.key, |items| items.insert(1, json!(99)))),
            ),
            (
                "added first",
                Box::new(at(case.key, |items| items.insert(0, json!(99)))),
            ),
            (
                "added last",
                Box::new(at(case.key, |items| items.push(json!(99)))),
            ),
            (
                "first removed",
                Box::new(at(case.key, |items| {
                    items.remove(0);
                })),
            ),
        ];
        for (index, (label, edit)) in edits.iter().enumerate() {
            from_every_view(
                &mut failures,
                &format!("{label} in {}", case.lists[0]),
                case.host,
                &text(case.lists[0]),
                edit.as_ref(),
                &text(case.lists[index + 1]),
            );
        }
    }
    report(failures);
}
