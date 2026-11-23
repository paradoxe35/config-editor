//! P1: the host's ranges name places in the host's text. While a save is
//! held, the text on screen has moved on from it by edits the host has not
//! heard (a drag's move, typing over a selection); ranges given then are
//! mapped through what those edits recorded they did, never guessed from
//! the two texts, and a range whose text those edits already changed holds
//! the save until the text holds it again: the host never hears a text
//! without its locked text.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::EditorSettings;
use super::super::Language;
use super::super::edit::Edit;
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::state::{EditorState, Init};
use super::ProtectInit;

/// One text per language, with wide characters (an emoji and a ZWJ
/// family) before the fields, so UTF-16 offsets, bytes and characters all
/// differ.
const TEXTS: [(Language, &str); 5] = [
    (
        Language::YAML,
        "# 😀👨\u{200d}👩\u{200d}👧 notes\nlisten: 7835\npublic_address: tunnel.example.com\n",
    ),
    (
        Language::JSON,
        "{\n  \"note\": \"😀👨\u{200d}👩\u{200d}👧\",\n  \"listen\": 7835,\n  \"public_address\": \"tunnel.example.com\"\n}\n",
    ),
    (
        Language::TOML,
        "# 😀👨\u{200d}👩\u{200d}👧 notes\nlisten = 7835\npublic_address = \"tunnel.example.com\"\n",
    ),
    (
        Language::HCL,
        "# 😀👨\u{200d}👩\u{200d}👧 notes\nlisten = 7835\npublic_address = \"tunnel.example.com\"\n",
    ),
    (
        Language::XML,
        "<!-- 😀👨\u{200d}👩\u{200d}👧 notes -->\n<config>\n  <server>\n    <listen>7835</listen>\n  </server>\n  <public_address>tunnel.example.com</public_address>\n</config>\n",
    ),
];

/// The host's side: its ranges, and what it heard.
struct Host {
    ranges: RwSignal<Vec<TextRange>>,
    heard: RwSignal<Vec<String>>,
}

fn mount(language: Language, text: &str, host: &Host) -> EditorState {
    let heard = host.heard;
    let state = EditorState::new(Init {
        host_language: language,
        edit_language: language,
        schema: None,
        schema_error: None,
        host_text: text.to_string(),
        baseline: text.to_string(),
        opening: None,
        read_only: Signal::stored(false),
        read_only_reason: Signal::stored(None),
        settings: Signal::stored(EditorSettings::default()),
        host_on_change: Some(Callback::new(move |text| {
            heard.update(|all| all.push(text));
        })),
        channel: None,
        protect: ProtectInit {
            // `listen` is locked by its path, and held by a copy of it
            // written after the fields (see `hold`).
            paths: Some(Signal::stored(vec![FieldPath::from(
                if language == Language::XML {
                    "/config/server/listen"
                } else {
                    "/listen"
                },
            )])),
            secrets: None,
            foreign: None,
        },
        on_save: None,
        on_baseline_conflict: None,
        clipboard: None,
        key_names: Default::default(),
        history: None,
    })
    .0;
    crate::editor::sync::follow_lists(
        state,
        crate::editor::sync::Lists {
            read_only_ranges: Some(host.ranges.into()),
            ..crate::editor::sync::Lists::default()
        },
    );
    state
}

/// Bytes `range` of `text` as the host gives a range: lines and character
/// columns, from one.
fn text_range(text: &str, range: std::ops::Range<usize>) -> TextRange {
    let position = |at: usize| {
        let line_start = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
        TextPosition::new(
            text[..at].matches('\n').count() + 1,
            text[line_start..at].chars().count() + 1,
        )
    };
    TextRange::new(position(range.start), position(range.end))
}

/// The host gives one range (bytes of its text) in place of its others,
/// effects hand them on.
fn give(state: EditorState, host: &Host, text: &str, range: std::ops::Range<usize>) {
    host.ranges.set(vec![text_range(text, range)]);
    state.place_ranges();
    state.locks_changed_by_host();
}

/// Browser input of `kind` writing `with` over bytes `at` of the text on
/// screen, the caret (or a drop's selection) after it; whether it was
/// taken.
fn input(state: EditorState, at: std::ops::Range<usize>, with: &str, kind: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..at.start], &before[at.end..]);
    let units = |byte: usize| before[..byte].encode_utf16().count();
    let end = units(at.start) + with.encode_utf16().count();
    let typed = if kind == "insertFromDrop" {
        (units(at.start), end)
    } else {
        (end, end)
    };
    state.commit(Edit::input_units(
        before.clone(),
        (units(at.start), units(at.end)),
        after,
        typed,
        kind.into(),
        0.0,
    ))
}

/// The texts the host ranges lock in the text on screen.
fn locked(state: EditorState) -> Vec<String> {
    let shown = state.doc().text.get_untracked();
    state.protect().ranges.with_untracked(|ranges| {
        ranges
            .iter()
            .map(|range| shown[*range].to_string())
            .collect()
    })
}

/// What [`hold`] writes in `language`: the text, the text it goes after,
/// and the text that follows it there (to find it again).
fn hold_edit(language: Language) -> (&'static str, &'static str, &'static str) {
    if language == Language::XML {
        // Another copy of the block, read in its place.
        (
            "\n  <server>\n    <listen>1</listen>\n  </server>",
            "</server>",
            "\n  <public",
        )
    } else if language == Language::JSON {
        // A copy of the key after the fields, read in its place.
        (",\n  \"listen\": 1", "\"tunnel.example.com\"", "\n}")
    } else {
        // The field commented out.
        ("# ", "notes\n", "listen")
    }
}

/// A lock the parse then reads otherwise: `listen`, locked by its path,
/// commented out (in JSON, which has no comments, and XML, another copy of
/// it read in its place). Taken, held, so the host stays a text behind the
/// screen; the text still parses. (A range touching a key locks that key,
/// so typing into it is no way to hold.)
fn hold(language: Language, state: EditorState, _host: &Host, text: &str) {
    let (written, after, _) = hold_edit(language);
    let at = text.find(after).expect("the field") + after.len();
    let taken = input(state, at..at, written, "insertFromPaste");
    assert!(taken, "{language:?}: taken");
    assert!(state.changes_held(), "{language:?}: held");
}

/// A drag moving the end of a value to the end of the text (its delete,
/// then its drop) while a save is held, wide characters before it: a range
/// the host then gives on the rest of the value locks that text, where the
/// edits moved it.
#[test]
fn ranges_given_after_a_held_drag_lock_their_text_in_every_language() {
    for (language, text) in TEXTS {
        let _owner = Owner::new();
        _owner.set();
        let host = Host {
            ranges: RwSignal::new(Vec::new()),
            heard: RwSignal::new(Vec::new()),
        };
        let state = mount(language, text, &host);
        hold(language, state, &host, text);
        let shown = state.doc().text.get_untracked();
        let start = shown.find(".com").expect("the value's end");
        let end = start + ".com".len();
        let moved = shown[start..end].to_string();
        assert!(
            input(state, start..end, "", "deleteByDrag"),
            "{language:?}: the drag's delete is taken"
        );
        let end = state.doc().text.get_untracked().len();
        assert!(
            input(state, end..end, &moved, "insertFromDrop"),
            "{language:?}: the drop is taken"
        );
        // The host locks `tunnel.example` in its text, which has heard none
        // of the edits.
        let value = text.find("tunnel.example").expect("the value");
        give(state, &host, text, value..value + "tunnel.example".len());
        assert_eq!(
            locked(state),
            ["tunnel.example"],
            "{language:?}: in {:?}",
            state.doc().text.get_untracked()
        );
    }
}

/// A line copied below itself while a save is held (the copy is the new
/// line, the original keeps its place): a range the host then gives from
/// that line's end into the next one has its text written whole only from
/// the copy on, and locks it there.
#[test]
fn a_range_across_a_line_copied_while_held_locks_its_text_in_every_language() {
    for (language, text) in TEXTS {
        let _owner = Owner::new();
        _owner.set();
        let host = Host {
            ranges: RwSignal::new(Vec::new()),
            heard: RwSignal::new(Vec::new()),
        };
        let state = mount(language, text, &host);
        hold(language, state, &host, text);
        let shown = state.doc().text.get_untracked();
        let port = shown.find("7835").expect("the port");
        let start = shown[..port].rfind('\n').map_or(0, |newline| newline + 1);
        let end = port + shown[port..].find('\n').expect("its line end") + 1;
        let line = shown[start..end].to_string();
        assert!(
            input(state, end..end, &line, "insertFromPaste"),
            "{language:?}: the copy is taken"
        );
        let port = text.find("7835").expect("the port");
        let next = port + text[port..].find('\n').expect("its line end") + 1;
        let locked_text = &text[port..next + 2];
        give(state, &host, text, port..next + 2);
        assert_eq!(
            locked(state),
            [locked_text],
            "{language:?}: in {:?}",
            state.doc().text.get_untracked()
        );
    }
}

/// Takes back what [`hold`] wrote, by a new edit (not an undo): the lock
/// it held reads as the host gave it again.
fn unhold(language: Language, state: EditorState) {
    let shown = state.doc().text.get_untracked();
    let (written, _, next) = hold_edit(language);
    let at = shown
        .find(&format!("{written}{next}"))
        .expect("what hold wrote");
    assert!(
        input(state, at..at + written.len(), "", "deleteContentForward"),
        "{language:?}: taken back"
    );
}

/// A value typed over while a save is held, then locked by the host in
/// its text (which still has it): when the first hold goes, the save still
/// waits, saying why, and the host never hears a text without the locked
/// text; writing it back releases the save, the range on its text.
#[test]
fn a_range_the_held_edits_changed_before_the_lock_holds_the_save_in_every_language() {
    for (language, text) in TEXTS {
        let _owner = Owner::new();
        _owner.set();
        let host = Host {
            ranges: RwSignal::new(Vec::new()),
            heard: RwSignal::new(Vec::new()),
        };
        let state = mount(language, text, &host);
        hold(language, state, &host, text);
        let shown = state.doc().text.get_untracked();
        let value = shown.find("tunnel").expect("the value");
        assert!(
            input(state, value..value + "tunnel".len(), "lan", "insertText"),
            "{language:?}: the value typed over"
        );
        let value = text.find("tunnel").expect("the value");
        give(state, &host, text, value..value + "tunnel.".len());
        unhold(language, state);
        assert!(state.changes_held(), "{language:?}: still held");
        assert!(
            state
                .save_waits()
                .is_some_and(|why| why.starts_with("Not saved: ") && why.contains("\"tunnel.\"")),
            "{language:?}: the save says why: {:?}",
            state.save_waits()
        );
        assert!(
            host.heard
                .get_untracked()
                .iter()
                .all(|heard| heard.contains("tunnel.")),
            "{language:?}: the host heard {:?}",
            host.heard.get_untracked()
        );
        let shown = state.doc().text.get_untracked();
        let value = shown.find("lan.").expect("the value typed");
        assert!(
            input(state, value..value + "lan".len(), "tunnel", "insertText"),
            "{language:?}: the value written back"
        );
        assert_eq!(state.doc().text.get_untracked(), text, "{language:?}");
        assert!(!state.changes_held(), "{language:?}: released");
        assert_eq!(locked(state), ["tunnel."], "{language:?}");
        assert_eq!(
            host.heard.get_untracked().last().map(String::as_str),
            Some(text),
            "{language:?}: the host hears the text once released"
        );
    }
}

/// A drag moving a piece of a comment to the end of the text while a save
/// is held: a range the host then gives inside that piece locks it where
/// the drag put it, unchanged.
#[test]
fn a_range_inside_text_a_held_drag_moved_locks_it_where_it_went() {
    let _owner = Owner::new();
    _owner.set();
    let text = "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\n";
    let host = Host {
        ranges: RwSignal::new(Vec::new()),
        heard: RwSignal::new(Vec::new()),
    };
    let state = mount(Language::YAML, text, &host);
    let port = text.find("sten: 78").expect("the port");
    give(state, &host, text, port..port + "sten: 78".len());
    for (at, typed) in ["l", "i", "s", "t", "e", "n"].into_iter().enumerate() {
        assert!(input(state, at..at, typed, "insertText"), "{typed} typed");
    }
    assert!(state.changes_held(), "held");
    let heard = host
        .heard
        .get_untracked()
        .last()
        .cloned()
        .unwrap_or(text.to_string());
    let shown = state.doc().text.get_untracked();
    let start = shown.find("l server").expect("the piece");
    let end = start + "l server confi".len();
    let moved = shown[start..end].to_string();
    assert!(
        input(state, start..end, "", "deleteByDrag"),
        "the drag's delete"
    );
    let end = state.doc().text.get_untracked().len();
    assert!(input(state, end..end, &moved, "insertFromDrop"), "the drop");
    let piece = heard.find("er co").expect("in the host's text");
    give(state, &host, &heard, piece..piece + "er co".len());
    assert_eq!(
        locked(state),
        ["er co"],
        "heard {heard:?}, shown {:?}",
        state.doc().text.get_untracked()
    );
}

/// A drag moving the end of a value to the end of the text while a save is
/// held, then a space typed at the start: a range the host then gives
/// inside the moved text locks it where the drag put it, in every language.
#[test]
fn a_range_inside_text_dragged_while_held_locks_it_where_it_went_in_every_language() {
    for (language, text) in TEXTS {
        let _owner = Owner::new();
        _owner.set();
        let host = Host {
            ranges: RwSignal::new(Vec::new()),
            heard: RwSignal::new(Vec::new()),
        };
        let state = mount(language, text, &host);
        hold(language, state, &host, text);
        let heard = host
            .heard
            .get_untracked()
            .last()
            .cloned()
            .unwrap_or(text.to_string());
        let shown = state.doc().text.get_untracked();
        let start = shown.find("example.com").expect("the value's end");
        let end = start + "example.com".len();
        assert!(
            input(state, start..end, "", "deleteByDrag"),
            "{language:?}: the drag's delete"
        );
        let end = state.doc().text.get_untracked().len();
        assert!(
            input(state, end..end, "example.com", "insertFromDrop"),
            "{language:?}: the drop"
        );
        assert!(
            input(state, 0..0, " ", "insertText"),
            "{language:?}: typed after"
        );
        let piece = heard.find("ample").expect("in the host's text");
        give(state, &host, &heard, piece..piece + "ample".len());
        assert_eq!(
            locked(state),
            ["ample"],
            "{language:?}: in {:?}",
            state.doc().text.get_untracked()
        );
    }
}
