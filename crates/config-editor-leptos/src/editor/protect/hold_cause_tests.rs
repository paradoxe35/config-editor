//! P1: a locked field held (it reads otherwise than the host gave it, its
//! text kept) says why in its "Not saved" warning and in the save's
//! reason, for the cause recorded when the hold was decided: a duplicate, a rename (an undo
//! included), a move under another parent, another list item in its place,
//! a comment, a removal, text added into it, a language without spans, and
//! text beside a whole placeholder, a change made before the host locked
//! it. A hold that cannot tell says only that it no longer reads as the
//! host gave it; and the cause is the one recorded, kept through the
//! edits after.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::{Change, Edit};
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::state::{EditorState, Init};
use super::super::test_state::TestEditor;
use super::ProtectInit;
use super::guarantee_tests::replace;
use super::locks::HoldCause;
use super::secret_tests::SECRET;
use super::spanless_tests::{ENV, TEXT as ENV_TEXT};
use super::texts::{Reason, save_held};
use super::whole_guard::Held;
use crate::EditorSettings;
use crate::editor::text::{byte_range, text_size};
use crate::strings::default_strings;

/// The warnings starting "Not saved" of the text on screen, and the
/// reason a save waits.
fn held(state: EditorState) -> (Vec<String>, Option<String>) {
    let text = state.doc().text.get_untracked();
    let problems = untrack(|| {
        state
            .protect()
            .held_problems(state.behavior(), &text, default_strings())
    })
    .into_iter()
    .map(|problem| problem.message)
    .filter(|message| message.starts_with("Not saved"))
    .collect();
    (problems, untrack(|| state.save_waits()))
}

/// Asserts the field is held with `expected` as its warning and the save's
/// reason.
fn assert_held(state: EditorState, expected: &str) {
    let text = state.doc().text.get_untracked();
    let (problems, waits) = held(state);
    assert_eq!(problems, [expected], "the warnings of {text:?}");
    assert_eq!(waits.as_deref(), Some(expected), "the save of {text:?}");
}

/// Replaces the first `old` of the text on screen with `new`.
fn change(state: EditorState, old: &str, new: &str) {
    let text = state.doc().text.get_untracked();
    let at = text.find(old).expect("the text to change");
    replace(state, at..at + old.len(), new);
    assert_ne!(
        state.doc().text.get_untracked(),
        text,
        "the edit {old:?} to {new:?} is taken"
    );
}

/// Replaces every `old` of the text on screen with `new` as the Replace
/// all command does: one edit, each match its own replacement.
fn replace_every(state: EditorState, old: &str, new: &str) {
    let before = state.doc().text.get_untracked();
    let matches = before
        .match_indices(old)
        .map(|(at, _)| (byte_range(at..at + old.len()), new.to_string()));
    let change =
        Change::replacing(text_size(before.len()), matches).expect("the matches fit the text");
    assert!(
        state.commit(Edit::command_units(before.clone(), change, (0, 0))),
        "the Replace all of {old:?} is taken"
    );
    assert_ne!(state.doc().text.get_untracked(), before);
}

fn mount(language: Language, text: &str, paths: &[&str]) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(language, text);
    editor.read_only_paths = paths.iter().map(|path| FieldPath::from(*path)).collect();
    editor.mount()
}

/// HCL repeats a block for a list of them; YAML, JSON and TOML refuse a
/// key written twice (the text does not parse, so nothing is held).
#[test]
fn a_block_written_twice_in_hcl_is_named_a_duplicate() {
    let text = "services {\n  svc {\n    port = 9\n  }\n}\n";
    let (_owner, state) = mount(Language::HCL, text, &["/services/svc/port"]);
    let at = text.rfind("}\n").expect("the end");
    replace(state, at..at, "  svc {\n    port = 9\n  }\n");
    assert_held(
        state,
        "Not saved: the read-only field /services/svc/port is written twice, and the other copy is read in its place; remove the copy to save",
    );
}

#[test]
fn a_block_written_twice_in_xml_is_named_a_duplicate() {
    let text = "<root>\n  <svc>\n    <port>9</port>\n  </svc>\n</root>\n";
    let (_owner, state) = mount(Language::XML, text, &["/root/svc/port"]);
    let at = text.find("</root>").expect("the end");
    replace(state, at..at, "  <svc>\n    <port>9</port>\n  </svc>\n");
    assert_held(
        state,
        "Not saved: the read-only field /root/svc/port is written twice, and the other copy is read in its place; remove the copy to save",
    );
}

#[test]
fn a_key_renamed_by_typing_is_named() {
    let text = "listen: 7835\npublic_address: tunnel.example.com\n";
    let mut editor = TestEditor::new(Language::YAML, text);
    // `public_address`'s value.
    editor.read_only_ranges = vec![TextRange::new(
        TextPosition::new(2, 17),
        TextPosition::new(2, 35),
    )];
    let (_owner, state) = editor.mount();
    change(state, "public_address", "tpublic_address");
    assert_held(
        state,
        "Not saved: a key holding the read-only field /public_address was renamed; give the key its old name back to save",
    );
}

/// Each match of a Replace all is a piece of its own: the one in the
/// locked field's key renames it, the others (in a comment, in another
/// value) leave it be.
#[test]
fn a_key_renamed_by_a_replace_all_among_other_matches_is_named() {
    let text = "# public\nlisten: 7835\npublic_address: tunnel.example.com\nmode: public\n";
    let mut editor = TestEditor::new(Language::YAML, text);
    // `public_address`'s value.
    editor.read_only_ranges = vec![TextRange::new(
        TextPosition::new(3, 17),
        TextPosition::new(3, 35),
    )];
    let (_owner, state) = editor.mount();
    replace_every(state, "public", "pub");
    assert_eq!(
        state.doc().text.get_untracked(),
        "# pub\nlisten: 7835\npub_address: tunnel.example.com\nmode: pub\n"
    );
    assert_held(
        state,
        "Not saved: a key holding the read-only field /public_address was renamed; give the key its old name back to save",
    );
}

/// A Replace all whose matches lie around a locked field without touching
/// its keys renames another element to its parent's name: a copy written,
/// not a rename.
#[test]
fn an_element_renamed_by_a_replace_all_to_a_locked_ones_name_is_named_a_duplicate() {
    let text = "<root>\n  <tmp>\n    <port>9</port>\n  </tmp>\n  <svc>\n    <port>9</port>\n  </svc>\n  <note>tmp</note>\n</root>\n";
    let (_owner, state) = mount(Language::XML, text, &["/root/svc/port"]);
    replace_every(state, "tmp", "svc");
    assert_held(
        state,
        "Not saved: the read-only field /root/svc/port is written twice, and the other copy is read in its place; remove the copy to save",
    );
}

#[test]
fn a_key_renamed_by_an_undo_is_named() {
    let owner = Owner::new();
    owner.set();
    let text = "listen: 7835\npublic_address: tunnel.example.com\n";
    let ranges = RwSignal::new(Vec::<TextRange>::new());
    let (state, _) = EditorState::new(Init {
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
        host_on_change: Some(Callback::new(|_: String| {})),
        channel: None,
        protect: ProtectInit {
            paths: None,
            secrets: None,
            foreign: None,
        },
        on_save: None,
        on_baseline_conflict: None,
        clipboard: None,
    });
    crate::editor::sync::follow_lists(
        state,
        crate::editor::sync::Lists {
            read_only_ranges: Some(ranges.into()),
            ..crate::editor::sync::Lists::default()
        },
    );
    change(state, "7835", "78350");
    change(state, "public_address", "tpublic_address");
    // The host locks the renamed field's value; undoing the typed letter
    // renames its key back (to a text that is not the host's: the other
    // edit stays).
    ranges.set(vec![TextRange::new(
        TextPosition::new(2, 18),
        TextPosition::new(2, 36),
    )]);
    state.place_ranges();
    state.locks_changed_by_host();
    let edit = state.history_edit(false).expect("an undo");
    assert!(state.commit(edit), "the undo is taken");
    assert_eq!(
        state.doc().text.get_untracked(),
        "listen: 78350\npublic_address: tunnel.example.com\n"
    );
    assert_held(
        state,
        "Not saved: a key holding the read-only field /tpublic_address was renamed; give the key its old name back to save",
    );
}

#[test]
fn a_field_moved_under_another_parent_is_named() {
    let (_owner, state) = mount(Language::YAML, "a:\n  x: 1\nb: 2\n", &["/a/x"]);
    change(state, "  x: 1", "c:\n  x: 1");
    assert_held(
        state,
        "Not saved: the read-only field /a/x moved under another parent; move it back to save",
    );
}

#[test]
fn another_list_item_in_its_place_is_named() {
    let (_owner, state) = mount(Language::YAML, "hosts:\n  - a\n  - b\n", &["/hosts/1"]);
    change(state, "  - a", "  - z\n  - a");
    assert_held(
        state,
        "Not saved: the read-only field /hosts/1 is now another item of its list, because items were added, removed or moved before it; put the items back in order to save",
    );
}

#[test]
fn a_commented_out_field_is_named() {
    let (_owner, state) = mount(Language::YAML, "id: srv-42\nlisten: 1\n", &["/id"]);
    change(state, "id: srv-42", "# id: srv-42");
    assert_held(
        state,
        "Not saved: the read-only field /id is commented out; remove the comment mark to save",
    );
}

#[test]
fn a_field_the_text_no_longer_has_is_named_removed() {
    let owner = Owner::new();
    owner.set();
    let text = "name: a\nport: 80\n";
    let paths = RwSignal::new(Vec::<FieldPath>::new());
    let (state, _) = EditorState::new(Init {
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
        host_on_change: None,
        channel: None,
        protect: ProtectInit {
            paths: Some(paths.into()),
            secrets: None,
            foreign: None,
        },
        on_save: None,
        on_baseline_conflict: None,
        clipboard: None,
    });
    change(state, "port: 80\n", "");
    // The host names the path after the line was cut.
    paths.set(vec![FieldPath::from("/port")]);
    assert_held(
        state,
        "Not saved: the read-only field /port is no longer in the text; write it back as the host gave it to save",
    );
}

#[test]
fn text_added_into_a_locked_block_is_named() {
    let (_owner, state) = mount(
        Language::YAML,
        "limits:\n  burst: 5\nname: x\n",
        &["/limits"],
    );
    change(state, "name: x", "  extra: 1\nname: x");
    assert_held(
        state,
        "Not saved: the read-only field /limits now reads text that was added into it or beside it; remove that text to save",
    );
}

/// Held at the keystroke that makes the block read the key, far from where
/// the block ended before: the text typed is inside the block now.
#[test]
fn a_key_typed_into_a_locked_block_one_character_at_a_time_is_named() {
    let text = "limits:\n  burst: 5\nname: x\n";
    let (_owner, state) = mount(Language::YAML, text, &["/limits"]);
    let mut at = text.find("\nname").expect("the block's end");
    for typed in "\n  extra: 1".chars() {
        let typed = typed.to_string();
        replace(state, at..at, &typed);
        at += typed.len();
    }
    assert_eq!(
        state.doc().text.get_untracked(),
        "limits:\n  burst: 5\n  extra: 1\nname: x\n"
    );
    assert_held(
        state,
        "Not saved: the read-only field /limits now reads text that was added into it or beside it; remove that text to save",
    );
}

#[test]
fn a_value_reading_otherwise_without_spans_is_named() {
    let mut editor = TestEditor::new(Language::new(&ENV), ENV_TEXT);
    editor.read_only_paths = vec![FieldPath::from("/ID")];
    let (_owner, state) = editor.mount();
    change(state, "NAME=a", "NAMEa");
    change(state, "ID=srv-1", "ID=srv-2");
    change(state, "NAMEa", "NAME=b");
    assert_held(
        state,
        "Not saved: the read-only field /ID now reads another value than the host gave; put its value back as it was to save",
    );
}

#[test]
fn text_beside_a_whole_placeholder_is_named() {
    let host = format!("name: a\npassword: '{SECRET}'\n");
    let (_owner, state) = TestEditor::new(Language::YAML, &host).mount();
    let at = host.find("password").expect("the key");
    replace(state, at..at, "x: ");
    state.protect().secrets.held.set_value(Some(Held {
        field: "/password".into(),
        tokens: vec![SECRET.into()],
    }));
    let expected = "Not saved: /password now reads other text beside its secret placeholder; remove that text so the placeholder stands alone to save";
    let (problems, waits) = held(state);
    assert_eq!(problems, [expected]);
    assert_eq!(waits.as_deref(), Some(expected));
}

/// A path the host names after the user changed its value: the text reads
/// it otherwise than the host's text does.
#[test]
fn a_field_changed_before_the_host_locked_it_is_named() {
    let owner = Owner::new();
    owner.set();
    let text = "name: a\nport: 80\n";
    let paths = RwSignal::new(Vec::<FieldPath>::new());
    let (state, _) = EditorState::new(Init {
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
        host_on_change: None,
        channel: None,
        protect: ProtectInit {
            paths: Some(paths.into()),
            secrets: None,
            foreign: None,
        },
        on_save: None,
        on_baseline_conflict: None,
        clipboard: None,
    });
    change(state, "port: 80", "port: 81");
    paths.set(vec![FieldPath::from("/port")]);
    assert_held(
        state,
        "Not saved: /port was changed before the host made it read-only; put it back as the host gave it to save",
    );
}

/// The cause is the one recorded when the field was held, not read again
/// off the text: a rename, then an edit far from it and its line commented
/// out (which, held afresh, would read as commented), still says renamed.
#[test]
fn a_hold_keeps_its_recorded_cause_through_the_edits_after() {
    let text = "listen: 7835\npublic_address: tunnel.example.com\n";
    let mut editor = TestEditor::new(Language::YAML, text);
    editor.read_only_ranges = vec![TextRange::new(
        TextPosition::new(2, 17),
        TextPosition::new(2, 35),
    )];
    let (_owner, state) = editor.mount();
    change(state, "public_address", "tpublic_address");
    let cause = || {
        state
            .protect()
            .protected
            .with_untracked(|protected| protected.held.cause("/public_address"))
    };
    assert_eq!(cause(), Some(HoldCause::Renamed));
    change(state, "7835", "78350");
    assert_eq!(
        cause(),
        Some(HoldCause::Renamed),
        "after an edit far from it"
    );
    change(state, "tpublic_address", "# tpublic_address");
    assert_eq!(cause(), Some(HoldCause::Renamed), "after commenting it out");
    // The range's own field there (`/tpublic_address`) is held too, for
    // the edit that commented it out.
    let (problems, _) = held(state);
    let renamed = "Not saved: a key holding the read-only field /public_address was renamed; give the key its old name back to save";
    assert!(
        problems.iter().any(|problem| problem == renamed),
        "{problems:#?}"
    );
}

/// Each cause has its own text; one a hold cannot tell guesses none.
#[test]
fn every_cause_has_its_own_wording() {
    let strings = default_strings();
    let message = |cause: HoldCause, reason: Reason| save_held(strings, "/a", &reason, cause);
    assert_eq!(
        message(HoldCause::RangeMoved, Reason::Range),
        "Not saved: the read-only text the host gave no longer reads as /a; put that text back where it was to save"
    );
    assert_eq!(
        message(
            HoldCause::Unclear,
            Reason::Given("Set by the server".into())
        ),
        "Not saved: the read-only field /a no longer reads as the host gave it (Set by the server); put it back as it was to save"
    );
    let all = [
        HoldCause::Duplicate,
        HoldCause::Renamed,
        HoldCause::Moved,
        HoldCause::ListItem,
        HoldCause::Commented,
        HoldCause::Removed,
        HoldCause::Added,
        HoldCause::Spanless,
        HoldCause::RangeMoved,
        HoldCause::ChangedBeforeLock,
        HoldCause::Unclear,
    ];
    let texts: std::collections::HashSet<String> = all
        .into_iter()
        .map(|cause| message(cause, Reason::Field))
        .collect();
    assert_eq!(
        texts.len(),
        all.len(),
        "each cause is worded apart: {texts:#?}"
    );
    assert!(
        texts.iter().all(|text| text.starts_with("Not saved: ")),
        "{texts:#?}"
    );
}

/// A refused rename of a key holding a read-only field says why only when
/// something says more than that the field is read-only.
#[test]
fn a_refused_rename_names_a_reason_only_when_one_is_given() {
    let strings = default_strings();
    assert_eq!(Reason::Field.holding(strings), "Holds a read-only field");
    assert_eq!(Reason::Range.holding(strings), "Holds a read-only field");
    assert_eq!(
        Reason::Given("Set by your plan".into()).holding(strings),
        "Holds a read-only field: Set by your plan"
    );
}
