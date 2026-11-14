//! A never-shown document's save reads its line breaks as a shown editor
//! does: as LF (the editor's text), so a lone CR that TOML or HCL does not
//! read as a line break saves unshown exactly as shown, in every language,
//! with each kind of line break and a mix of them; the text sent is the
//! host's as given (QA 33, FW32-3).

use leptos::prelude::*;

use super::bom_save_tests::{LANGUAGES, sized, unshown};
use crate::editor::Language;

const BREAKS: [(&str, &str); 4] = [("LF", "\n"), ("CRLF", "\r\n"), ("CR", "\r"), ("mixed", "")];

/// `text` (LF) with `breaks`; `mixed` takes CRLF, CR and LF in turn.
fn with_breaks(text: &str, breaks: &str) -> String {
    if !breaks.is_empty() {
        return text.replace('\n', breaks);
    }
    let mut out = String::new();
    for (at, line) in text.split_inclusive('\n').enumerate() {
        let ending = ["\r\n", "\r", "\n"][at % 3];
        out.push_str(line.strip_suffix('\n').unwrap_or(line));
        if line.ends_with('\n') {
            out.push_str(ending);
        }
    }
    out
}

#[test]
fn a_never_shown_document_saves_whatever_its_line_breaks_as_a_shown_one() {
    let owner = Owner::new();
    owner.set();
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let text = sized(language, 4);
        for (name, breaks) in BREAKS {
            let given = with_breaks(&text, breaks);
            // A shown editor's gate reads its own text: the host's with LF.
            let shown = Language::saves(language, &text);
            let (one, all, heard) = unshown(language, &given);
            let expected: Vec<(String, String)> =
                vec![("x".into(), given.clone()), ("x".into(), given.clone())];
            if !shown || !one || all != ["x"] || heard != expected {
                failures.push(format!(
                    "{language:?} {name}: shown gate {shown}, save {one}, save_all {all:?}, \
                     heard {heard:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The gate reads a text with lone CRs once, as one with LF, at two
/// document sizes.
#[test]
fn a_text_with_lone_carriage_returns_is_read_once_by_the_save_gate() {
    use crate::editor::work_count::{Work, counted};
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for entries in [20, 2000] {
            let text = with_breaks(&sized(language, entries), "\r");
            let (saves, parsed) = counted(Work::ParsedBytes, || Language::saves(language, &text));
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
