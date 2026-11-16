//! A lock costs a keystroke away from it nothing, in every language and
//! every Edit-as view: typing in a value far from the one locked field
//! parses no more of the document, and reads no more of its spans, than
//! with nothing locked (it read the whole document once per keystroke); the
//! path of the line typed on is read once for all the keys typed there; and
//! the lock is then where placing it afresh puts it.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted};
use super::locks::{Sources, protect};

/// `blocks` services, each with an image, a port and a comment where the
/// language has one; YAML's merge the defaults they share.
pub(super) fn services(language: Language, blocks: usize) -> String {
    let each = |index: usize| match language.label() {
        "YAML" => format!(
            "  # service {index}\n  svc_{index}:\n    <<: *defaults\n    image: \"app-{index}:1.0\"\n    port: {index}\n"
        ),
        "JSON" => format!(
            "    \"svc_{index}\": {{\n      \"image\": \"app-{index}:1.0\",\n      \"port\": {index}\n    }}{}\n",
            if index + 1 == blocks { "" } else { "," }
        ),
        "TOML" => format!(
            "# service {index}\n[services.svc_{index}]\nimage = \"app-{index}:1.0\"\nport = {index}\n\n"
        ),
        "HCL" => format!(
            "  # service {index}\n  svc_{index} {{\n    image = \"app-{index}:1.0\"\n    port  = {index}\n  }}\n"
        ),
        _ => format!(
            "    <!-- service {index} -->\n    <svc_{index}>\n      <image>app-{index}:1.0</image>\n      <port>{index}</port>\n    </svc_{index}>\n"
        ),
    };
    let body: String = (0..blocks).map(each).collect();
    match language.label() {
        "YAML" => format!("x-defaults: &defaults\n  restart: always\nservices:\n{body}"),
        "JSON" => format!("{{\n  \"services\": {{\n{body}  }}\n}}\n"),
        "HCL" => format!("services {{\n{body}}}\n"),
        "XML" => format!("<root>\n  <services>\n{body}  </services>\n</root>\n"),
        _ => body,
    }
}

/// The path of a service's image in `language`'s document.
fn image(language: Language, index: usize) -> String {
    let root = if language.label() == "XML" {
        "/root"
    } else {
        ""
    };
    format!("{root}/services/svc_{index}/image")
}

/// The host languages and the language each is edited as (itself, or
/// another view).
fn views() -> Vec<(Language, Language)> {
    let mut views: Vec<(Language, Language)> = Language::ALL.iter().map(|&l| (l, l)).collect();
    views.extend([
        (Language::YAML, Language::JSON),
        (Language::TOML, Language::JSON),
        (Language::XML, Language::JSON),
        (Language::HCL, Language::YAML),
        (Language::JSON, Language::YAML),
    ]);
    views
}

/// Types `qqq` inside the image of the middle service, a key at a time,
/// as the browser does.
pub(super) fn type_in_middle(state: EditorState, blocks: usize) {
    let value = format!("app-{}:", blocks / 2);
    let shown = state.doc().text.get_untracked();
    let byte = shown.find(&value).expect("the middle image") + 2;
    for typed in 0..3 {
        let before = state.doc().text.get_untracked();
        let at = before[..byte + typed].encode_utf16().count();
        let mut text = before.clone();
        text.insert(byte + typed, 'q');
        let key = Edit::input(
            before,
            (at, at),
            text,
            (at + 1, at + 1),
            "insertText".into(),
            typed as f64,
        );
        assert!(state.commit(key), "typed");
        let _ = state.protect().protected.with_untracked(|p| p.locks.len());
    }
}

/// The host's read-only range over the first service's image in `text`.
fn first_image_range(text: &str) -> TextRange {
    let at = text.find("app-0:1.0").expect("the first image");
    let position = |at: usize| {
        let before = &text[..at];
        let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        TextPosition::new(before.matches('\n').count() + 1, column)
    };
    TextRange::new(position(at), position(at + "app-0:1.0".len()))
}

/// The bytes parsed and the documents read for their spans typing three keys
/// (the locks read after it, as the view does), with `locks` locked, and
/// the first image too with `range` (a host's read-only range over it).
fn typing_work(host: Language, view: Language, locks: &[FieldPath], range: bool) -> [usize; 3] {
    let blocks = 200;
    let text = services(host, blocks);
    let (_owner, state) = TestEditor {
        edit_as: (view != host).then_some(view),
        read_only_paths: locks.to_vec(),
        read_only_ranges: if range {
            vec![first_image_range(&text)]
        } else {
            Vec::new()
        },
        ..TestEditor::new(host, &text)
    }
    .mount();
    let count = || state.protect().protected.with_untracked(|p| p.locks.len());
    // A range is a lock of its text and of the field it covers (in another
    // view, of the field alone).
    let expected = locks.len()
        + if range && view == host {
            2
        } else {
            usize::from(range)
        };
    assert_eq!(count(), expected, "{host} as {view}: locked");
    let ((((), parsed), reads), paths) = counted(Work::PathLookup, || {
        counted(Work::DocumentRead, || {
            counted(Work::ParsedBytes, || {
                type_in_middle(state, blocks);
                let _ = count();
            })
        })
    });
    // The lock is where placing it in the new text puts it.
    let shown = state.doc().text.get_untracked();
    // In another view the range locks the field it covers, by its value.
    let ranges = if view == host {
        state.protect().ranges.get_untracked()
    } else {
        Vec::new()
    };
    let covered = if range && view != host {
        vec![FieldPath::from(image(host, 0).as_str())]
    } else {
        Vec::new()
    };
    let placed = protect(
        view.behavior(),
        &shown,
        Sources {
            paths: locks,
            ranges: &ranges,
            range_fields: &covered,
            ..Sources::default()
        },
    );
    state.protect().protected.with_untracked(|carried| {
        assert_eq!(
            carried.locks, placed.locks,
            "{host} as {view}: the lock moved"
        );
    });
    [parsed, reads, paths]
}

#[test]
fn a_lock_away_from_a_keystroke_costs_it_no_read_of_the_document_in_every_view() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for (host, view) in views() {
        let free = typing_work(host, view, &[], false);
        let path = [FieldPath::from(image(host, 0).as_str())];
        // The line's path is read once for the three keys (each key's check
        // and locks ask for it).
        let [parsed, reads, paths] = free;
        for (kind, locked) in [
            ("a path", typing_work(host, view, &path, false)),
            ("a range", typing_work(host, view, &[], true)),
        ] {
            if locked != [parsed, reads, paths + 1] {
                costly.push(format!(
                    "{host} as {view}, {kind}: (bytes parsed, span reads, path lookups) {locked:?} with a lock, {free:?} without"
                ));
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}

/// The out-of-reach fast path gives the full rule's answer: random edits
/// (characters typed, deleted or replaced anywhere, in documents locked by
/// the schema, by paths and by a host range) are judged alike with the
/// fast path and without it.
#[test]
fn the_fast_path_answers_as_the_full_rule_on_random_edits() {
    use super::guard::{check, without_reach};
    use super::reach::out_of_reach;
    use super::tests::{documents, schema};

    let schema = schema();
    let mut seed: u64 = std::env::var("REACH_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(0x9e37_79b9_7f4a_7c15);
    let cases: usize = std::env::var("REACH_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(400);
    let mut next = move |below: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        usize::try_from(seed % below.max(1) as u64).unwrap_or(0)
    };
    let typed = ["1", "a", " ", ":", "\"", "\n", "#", "x9", "=", ","];
    let mut differ = Vec::new();
    let mut fast = 0;
    for (language, text) in documents() {
        let behavior = language.behavior();
        let paths = [FieldPath::from("/port"), FieldPath::from("/meta")];
        let at = text.find("abc").expect("the id");
        let range = at..at + 3;
        let sources = [
            Sources {
                marking_schema: Some(&schema),
                reading_schema: Some(&schema),
                ..Sources::default()
            },
            Sources {
                reading_schema: Some(&schema),
                paths: &paths,
                ..Sources::default()
            },
            Sources {
                reading_schema: Some(&schema),
                ranges: std::slice::from_ref(&range),
                ..Sources::default()
            },
        ];
        for sources in sources {
            let protected = protect(behavior, text, sources);
            for _ in 0..cases {
                let starts: Vec<usize> = text.char_indices().map(|(at, _)| at).collect();
                let at = starts[next(starts.len())];
                let end = (at + next(3)).min(text.len());
                let end = (end..=text.len())
                    .find(|end| text.is_char_boundary(*end))
                    .unwrap_or(text.len());
                let insert = if next(3) == 0 {
                    ""
                } else {
                    typed[next(typed.len())]
                };
                let after = format!("{}{insert}{}", &text[..at], &text[end..]);
                let with = check(behavior, Some(&schema), &protected, text, &after).is_ok();
                let without = without_reach(|| {
                    check(behavior, Some(&schema), &protected, text, &after).is_ok()
                });
                if out_of_reach(behavior, &protected, text, &after).is_some() {
                    fast += 1;
                }
                if with != without {
                    differ.push(format!("{language}: {text:?} -> {after:?}: fast {with}"));
                }
            }
        }
    }
    assert!(fast > 0, "no edit took the fast path");
    assert!(differ.is_empty(), "{differ:#?}");
}
