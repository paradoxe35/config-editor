//! An index is found only on a list: a list written as one, the items of
//! repeated XML elements or HCL blocks, and the only item of an element or
//! block written once (which reads as a one-item list too). Never on a
//! scalar, an object, an attribute, an object of labelled blocks, another
//! index, or the document. `range_of` (and so `reveal`), a host problem's
//! place and a lock on the path agree, in every language.

use super::super::Language;
use super::super::diagnostics::Severity;
use super::super::edit::Edit;
use super::super::host_diagnostics::{HostDiagnostic, RangeText, place};
use super::super::test_state::TestEditor;
use super::FieldPath;
use crate::strings::default_strings;

/// A document in `language`, the pointers to look up in it, and whether
/// each names something (the text of the field it would name otherwise
/// follows: where an edit is made under a lock on it).
fn cases(language: Language) -> (&'static str, Vec<(&'static str, bool, &'static str)>) {
    let common = |prefix: &'static str| -> Vec<(String, bool, &'static str)> {
        [
            ("/name/0", "web"),
            ("/o/0", "k"),
            ("/o/k/0", "1"),
            ("/l/0/0", "x1"),
            ("/l/1/0", "x2"),
            ("/l/0/a/0", "x1"),
        ]
        .into_iter()
        .map(|(pointer, at)| (format!("{prefix}{pointer}"), false, at))
        .chain([
            (format!("{prefix}/l/0"), true, "x1"),
            (format!("{prefix}/l/1/a"), true, "x2"),
            ("/0".to_string(), false, "web"),
        ])
        .collect()
    };
    let leak =
        |cases: Vec<(String, bool, &'static str)>| -> Vec<(&'static str, bool, &'static str)> {
            cases
                .into_iter()
                .map(|(pointer, found, at)| (&*Box::leak(pointer.into_boxed_str()), found, at))
                .collect()
        };
    match language.label() {
        "YAML" => (
            "name: web\no:\n  k: 1\nl:\n  - a: x1\n  - a: x2\none:\n  - a: x3\n",
            leak(common("")),
        ),
        "JSON" => (
            "{\n  \"name\": \"web\",\n  \"o\": {\n    \"k\": 1\n  },\n  \"l\": [\n    {\n      \"a\": \"x1\"\n    },\n    {\n      \"a\": \"x2\"\n    }\n  ]\n}\n",
            leak(common("")),
        ),
        "TOML" => (
            "name = \"web\"\n\n[o]\nk = 1\n\n[[l]]\na = \"x1\"\n\n[[l]]\na = \"x2\"\n",
            leak(common("")),
        ),
        "HCL" => {
            let mut cases = common("");
            cases.extend([
                // A block written once is its one-item list; an object of
                // labelled blocks is no list, nor an index on an item.
                ("/b/0".to_string(), true, "x4"),
                ("/b/0/c".to_string(), true, "x4"),
                ("/b/0/0".to_string(), false, "x4"),
                ("/b/c/0".to_string(), false, "x4"),
                ("/svc/0".to_string(), false, "x5"),
                ("/svc/s/0".to_string(), true, "x5"),
                ("/svc/s/0/0".to_string(), false, "x5"),
                ("/svc/0/s".to_string(), false, "x5"),
            ]);
            (
                "name = \"web\"\no = {\n  k = 1\n}\nl {\n  a = \"x1\"\n}\nl {\n  a = \"x2\"\n}\nb {\n  c = \"x4\"\n}\nsvc \"s\" {\n  c = \"x5\"\n}\n",
                leak(cases),
            )
        }
        "XML" => {
            let mut cases: Vec<(String, bool, &'static str)> = common("/root")
                .into_iter()
                // An element written once is its one-item list.
                .map(|(pointer, found, at)| {
                    let lone = ["/root/name/0", "/root/o/0", "/root/o/k/0", "/root/l/0/a/0"]
                        .contains(&pointer.as_str());
                    (pointer, found || lone, at)
                })
                .collect();
            cases.extend([
                ("/root/0".to_string(), true, "web"),
                ("/root/name/0/0".to_string(), false, "web"),
                ("/root/at/0".to_string(), false, "x6"),
                ("/root/o/0/k".to_string(), true, "1"),
            ]);
            (
                "<root at=\"x6\">\n  <name>web</name>\n  <o>\n    <k>1</k>\n  </o>\n  <l>\n    <a>x1</a>\n  </l>\n  <l>\n    <a>x2</a>\n  </l>\n</root>\n",
                leak(cases),
            )
        }
        other => panic!("no cases for {other}"),
    }
}

/// Whether an edit inside `at` (after its first character) is refused
/// under a lock on `pointer`.
fn locked(language: Language, text: &str, pointer: &str, at: &str) -> bool {
    let mut editor = TestEditor::new(language, text);
    editor.read_only_paths = vec![FieldPath::from_pointer(pointer)];
    let (_owner, state) = editor.mount();
    let byte = text.find(at).expect("the field's text") + 1;
    let offset = text[..byte].encode_utf16().count();
    let mut edited = text.to_string();
    edited.insert(byte, '9');
    !state.commit(Edit::new(
        text.to_string(),
        (offset, offset),
        edited,
        offset + 1,
    ))
}

#[test]
fn an_index_is_found_only_on_a_list_and_every_lookup_agrees() {
    let mut failures = Vec::new();
    for &language in Language::ALL {
        let (text, cases) = cases(language);
        for (pointer, found, at) in cases {
            let case = format!("{language} {pointer}");
            let range = language.range_of(text, pointer);
            if range.is_some() != found {
                failures.push(format!("{case}: range_of {range:?}"));
            }
            let problem = HostDiagnostic::at_path(pointer, Severity::Error, "M");
            let placed = place(
                language.behavior(),
                text,
                &[problem],
                RangeText::OnScreen,
                &[],
                default_strings(),
            );
            let line = placed.first().and_then(|placed| placed.line);
            if line.is_some() != found {
                failures.push(format!("{case}: the host problem is on line {line:?}"));
            }
            if locked(language, text, pointer, at) != found {
                failures.push(format!("{case}: a lock on it locks {at:?}: {}", !found));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
