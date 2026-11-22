//! An open editor keeps few copies of its document (P5): the places that
//! hold the same text (the document, the host's text, the host log, what
//! validation read, the baseline, the locks, the parses kept) share one,
//! mounted and after typing. A long text makes each copy plain among the
//! allocations.
//!
//! Typed in, the baseline is the text the editor mounted with, which the
//! diff and the dirty flag compare with: a second text, kept once. Editing
//! as another language, the host's text and the text on screen are two
//! texts, one copy each.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::parse_cache;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use crate::live_allocations::{copies_of, watch_from};
use leptos::prelude::*;

/// What every copy of a text starts with: long enough to be no other
/// allocation's.
const HEAD: usize = 40;

/// `text`'s length in UTF-16 units, as the browser counts.
fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// About 400 KB written in `language`.
#[cfg_attr(
    not(any(feature = "json", feature = "toml")),
    allow(unused_variables, reason = "the other languages a build may leave out")
)]
fn long_text(language: Language) -> String {
    let entries = 25_000;
    #[cfg(feature = "json")]
    if language == Language::JSON {
        let body: Vec<String> = (0..entries)
            .map(|index| format!("  \"head of the copied text {index}\": {index}"))
            .collect();
        return format!("{{\n{}\n}}\n", body.join(",\n"));
    }
    #[cfg(feature = "toml")]
    if language == Language::TOML {
        return (0..entries)
            .map(|index| format!("head_of_the_copied_text_{index} = {index}\n"))
            .collect();
    }
    (0..entries)
        .map(|index| format!("head of the copied text {index}: {index}\n"))
        .collect()
}

/// `typed` written after the last value, as the browser's input.
fn type_at_end(state: EditorState, typed: &str) {
    let before = state.doc().text.get_untracked();
    let at = before.trim_end_matches(['\n', '}']).len();
    let text = format!("{}{typed}{}", &before[..at], &before[at..]);
    let start = units(&before[..at]);
    state.commit(Edit::input_units(
        before,
        (start, start),
        text,
        (start, start + units(typed)),
        "insertText".to_string(),
        0.0,
    ));
}

/// Reads what the view reads of the text: the problems, the dirty flag,
/// the locks.
fn read_as_shown(state: EditorState) {
    let _ = state.problems().diagnostics.get_untracked();
    let _ = state.editing().dirty.get_untracked();
    #[cfg(feature = "protect")]
    let _ = state.protect().protected.get_untracked();
}

/// The copies kept of texts starting as the host's text does and of texts
/// starting as the text on screen does (none when that is the host's
/// language): mounted, then after typing and the typing pause, with a host
/// that hears each text, as a binding's does.
fn kept(host: Language, edit_as: Option<Language>) -> [(usize, usize); 2] {
    let text = long_text(host);
    let host_head = text[..HEAD].to_string();
    let mut editor = TestEditor::new(host, &text);
    editor.edit_as = edit_as;
    // The host hears each text and keeps nothing of it here.
    editor.on_change = Some(Callback::new(|_: String| {}));
    watch_from(text.len() / 2);
    drop(text);
    // Mounted, as the component holds the parses.
    parse_cache::hold();
    let (owner, state) = editor.mount();
    read_as_shown(state);
    let view_head = state
        .doc()
        .text
        .with_untracked(|text| text[..HEAD].to_string());
    let count = || {
        let view = if view_head == host_head {
            0
        } else {
            copies_of(&view_head)
        };
        (copies_of(&host_head), view)
    };
    let mounted = count();
    eprintln!("MOUNTED {host:?}");
    for typed in ["7", "8", "9"] {
        type_at_end(state, typed);
    }
    // The typing pause's work, and what the view reads then.
    state.check_text_now();
    read_as_shown(state);
    let typed = count();
    eprintln!("TYPED {host:?}");
    drop(owner);
    parse_cache::release();
    watch_from(0);
    [mounted, typed]
}

#[test]
fn an_open_editor_keeps_one_copy_of_its_text() {
    #[cfg_attr(
        not(any(feature = "json", feature = "toml")),
        allow(unused_mut, reason = "the other languages a build may leave out")
    )]
    let mut cases = vec![Language::YAML];
    #[cfg(feature = "json")]
    cases.push(Language::JSON);
    #[cfg(feature = "toml")]
    cases.push(Language::TOML);
    let mut failures = Vec::new();
    for host in cases {
        let [mounted, typed] = kept(host, None);
        eprintln!("{host:?}: copies kept: mounted {mounted:?}, typed {typed:?}");
        // Typed: the document and the baseline.
        if [mounted, typed] != [(1, 0), (2, 0)] {
            failures.push(format!("{host:?}: mounted {mounted:?}, typed {typed:?}"));
        }
    }
    assert!(
        failures.is_empty(),
        "copies kept (texts as the host's, texts as the view's):\n{}",
        failures.join("\n")
    );
}

/// Editing YAML as JSON: one copy of the host's text (the host's text, the
/// host log, the baseline, the parses) and one of the text on screen.
/// Typed in, besides the two texts now: the baseline, the host text mounted
/// with; the text on screen mounted with, which the undo step goes back to
/// (the baseline as shown shares it); and the parse the save kept of a host
/// text it wrote on the way (the parses kept are bounded).
#[test]
#[cfg(feature = "json")]
fn an_edit_as_view_keeps_one_copy_of_each_text() {
    let [mounted, typed] = kept(Language::YAML, Some(Language::JSON));
    eprintln!("YAML as JSON: copies kept: mounted {mounted:?}, typed {typed:?}");
    assert_eq!([mounted, typed], [(1, 1), (3, 2)]);
}
