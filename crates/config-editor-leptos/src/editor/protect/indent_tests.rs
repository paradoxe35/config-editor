//! Tab and Shift+Tab on lines inside a read-only block, in every language:
//! whitespace around locks is free except a YAML locked line's indent
//! (structure there), so indenting or outdenting a whole locked line is
//! allowed wherever the language ignores indentation and refused in YAML;
//! the locked values never change.

use super::super::Language;
use super::super::commands::{indent_lines, tab_insert};
use super::super::edit::Edit;
use super::super::keyboard::replace_selection;
use super::super::paths::FieldPath;
use super::super::test_state::TestEditor;
use super::tests::documents;
use leptos::prelude::*;

/// What a key press does to the line holding `b` in `/meta`.
#[derive(Clone, Copy, Debug)]
enum Press {
    /// Tab with the caret before the key.
    TabAtKey,
    /// Tab with the whole line selected.
    TabOnLine,
    /// Shift+Tab with the caret before the key.
    ShiftTab,
    /// Tab with the caret at the key's name: past a JSON quote or an XML
    /// `<`, which splits the key (refused); before a bare key, indent.
    TabInKey,
}

/// The text after `press` on the `b` line of `text`, or `None` when the
/// key changes nothing there (an outdent at column 0).
fn pressed(text: &str, press: Press, unit: &str) -> Option<(usize, usize, String, usize)> {
    let key = text.match_indices('b').map(|(at, _)| at).find(|at| {
        let line_start = text[..*at].rfind('\n').map_or(0, |start| start + 1);
        text[line_start..*at]
            .trim_end_matches(['"', '<'])
            .trim()
            .is_empty()
    });
    let key = key?;
    let line_start = text[..key].rfind('\n').map_or(0, |start| start + 1);
    let caret = key - usize::from(text[line_start..key].ends_with(['"', '<']));
    let line_end = text[key..].find('\n').map_or(text.len(), |end| key + end);
    let updated = match press {
        Press::TabAtKey => {
            let (updated, after) =
                replace_selection(text, caret, caret, &tab_insert(text, caret, unit));
            return Some((caret, caret, updated, after));
        }
        Press::TabInKey => {
            let (updated, after) = replace_selection(text, key, key, &tab_insert(text, key, unit));
            return Some((key, key, updated, after));
        }
        Press::TabOnLine => indent_lines(text, line_start, line_end, false, unit)?,
        Press::ShiftTab => indent_lines(text, caret, caret, true, unit)?,
    };
    let (updated, _, end) = updated;
    (updated != text).then_some((caret, caret, updated, end))
}

#[test]
fn indenting_a_locked_line_is_free_except_in_yaml() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        for press in [
            Press::TabAtKey,
            Press::TabOnLine,
            Press::ShiftTab,
            Press::TabInKey,
        ] {
            let mut editor = TestEditor::new(language, text);
            editor.read_only_paths = vec![FieldPath::from("/meta")];
            let (_owner, state) = editor.mount();
            let Some((start, end, updated, caret)) = pressed(text, press, "  ") else {
                continue;
            };
            state.commit(Edit::new(
                text.to_string(),
                (start, end),
                updated.clone(),
                caret,
            ));
            let now = state.doc().text.get_untracked();
            let applied = now == updated;
            let splits_key =
                matches!(press, Press::TabInKey) && matches!(language.label(), "JSON" | "XML");
            let expected = language != Language::YAML && !splits_key;
            if applied != expected {
                failures.push(format!(
                    "{language} {press:?}: {} (now {now:?})",
                    if applied { "allowed" } else { "refused" }
                ));
            }
            let value = |text: &str| language.parse(text).ok().map(|value| value["meta"].clone());
            if value(&now) != value(text) {
                failures.push(format!("{language} {press:?}: /meta changed: {now:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
