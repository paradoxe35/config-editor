//! A never-shown document's save reads its text as a shown editor's does:
//! a leading byte order mark is no part of the document (a shown editor's
//! text never holds it), so the gate answers alike with or without one, in
//! every language and whatever characters the document holds, and never
//! panics; `save_all` sends every document the gate lets through, whatever
//! another one in the list holds (QA 32, FW32-1 and FW32-2).

use std::sync::{Arc, Mutex};

use leptos::prelude::*;

use super::entries::{Callbacks, Documents};
use crate::EditorSettings;
use crate::documents::model::Document;
use crate::editor::Language;

const BOM: char = '\u{FEFF}';

/// Characters of each width: one byte, two, three, and four (two UTF-16
/// units).
const CHARACTERS: [&str; 4] = ["a", "é", "設定", "😀"];

/// Where the characters go in a document of `language`: a key, a value,
/// and (XML) an attribute value or element text; each a document that
/// reads.
fn documents(language: Language, characters: &str) -> Vec<(&'static str, String)> {
    let c = characters;
    if language == Language::YAML {
        vec![("key", format!("{c}: 1\n")), ("value", format!("k: {c}\n"))]
    } else if language == Language::JSON {
        vec![
            ("key", format!("{{\"{c}\": 1}}\n")),
            ("value", format!("{{\"k\": \"{c}\"}}\n")),
        ]
    } else if language == Language::TOML {
        vec![
            ("key", format!("\"{c}\" = 1\n")),
            ("value", format!("k = \"{c}\"\n")),
        ]
    } else if language == Language::HCL {
        vec![
            ("key", format!("o = {{ \"{c}\" = 1 }}\n")),
            ("value", format!("k = \"{c}\"\n")),
        ]
    } else {
        vec![
            ("key", format!("<r><{c}>1</{c}></r>\n")),
            ("value", format!("<r k=\"{c}\"/>\n")),
            ("element text", format!("<r>{c}</r>\n")),
            ("text then child", format!("<r>{c}<k>1</k></r>\n")),
        ]
    }
}

/// A document of `language` that does not read.
fn broken(language: Language) -> &'static str {
    [
        (Language::YAML, "a: [1, 2\n"),
        (Language::JSON, "{\"a\": 2,\n"),
        (Language::TOML, "a = \n"),
        (Language::HCL, "a {\n  b = 1\n"),
    ]
    .into_iter()
    .find(|(known, _)| *known == language)
    .map_or("<r><p:x>1</p:x></r>\n", |(_, text)| text)
}

fn file_name(language: Language) -> String {
    language.default_file_name()
}

type Sent = Arc<Mutex<Vec<(String, String)>>>;

/// A view over `list` that records what `on_save` hears.
fn view(list: &[Document]) -> (Documents, Sent) {
    let sent: Sent = Arc::default();
    let on_save = {
        let sent = sent.clone();
        Some(Callback::new(
            move |(id, request): (String, crate::SaveRequest)| {
                sent.lock().unwrap().push((id, request.text));
            },
        ))
    };
    let documents = Documents::new(
        Callbacks {
            on_save,
            ..Callbacks::default()
        },
        Signal::stored(EditorSettings::default()),
    );
    documents.sync(list);
    (documents, sent)
}

/// What a never-shown `x` holding `text` answers to `save` and to
/// `save_all`, and the texts each sent.
fn unshown(language: Language, text: &str) -> (bool, Vec<String>, Vec<(String, String)>) {
    let list = [
        Document::new("first", "first.yaml", "k: 1\n"),
        Document::new("x", file_name(language), text).baseline("changed\n"),
    ];
    let (documents, sent) = view(&list);
    let one = documents.save("x");
    let all = documents.save_all();
    let heard = sent.lock().unwrap().clone();
    (one, all, heard)
}

const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// Every language, every character width in every place, with and without
/// a mark: the never-shown gate answers as the shown one does (which reads
/// the text without the mark), the text sent as given.
#[test]
fn a_byte_order_mark_never_changes_a_never_shown_document_save() {
    let owner = Owner::new();
    owner.set();
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let mut cases: Vec<(String, String, bool)> = CHARACTERS
            .iter()
            .flat_map(|characters| {
                documents(language, characters)
                    .into_iter()
                    .map(move |(place, text)| (format!("{characters:?} in {place}"), text, true))
            })
            .collect();
        cases.push(("broken".to_string(), broken(language).to_string(), false));
        for (case, text, reads) in cases {
            // What a shown editor's gate reads: its text never holds a mark.
            let shown = Language::saves(language, &text);
            if shown != reads {
                failures.push(format!("{language:?} {case}: shown gate says {shown}"));
                continue;
            }
            for marked in [false, true] {
                let given = if marked {
                    format!("{BOM}{text}")
                } else {
                    text.clone()
                };
                let Ok((one, all, heard)) =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        unshown(language, &given)
                    }))
                else {
                    failures.push(format!("{language:?} {case} marked={marked}: panicked"));
                    continue;
                };
                let expected_all: Vec<String> = if shown { vec!["x".into()] } else { vec![] };
                let expected_heard: Vec<(String, String)> = if shown {
                    vec![("x".into(), given.clone()), ("x".into(), given.clone())]
                } else {
                    vec![]
                };
                if one != shown || all != expected_all || heard != expected_heard {
                    failures.push(format!(
                        "{language:?} {case} marked={marked}: save {one}, save_all {all:?}, \
                         heard {heard:?}; shown saves {shown}"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A document the gate refuses (or one holding a mark and a four-byte
/// character, which once panicked the XML reader) in the middle of the
/// list stops neither the documents after it nor the list of ids sent.
#[test]
fn save_all_sends_every_other_document_past_one_it_refuses() {
    let owner = Owner::new();
    owner.set();
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for (case, middle, sends) in [
            ("broken", broken(language).to_string(), false),
            (
                "marked astral",
                format!("{BOM}{}", documents(language, "😀").pop().unwrap().1),
                true,
            ),
        ] {
            let list = [
                Document::new("a", "a.yaml", "k: 2\n").baseline("k: 1\n"),
                Document::new("m", file_name(language), middle.as_str()).baseline("x\n"),
                Document::new("b", "b.toml", "k = 2\n").baseline("k = 1\n"),
            ];
            let (documents, sent) = view(&list);
            let Ok(all) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| documents.save_all()))
            else {
                failures.push(format!("{language:?} {case}: save_all panicked"));
                continue;
            };
            let expected: Vec<&str> = if sends {
                vec!["a", "m", "b"]
            } else {
                vec!["a", "b"]
            };
            let heard: Vec<String> = sent
                .lock()
                .unwrap()
                .iter()
                .map(|(id, _)| id.clone())
                .collect();
            if all != expected || heard != expected {
                failures.push(format!(
                    "{language:?} {case}: save_all {all:?}, heard {heard:?}, expected {expected:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A document of `language` with `entries` values, each different.
fn sized(language: Language, entries: usize) -> String {
    let values = (0..entries).map(|at| format!("é{at}"));
    if language == Language::YAML {
        values.map(|value| format!("k{value}: v\n")).collect()
    } else if language == Language::JSON {
        let body: Vec<String> = values.map(|value| format!("\"{value}\": 1")).collect();
        format!("{{{}}}\n", body.join(",\n"))
    } else if language == Language::TOML {
        values.map(|value| format!("\"{value}\" = 1\n")).collect()
    } else if language == Language::HCL {
        let body: String = values.map(|value| format!("  \"{value}\" = 1\n")).collect();
        format!("o = {{\n{body}}}\n")
    } else {
        let body: String = values.map(|value| format!("<v>{value}</v>")).collect();
        format!("<r>{body}</r>\n")
    }
}

/// The gate reads a marked text once, as it does one without the mark (it
/// is read past, not copied and read again), at two document sizes.
#[test]
fn a_marked_text_is_read_once_by_the_save_gate() {
    use crate::editor::work_count::{Work, counted};
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for entries in [20, 2000] {
            let text = sized(language, entries);
            let marked = format!("{BOM}{text}");
            let (saves, parsed) = counted(Work::ParsedBytes, || Language::saves(language, &marked));
            if !saves || parsed == 0 || parsed > text.len() {
                failures.push(format!(
                    "{language:?} {entries} entries ({} bytes): saves {saves}, read {parsed} bytes",
                    text.len()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
