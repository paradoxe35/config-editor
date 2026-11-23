//! The host's own change (`applyChanges`) through the edit pipeline: made
//! on its text now or on an older revision (rebased over the user's edits),
//! followed exactly by the locks, never an undo step.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;

use super::{ChangeError, HostEdits, apply_host_changes};
use crate::editor::edit::Edit;
use crate::editor::languages::Language;
use crate::editor::paths::{TextPosition, TextRange};
use crate::editor::state::EditorState;
use crate::editor::test_state::TestEditor;

/// An editor on `text` with `locked` (lines and columns) read-only, and
/// what `on_change` heard.
fn mounted(text: &str, locked: Vec<TextRange>) -> (Owner, EditorState, Arc<Mutex<Vec<String>>>) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let told = heard.clone();
    let (owner, state) = TestEditor {
        on_change: Some(Callback::new(move |text: String| {
            told.lock().expect("heard").push(text);
        })),
        read_only_ranges: locked,
        ..TestEditor::new(Language::YAML, text)
    }
    .mount();
    (owner, state, heard)
}

/// The user types `insert` at byte `at`.
fn typed(state: EditorState, byte: usize, insert: &str) {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{insert}{}", &before[..byte], &before[byte..]);
    state.commit(Edit::new_units(
        before.clone(),
        (byte, byte),
        after,
        byte + insert.len(),
    ));
}

fn locked_text(state: EditorState) -> Vec<String> {
    let text = state.doc().text.get_untracked();
    state.protect().ranges.with_untracked(|ranges| {
        ranges
            .iter()
            .map(|range| text[usize::from(range.start())..usize::from(range.end())].to_string())
            .collect()
    })
}

fn line(line: usize, from: usize, to: usize) -> TextRange {
    TextRange::new(TextPosition::new(line, from), TextPosition::new(line, to))
}

fn utf16(from: usize, to: usize, insert: &str) -> HostEdits {
    HostEdits::Utf16(vec![(from, to, insert.to_string())])
}

#[test]
fn a_change_on_the_hosts_text_moves_the_locks_and_is_no_undo_step() {
    let (_owner, state, heard) = mounted("a: 1\nb: 2\n", vec![line(2, 1, 5)]);
    let revision = apply_host_changes(state, &utf16(0, 0, "top: 0\n"), None).expect("applied");
    assert!(revision.is_some());
    assert_eq!(state.doc().text.get_untracked(), "top: 0\na: 1\nb: 2\n");
    assert_eq!(locked_text(state), ["b: 2"]);
    // The host knows its own change: nothing told, nothing to undo.
    assert!(heard.lock().expect("heard").is_empty());
    assert!(
        !state
            .doc()
            .history
            .with_untracked(|history| history.can_undo())
    );
}

#[test]
fn a_change_on_an_older_revision_is_rebased_over_the_typing_and_told() {
    let (_owner, state, heard) = mounted("a: 1\nb: 2\n", vec![line(2, 1, 5)]);
    let base = state
        .sync()
        .link(|link| link.log.host_revision())
        .expect("mounted");
    // The user types at the end of `a`'s value, then the host, on the text
    // before, changes `a`'s value and adds a line above `b`.
    typed(state, 4, "1");
    let edits = HostEdits::Utf16(vec![(3, 4, "7".to_string()), (5, 5, "c: 3\n".to_string())]);
    apply_host_changes(state, &edits, Some(&base.into())).expect("applied");
    assert_eq!(state.doc().text.get_untracked(), "a: 71\nc: 3\nb: 2\n");
    assert_eq!(locked_text(state), ["b: 2"]);
    assert_eq!(
        heard.lock().expect("heard").last().map(String::as_str),
        Some("a: 71\nc: 3\nb: 2\n")
    );
}

/// A host that keeps no revision (its store has not heard the typing yet)
/// names the text it made its change on: rebased over the typing, as with
/// that text's revision; a text the editor cannot name changes nothing.
#[test]
fn a_change_named_by_its_text_is_rebased_over_the_typing() {
    let loaded = "a: 1\nb: 2\n";
    let (_owner, state, _) = mounted(loaded, vec![line(2, 1, 5)]);
    typed(state, 4, "1");
    let edits = HostEdits::Utf16(vec![(3, 4, "7".to_string()), (5, 5, "c: 3\n".to_string())]);
    apply_host_changes(state, &edits, Some(&loaded.into())).expect("applied");
    assert_eq!(state.doc().text.get_untracked(), "a: 71\nc: 3\nb: 2\n");
    assert_eq!(locked_text(state), ["b: 2"]);
    // The document itself is a text the host may name too.
    let document = state.doc().text.get_untracked();
    apply_host_changes(
        state,
        &utf16(0, 0, "top: 0\n"),
        Some(&document.as_str().into()),
    )
    .expect("applied");
    assert_eq!(
        state.doc().text.get_untracked(),
        format!("top: 0\n{document}")
    );
    assert_eq!(
        apply_host_changes(state, &utf16(0, 0, "x"), Some(&"other: 1\n".into())),
        Err(ChangeError::UnknownText)
    );
}

#[test]
fn a_base_never_sent_or_out_of_date_changes_nothing() {
    let (_owner, state, _) = mounted("a: 1\n", Vec::new());
    assert_eq!(
        apply_host_changes(state, &utf16(0, 0, "x"), Some(&u64::MAX.into())),
        Err(ChangeError::Unknown(u64::MAX))
    );
    assert_eq!(
        apply_host_changes(state, &utf16(4, 3, "x"), None),
        Err(ChangeError::BadRanges)
    );
    assert_eq!(state.doc().text.get_untracked(), "a: 1\n");
}

/// A small seeded generator (xorshift).
struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        usize::try_from(self.0 % bound.max(1) as u64).unwrap_or(0)
    }
}

fn seed() -> u64 {
    std::env::var("HOST_LOG_SEED")
        .ok()
        .and_then(|seed| seed.parse().ok())
        .unwrap_or(0x7f4a_7c15_2545_f491)
}

fn cases(default: usize) -> usize {
    std::env::var("HOST_LOG_CASES")
        .ok()
        .and_then(|cases| cases.parse().ok())
        .unwrap_or(default)
}

/// A host change on an older revision gives the text the user's edits
/// make of the host's changed text (both orders meet), and the lock stays
/// on its text.
#[test]
fn host_changes_on_older_revisions_meet_the_users_edits() {
    let mut rng = Rng(seed());
    for case in 0..cases(40) {
        let (_owner, state, _) = mounted("a: 1\nkeep: me\nz: 9\n", vec![line(2, 1, 9)]);
        let base = state
            .sync()
            .link(|link| link.log.host_revision())
            .expect("mounted");
        let base_text = state.doc().text.get_untracked();
        // The user types a few letters outside the lock.
        for _ in 0..rng.below(4) {
            let text = state.doc().text.get_untracked();
            let at = [0, 2, text.len()][rng.below(3)];
            typed(state, at, ["x", "\n", "yy"][rng.below(3)]);
        }
        // The host inserts at a line start of the base text.
        let at = [0, 5, base_text.len()][rng.below(3)];
        let insert = ["h: 1\n", "#\n", ""][rng.below(3)];
        apply_host_changes(state, &utf16(at, at, insert), Some(&base.into())).expect("applied");
        let text = state.doc().text.get_untracked();
        assert_eq!(locked_text(state), ["keep: me"], "case {case}: {text:?}");
        assert!(text.contains(insert), "case {case}: {text:?}");
        assert_eq!(text.matches("keep: me").count(), 1, "case {case}");
    }
}

/// While editing as another language the host's change applies to its
/// text, and the call names the new text's revision (it changed).
#[cfg(feature = "json")]
#[test]
fn a_host_change_while_editing_as_another_language_names_its_revision() {
    let (_owner, state) = TestEditor {
        edit_as: Some(Language::JSON),
        ..TestEditor::new(Language::YAML, "a: 1\n")
    }
    .mount();
    let revision = apply_host_changes(state, &utf16(0, 0, "b: 2\n"), None).expect("applied");
    assert!(revision.is_some());
    assert_eq!(*state.sync().host_text.get_value(), *"b: 2\na: 1\n");
}

/// A language's text with a field `a` (value `1`) and `b`, where each
/// `z` typed renames `a` (bytes of the text as typed so far), and a
/// line the host puts in at a byte of the text loaded.
struct Renamed {
    language: Language,
    text: &'static str,
    renames: &'static [usize],
    host_at: usize,
    host_line: &'static str,
    /// Whether typing in the key is taken (XML refuses it: the element's
    /// tags hold the locked value, so no rename is ever held).
    taken: bool,
}

fn renamed_cases() -> Vec<Renamed> {
    let mut cases = vec![Renamed {
        language: Language::YAML,
        text: "a: 1\nb: 2\n",
        renames: &[0],
        host_at: 0,
        host_line: "top: 0\n",
        taken: true,
    }];
    #[cfg(feature = "json")]
    cases.push(Renamed {
        language: Language::JSON,
        text: "{\n  \"a\": 1,\n  \"b\": 2\n}\n",
        renames: &[5],
        host_at: 2,
        host_line: "  \"top\": 0,\n",
        taken: true,
    });
    #[cfg(feature = "toml")]
    cases.push(Renamed {
        language: Language::TOML,
        text: "a = 1\nb = 2\n",
        renames: &[0],
        host_at: 0,
        host_line: "top = 0\n",
        taken: true,
    });
    #[cfg(feature = "hcl")]
    cases.push(Renamed {
        language: Language::HCL,
        text: "a = 1\nb = 2\n",
        renames: &[0],
        host_at: 0,
        host_line: "top = 0\n",
        taken: true,
    });
    #[cfg(feature = "xml")]
    cases.push(Renamed {
        language: Language::XML,
        text: "<root>\n  <a>1</a>\n  <b>2</b>\n</root>\n",
        renames: &[10, 16],
        host_at: 7,
        host_line: "  <top>0</top>\n",
        taken: false,
    });
    cases
}

/// The user types `z` at `byte` (UTF-16 units: the texts are ASCII).
fn type_z(state: EditorState, byte: usize) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}z{}", &before[..byte], &before[byte..]);
    state.commit(Edit::input_units(
        before,
        (byte, byte),
        after,
        (byte + 1, byte + 1),
        "insertText".into(),
        0.0,
    ))
}

/// The user deletes every `z` typed.
fn delete_zs(state: EditorState) {
    let mut now = state.doc().text.get_untracked();
    while let Some(at) = now.find('z') {
        let after = format!("{}{}", &now[..at], &now[at + 1..]);
        assert!(state.commit(Edit::input_units(
            now,
            (at, at + 1),
            after.clone(),
            (at, at),
            "deleteContentBackward".into(),
            0.0,
        )));
        now = after;
    }
}

/// Where `a`'s value `1` is in `case`'s text.
fn value_of_a(case: &Renamed) -> usize {
    case.text.find(">1<").map_or_else(
        || case.text.find(": 1").or(case.text.find("= 1")).expect("1") + 2,
        |at| at + 1,
    )
}

/// An editor on `case`'s text with `a`'s value read-only (a host range),
/// and what `on_change` heard.
fn mounted_locking_a(case: &Renamed) -> (Owner, EditorState, Arc<Mutex<Vec<String>>>) {
    let value = value_of_a(case);
    let line_start = case.text[..value].rfind('\n').map_or(0, |at| at + 1);
    let number = case.text[..value].matches('\n').count() + 1;
    let column = value - line_start + 1;
    let heard = Arc::new(Mutex::new(Vec::new()));
    let told = heard.clone();
    let (owner, state) = TestEditor {
        on_change: Some(Callback::new(move |text: String| {
            told.lock().expect("heard").push(text);
        })),
        read_only_ranges: vec![line(number, column, column + 1)],
        ..TestEditor::new(case.language, case.text)
    }
    .mount();
    (owner, state, heard)
}

/// A key renamed while `a`'s value is locked holds the save; the host's
/// change rebased over that typing (made on its text, or named by its
/// revision) keeps it held: `on_change` hears nothing and no revision names
/// the result, until the user's text reads `a` again. Then it hears the
/// host's change with the user's text. A host that rewrites its own locked
/// value meanwhile has it taken: the lock covers what it wrote.
#[test]
fn a_host_change_over_a_held_rename_keeps_the_save_held() {
    for case in renamed_cases() {
        for (with_revision, rewrite) in [(false, false), (true, false), (false, true), (true, true)]
        {
            let name = format!(
                "{} (revision: {with_revision}, rewrites a: {rewrite})",
                case.language.label()
            );
            let (_owner, state, heard) = mounted_locking_a(&case);
            let base = state
                .sync()
                .link(|link| link.log.host_revision())
                .expect("mounted");
            if !case.taken {
                assert!(!type_z(state, case.renames[0]), "{name}: refused");
                continue;
            }
            for at in case.renames {
                assert!(type_z(state, *at), "{name}: the rename is taken");
            }
            assert!(state.changes_held(), "{name}: the rename holds the save");
            let heard_before = heard.lock().expect("heard").len();
            let base = with_revision.then(|| base.into());
            let at = case.host_at;
            let value = value_of_a(&case);
            let mut edits = vec![(at, at, case.host_line.to_string())];
            let mut expected = case.text.to_string();
            if rewrite {
                edits.push((value, value + 1, "7".to_string()));
                expected.replace_range(value..value + 1, "7");
            }
            expected.insert_str(at, case.host_line);
            let revision = apply_host_changes(state, &HostEdits::Utf16(edits), base.as_ref())
                .expect("applied");
            let text = state.doc().text.get_untracked();
            assert!(text.contains(case.host_line), "{name}: {text:?}");
            assert!(state.changes_held(), "{name}: still held: {text:?}");
            assert!(state.save_waits().is_some(), "{name}: the save waits");
            assert_eq!(revision, None, "{name}: no revision names {text:?}");
            assert_eq!(
                heard.lock().expect("heard").len(),
                heard_before,
                "{name}: on_change heard the renamed text"
            );
            let locked = if rewrite { "7" } else { "1" };
            assert_eq!(locked_text(state), [locked], "{name}: {text:?}");
            // The user takes the renames back: the host hears its change
            // with `a` read as locked.
            delete_zs(state);
            let text = state.doc().text.get_untracked();
            assert!(!state.changes_held(), "{name}: released: {text:?}");
            assert_eq!(
                heard.lock().expect("heard").last(),
                Some(&expected),
                "{name}: the host hears the text released"
            );
        }
    }
}
