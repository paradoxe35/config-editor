//! P4: an edit a lock held back (the field it covers read otherwise, the
//! save waiting with `saveHeld`) reaches the host once the host takes the
//! locks away: the change event carries the text on screen at once, not
//! at the next keystroke.

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::state::{EditorState, Init};
use super::super::{EditorSettings, Language};
use super::ProtectInit;

/// The host's side: its locks, and what it heard.
struct Host {
    paths: RwSignal<Vec<FieldPath>>,
    ranges: RwSignal<Vec<TextRange>>,
    heard: RwSignal<Vec<String>>,
}

fn mount(text: &str, host: &Host) -> EditorState {
    let heard = host.heard;
    EditorState::new(Init {
        host_language: Language::YAML,
        edit_language: Language::YAML,
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
        host_diagnostics: Signal::default(),
        protect: ProtectInit {
            paths: Some(host.paths.into()),
            ranges: Some(host.ranges.into()),
            secrets: None,
            foreign: None,
        },
        on_save: None,
        on_baseline_conflict: None,
        clipboard: None,
    })
    .0
}

/// Writes `with` over bytes `at` of the text on screen; whether it was taken.
fn write(state: EditorState, at: std::ops::Range<usize>, with: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..at.start], &before[at.end..]);
    let units = |byte: usize| before[..byte].encode_utf16().count();
    let caret = units(at.start) + with.encode_utf16().count();
    state.commit(Edit::input_units(
        before.clone(),
        (units(at.start), units(at.end)),
        after,
        (caret, caret),
        "insertText".into(),
        0.0,
    ))
}

#[test]
fn an_edit_a_lock_held_reaches_the_host_when_the_locks_go() {
    let text = "listen: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n";
    let line = "insecure_plaintext: false";
    for unlock in ["ranges", "paths"] {
        let owner = Owner::new();
        owner.set();
        let host = Host {
            paths: RwSignal::new(Vec::new()),
            ranges: RwSignal::new(Vec::new()),
            heard: RwSignal::new(Vec::new()),
        };
        if unlock == "ranges" {
            // The last characters of `false`, as the browser fuzz locked them.
            host.ranges.set(vec![TextRange::new(
                TextPosition::new(3, line.len() - 3),
                TextPosition::new(3, line.len()),
            )]);
        } else {
            host.paths.set(vec![FieldPath::from("/insecure_plaintext")]);
        }
        let state = mount(text, &host);
        // The range's field renamed, or the locked key written again:
        // taken, held.
        let (at, with) = if unlock == "ranges" {
            let at = text.find("insecure").expect("the key");
            (at..at + 1, "Ω")
        } else {
            (text.len()..text.len(), "insecure_plaintext: true\n")
        };
        assert!(write(state, at, with), "{unlock}: the edit is taken");
        let shown = state.doc().text.get_untracked();
        assert!(state.changes_held(), "{unlock}: held while locked");
        assert!(
            host.heard
                .get_untracked()
                .iter()
                .all(|heard| heard != &shown),
            "{unlock}: nothing reached the host while held"
        );
        // The host takes its locks away (as the component's effects do).
        host.ranges.set(Vec::new());
        host.paths.set(Vec::new());
        state.place_ranges();
        state.locks_changed_by_host();
        assert!(!state.changes_held(), "{unlock}: released");
        assert_eq!(
            host.heard.get_untracked().last(),
            Some(&shown),
            "{unlock}: the held edit reaches the host at once"
        );
        // Taking the locks away again re-sends nothing.
        let count = host.heard.get_untracked().len();
        state.locks_changed_by_host();
        assert_eq!(
            host.heard.get_untracked().len(),
            count,
            "{unlock}: sent once"
        );
    }
}

/// P1: ranges the host gives while an edit is held name places in the
/// host's text, which the edit has not reached: they lock (and mark) the
/// text they name there, followed to the text on screen, not the places
/// of the same numbers on screen.
#[test]
fn ranges_given_while_an_edit_is_held_lock_the_hosts_text() {
    let text = "listen: 7835\npublic_address: tunnel.example.com\n";
    let _owner = Owner::new();
    _owner.set();
    let host = Host {
        paths: RwSignal::new(Vec::new()),
        ranges: RwSignal::new(vec![TextRange::new(
            TextPosition::new(1, 2),
            TextPosition::new(1, 12),
        )]),
        heard: RwSignal::new(Vec::new()),
    };
    let state = mount(text, &host);
    // A key renamed past a range covering part of it is held: not sent,
    // so the host is a text behind the screen.
    assert!(write(state, 0..0, "por"), "the edit is taken");
    assert!(state.changes_held(), "held");
    // The host locks `n: ` of `listen: 7835`, in its own text.
    host.ranges.set(vec![TextRange::new(
        TextPosition::new(1, 6),
        TextPosition::new(1, 9),
    )]);
    state.place_ranges();
    let shown = state.doc().text.get_untracked();
    let units: Vec<u16> = shown.encode_utf16().collect();
    let locked: Vec<String> = state.protect().protected.with_untracked(|protected| {
        protected
            .locks
            .iter()
            .filter(|lock| lock.pointer.is_none())
            .flat_map(|lock| lock.ranges.iter())
            .map(|range| String::from_utf16_lossy(&units[range.clone()]))
            .collect()
    });
    assert_eq!(locked, ["n: "], "in {shown:?}");
}
