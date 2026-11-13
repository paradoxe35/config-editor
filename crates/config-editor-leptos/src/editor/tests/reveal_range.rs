//! [`EditorHandle::reveal_range`]'s range in the text on screen: one-based
//! lines and character columns to the textarea's UTF-16 offsets, whatever
//! the host's line endings, clamped to the text and put in order.

use leptos::prelude::*;

use crate::editor::handle::EditorHandle;
use crate::editor::languages::Language;
use crate::editor::paths::{TextPosition, TextRange};
use crate::editor::test_state::TestEditor;

/// An editor on `host` in `language`, shown as `edit_as`, with a handle.
fn editor(language: Language, host: &str, edit_as: Option<Language>) -> (Owner, EditorHandle) {
    let (owner, state) = TestEditor {
        edit_as,
        ..TestEditor::new(language, host)
    }
    .mount();
    let handle = EditorHandle::new();
    handle.attach(state);
    (owner, handle)
}

fn range(start: (usize, usize), end: (usize, usize)) -> TextRange {
    TextRange::new(
        TextPosition::new(start.0, start.1),
        TextPosition::new(end.0, end.1),
    )
}

/// The text on screen `offsets` select.
fn selected(handle: EditorHandle, offsets: (usize, usize)) -> String {
    let units: Vec<u16> = handle.text().unwrap().encode_utf16().collect();
    String::from_utf16_lossy(&units[offsets.0..offsets.1])
}

#[test]
fn columns_count_characters_and_offsets_utf16_units() {
    let (_owner, handle) = editor(Language::YAML, "a: é😀x # ç😀\nb: 2\n", None);
    let offsets = handle.offsets_of(range((1, 4), (1, 7))).unwrap();
    assert_eq!(offsets, (3, 7));
    assert_eq!(selected(handle, offsets), "é😀x");
    let comment = handle.offsets_of(range((1, 8), (1, 12))).unwrap();
    assert_eq!(selected(handle, comment), "# ç😀");
}

/// A CRLF or lone-CR host is shown with LF: lines are counted in the text
/// on screen, each ending one unit long.
#[test]
fn lines_are_the_text_on_screen_whatever_the_hosts_endings() {
    for host in [
        "a: 1\r\n# note\r\n\r\nb: ü\r\n",
        "a: 1\r# note\r\rb: ü\r",
        "a: 1\n# note\n\nb: ü\n",
    ] {
        let (_owner, handle) = editor(Language::YAML, host, None);
        let comment = handle.offsets_of(range((2, 1), (2, 7))).unwrap();
        assert_eq!(comment, (5, 11), "{host:?}");
        assert_eq!(selected(handle, comment), "# note", "{host:?}");
        let blank = handle.offsets_of(range((3, 1), (3, 1))).unwrap();
        assert_eq!(blank, (12, 12), "{host:?}");
        let value = handle.offsets_of(range((4, 4), (4, 5))).unwrap();
        assert_eq!(selected(handle, value), "ü", "{host:?}");
    }
}

#[test]
fn a_range_past_the_end_is_clamped_and_a_reversed_one_ordered() {
    let (_owner, handle) = editor(Language::YAML, "a: 1\nbé: 2", None);
    let length = handle.text().unwrap().encode_utf16().count();
    // A column past its line's end is the line's end.
    let line_end = handle.offsets_of(range((1, 3), (1, 99))).unwrap();
    assert_eq!(selected(handle, line_end), " 1");
    // A line past the text is the text's end.
    assert_eq!(handle.offsets_of(range((2, 1), (40, 1))), Some((5, length)));
    assert_eq!(
        handle.offsets_of(range((40, 1), (90, 9))),
        Some((length, length))
    );
    assert_eq!(handle.offsets_of(range((2, 3), (1, 1))), Some((0, 7)));
}

#[test]
fn nothing_to_reveal_before_mount_or_in_an_empty_document() {
    let unmounted = EditorHandle::new();
    assert!(!unmounted.reveal_range(range((1, 1), (1, 2))));
    assert_eq!(unmounted.offsets_of(range((1, 1), (1, 2))), None);
    let (_owner, handle) = editor(Language::YAML, "", None);
    assert_eq!(handle.offsets_of(range((1, 1), (1, 1))), None);
    assert!(!handle.reveal_range(range((1, 1), (1, 1))));
}

/// While editing as another language the range is in the text on screen.
#[cfg(feature = "json")]
#[test]
fn while_editing_as_another_language_the_range_is_on_screen() {
    let (_owner, handle) = editor(Language::YAML, "a: 1\n", Some(Language::JSON));
    let shown = handle.text().unwrap();
    assert!(shown.starts_with('{'), "{shown}");
    let line = shown
        .lines()
        .position(|line| line.contains("\"a\""))
        .unwrap()
        + 1;
    let column = shown.lines().nth(line - 1).unwrap().find('"').unwrap() + 1;
    let offsets = handle
        .offsets_of(range((line, column), (line, column + 3)))
        .unwrap();
    assert_eq!(selected(handle, offsets), "\"a\"");
}
