//! The host's leading byte order mark outlasts every edit: the text on
//! screen never holds it, so nothing done to the first line (cut, deleted,
//! moved, duplicated, indented, typed before, pasted over) reaches it, and
//! the host hears it once, at the start (round 29 lost, moved or doubled it).

use leptos::prelude::*;

use crate::editor::edit::Edit;
use crate::editor::host_editing::SaveRequest;
use crate::editor::languages::Language;
use crate::editor::sync::convert_now;
use crate::editor::test_state::TestEditor;

const BOM: char = '\u{FEFF}';

/// A document in `language` whose first two lines can trade places, be
/// cut or doubled and stay readable.
fn sample(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "# head\nname: a\nport: 1\n",
        "JSON" => "{\n  \"name\": \"a\",\n  \"port\": 1\n}\n",
        "TOML" | "HCL" => "# head\nname = \"a\"\nport = 1\n",
        "XML" => "<?xml version=\"1.0\"?>\n<c>\n  <name>a</name>\n  <port>1</port>\n</c>\n",
        other => panic!("no sample for {other}"),
    }
}

/// The first line's end in `text`.
fn first_line_end(text: &str) -> usize {
    text.find('\n').unwrap_or(text.len())
}

/// What each edit of the first line makes of the text on screen, as the
/// keys do it (`Shift+Down Delete`, `Ctrl+Shift+K`, `Ctrl+X` on a line,
/// `Ctrl+A` and a paste, `Home` and typing, `Tab` and `Ctrl+]` on it,
/// `Alt+Down`, `Shift+Alt+Up`).
fn edits(text: &str) -> Vec<(&'static str, String)> {
    let end = first_line_end(text);
    let (first, rest) = (&text[..end], &text[end + 1..]);
    let second_end = rest.find('\n').unwrap_or(rest.len());
    let (second, after) = (&rest[..second_end], &rest[second_end..]);
    vec![
        ("delete the first line", rest.to_string()),
        ("paste over everything", text.to_string().replace('a', "b")),
        ("type before it", format!("x{text}")),
        ("indent it", format!("  {text}")),
        ("move it down", format!("{second}\n{first}{after}")),
        ("duplicate it", format!("{first}\n{text}")),
    ]
}

/// The mark's places in `text`.
fn marks(text: &str) -> Vec<usize> {
    text.char_indices()
        .filter(|(_, c)| *c == BOM)
        .map(|(at, _)| at)
        .collect()
}

/// Every edit of the first line in every language, on LF and CRLF hosts:
/// the screen has no mark, and `on_change` and `on_save` (when the text
/// parses) start with it, once, followed by the text on screen.
#[test]
fn no_edit_of_the_first_line_reaches_the_byte_order_mark() {
    let mut failures = Vec::new();
    for &language in Language::ALL {
        for crlf in [false, true] {
            let plain = sample(language);
            let host = if crlf {
                format!("{BOM}{}", plain.replace('\n', "\r\n"))
            } else {
                format!("{BOM}{plain}")
            };
            for (what, edited) in edits(plain) {
                let changes = RwSignal::new(Vec::<String>::new());
                let saves = RwSignal::new(Vec::<String>::new());
                let mut editor = TestEditor::new(language, &host);
                editor.on_change = Some(Callback::new(move |text| {
                    changes.update(|all| all.push(text));
                }));
                editor.on_save = Some(Callback::new(move |request: SaveRequest| {
                    saves.update(|all| all.push(request.text));
                }));
                let (_owner, state) = editor.mount();
                let case = format!("{language} (crlf {crlf}) {what}");
                let before = state.doc().text.get_untracked();
                if !marks(&before).is_empty() {
                    failures.push(format!("{case}: the screen holds the mark: {before:?}"));
                }
                let caret = edited.len();
                state.commit(Edit::new(
                    before.clone(),
                    (0, before.len()),
                    edited.clone(),
                    caret,
                ));
                state.save();
                let mut expected = format!("{BOM}{edited}");
                if crlf {
                    expected = expected.replace('\n', "\r\n");
                }
                let heard = changes.get_untracked();
                let heard = heard.last();
                if heard.map(|text| marks(text)) != Some(vec![0]) {
                    failures.push(format!("{case}: on_change heard {heard:?}"));
                } else if !crlf && heard != Some(&expected) {
                    failures.push(format!(
                        "{case}: on_change heard {heard:?}, not {expected:?}"
                    ));
                }
                // A text that does not parse is not saved.
                let saved = saves.get_untracked();
                let sendable = language.parse(&edited).is_ok();
                if sendable && saved.last().map(|text| marks(text)) != Some(vec![0]) {
                    failures.push(format!("{case}: on_save got {saved:?}"));
                } else if !sendable && !saved.is_empty() {
                    failures.push(format!("{case}: invalid, on_save got {saved:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A host emptied keeps its mark alone, and written again gets its text
/// after it.
#[test]
fn an_emptied_document_written_again_is_marked() {
    for &language in Language::ALL {
        let changes = RwSignal::new(Vec::<String>::new());
        let host = format!("{BOM}{}", sample(language));
        let mut editor = TestEditor::new(language, &host);
        editor.on_change = Some(Callback::new(move |text| {
            changes.update(|all| all.push(text));
        }));
        let (_owner, state) = editor.mount();
        let before = state.doc().text.get_untracked();
        state.commit(Edit::new(
            before.clone(),
            (0, before.len()),
            String::new(),
            0,
        ));
        let plain = sample(language).to_string();
        state.commit(Edit::new(String::new(), (0, 0), plain.clone(), plain.len()));
        assert_eq!(
            changes.get_untracked(),
            vec![BOM.to_string(), format!("{BOM}{plain}")],
            "{language}"
        );
    }
}

/// Edits from every other view keep the mark once, at the start.
#[test]
fn edits_from_every_view_keep_the_byte_order_mark() {
    let mut failures = Vec::new();
    for &host in Language::ALL {
        for &view in Language::ALL {
            if view == host {
                continue;
            }
            let changes = RwSignal::new(Vec::<String>::new());
            let mut editor = TestEditor::new(host, &format!("{BOM}{}", sample(host)));
            editor.edit_as = Some(view);
            editor.on_change = Some(Callback::new(move |text| {
                changes.update(|all| all.push(text));
            }));
            let (_owner, state) = editor.mount();
            let before = state.doc().text.get_untracked();
            let Some(at) = before.find('1') else {
                failures.push(format!("{host} as {view}: no value in {before:?}"));
                continue;
            };
            let after = format!("{}2{}", &before[..at], &before[at + 1..]);
            state.commit(Edit::new(before, (at, at + 1), after.clone(), at + 1));
            convert_now(state, after, view);
            let heard = changes.get_untracked();
            match heard.last() {
                Some(text) if marks(text) == vec![0] && text.contains('2') => {}
                other => failures.push(format!("{host} as {view}: on_change heard {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Undoing back to an empty screen (a file that is only its mark, edited and
/// undone; a document emptied by select-all and Delete, then undone and
/// saved) hands the host the mark, never the bare empty text.
#[test]
fn undoing_to_an_empty_screen_keeps_the_byte_order_mark() {
    for &language in Language::ALL {
        let changes = RwSignal::new(Vec::<String>::new());
        let mut editor = TestEditor::new(language, &BOM.to_string());
        editor.on_change = Some(Callback::new(move |text| {
            changes.update(|all| all.push(text));
        }));
        let (_owner, state) = editor.mount();
        assert_eq!(state.doc().text.get_untracked(), "", "{language}");
        state.commit(Edit::new(String::new(), (0, 0), "Z".to_string(), 1));
        state.commit(Edit::new("Z".to_string(), (0, 1), String::new(), 0));
        assert_eq!(
            changes.get_untracked().last(),
            Some(&BOM.to_string()),
            "{language}"
        );
    }
}
