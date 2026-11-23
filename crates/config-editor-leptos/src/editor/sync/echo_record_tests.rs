//! A destroyed editor's echo record handed to the next editor made for
//! the same document: under the host's name for the slot (`echo_key`, a
//! conditional render), and by a documents view's id. A v0.1.0 host (no
//! `value_revision`) hands each text back late.

use std::sync::{Arc, Mutex};

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::handle::EditorHandle;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::{
    Handed, HostChannel, Lists, follow_value, kept_echoes, mount_channel, record_echoes,
    save_on_unmount,
};

/// An editor with `echoes`, following the host's `value` as the
/// component does, its echo record kept as the component keeps it.
pub(crate) struct EchoingEditor {
    owner: Owner,
    state: EditorState,
    /// What `on_change` told the host.
    pub(crate) heard: Arc<Mutex<Vec<String>>>,
}

impl EchoingEditor {
    /// Mounted on `value` (and on `handle`, a documents view's editor:
    /// it takes the record the view hands on there), named `echo_key`.
    pub(crate) fn mount(
        value: RwSignal<String>,
        handle: Option<EditorHandle>,
        echo_key: Option<&str>,
    ) -> Self {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let told = heard.clone();
        // As the component starts it: the record of the editor that showed
        // the document before taken over as it mounts.
        let (channel, handed, text) = mount_channel(
            None,
            Handed {
                echoes: true,
                echo_key,
                handed: handle.and_then(EditorHandle::take_handed),
                revision: None,
            },
            value.get_untracked(),
        );
        let (owner, state) = TestEditor {
            on_change: Some(Callback::new(move |text: String| {
                told.lock().expect("heard").push(text);
            })),
            channel,
            ..TestEditor::new(Language::YAML, &text)
        }
        .mount();
        if let Some(handle) = handle {
            handle.attach(state);
        }
        record_echoes(state, true, handed, None);
        follow_value(state, value.into(), MaybeProp::default(), Lists::default());
        save_on_unmount(state, echo_key.map(str::to_string));
        Self {
            owner,
            state,
            heard,
        }
    }

    /// Types `text` at the end of the document's first line.
    pub(crate) fn type_at_line_end(&self, text: &str) {
        let before = self.state.doc().text.get_untracked();
        let at = before.find('\n').unwrap_or(before.len());
        let mut after = before.clone();
        after.insert_str(at, text);
        let end = at + text.len();
        let typed = Edit::input_units(
            before,
            (at, at),
            after,
            (end, end),
            "insertText".into(),
            0.0,
        );
        assert!(self.state.commit(typed));
    }

    /// The document on screen.
    pub(crate) fn text(&self) -> String {
        self.state.doc().text.get_untracked()
    }

    /// The editor goes (the component unmounts).
    pub(crate) fn unmount(self) {
        self.owner.cleanup();
    }
}

/// Hidden after typing `x`, the host still to hand it back, and shown
/// again as a new editor (named `shown_as`) on the host's older text.
fn hidden_and_shown(
    value: RwSignal<String>,
    hidden_as: Option<&str>,
    shown_as: Option<&str>,
) -> EchoingEditor {
    let gone = EchoingEditor::mount(value, None, hidden_as);
    gone.type_at_line_end("x");
    assert_eq!(*gone.heard.lock().expect("heard"), ["a: 1x\n"]);
    gone.unmount();
    EchoingEditor::mount(value, None, shown_as)
}

/// P4, a v0.1.0 host: a conditional render shows a new editor with the
/// same `echo_key`. It takes over the gone editor's texts on their way and
/// shows the newest at once, so what is typed in it goes on from the gone
/// editor's typing, and their late echo keeps both.
#[test]
fn a_new_editor_with_the_same_echo_key_keeps_the_typing() {
    let value = RwSignal::new("a: 1\n".to_string());
    let shown = hidden_and_shown(value, Some("typed"), Some("typed"));
    assert_eq!(kept_echoes::count_of("typed"), 0, "taken");
    assert_eq!(shown.text(), "a: 1x\n", "the gone editor's typing at once");
    shown.type_at_line_end("y");
    value.set("a: 1x\n".to_string());
    assert_eq!(shown.text(), "a: 1xy\n");
    value.set("a: 1xy\n".to_string());
    assert_eq!(shown.text(), "a: 1xy\n");
    shown.unmount();

    let value = RwSignal::new("a: 1\n".to_string());
    let shown = hidden_and_shown(value, Some("untyped"), Some("untyped"));
    value.set("a: 1x\n".to_string());
    assert_eq!(shown.text(), "a: 1x\n");
}

/// Without a name, or with another one, the new editor knows only the
/// texts it sends: the late echo is the host's own text for it (as
/// before `echo_key`; `value_revision` tells it apart there).
#[test]
fn a_new_editor_without_the_same_echo_key_takes_the_late_echo_as_the_hosts() {
    for (hidden_as, shown_as) in [
        (None, None),
        (Some("one"), Some("other")),
        (Some("named"), None),
    ] {
        let value = RwSignal::new("a: 1\n".to_string());
        let shown = hidden_and_shown(value, hidden_as, shown_as);
        shown.type_at_line_end("y");
        value.set("a: 1x\n".to_string());
        assert_eq!(shown.text(), "a: 1x\n", "{hidden_as:?} then {shown_as:?}");
        shown.unmount();
    }
    // The record left under a name no editor took stays, one per name.
    assert_eq!(kept_echoes::count_of("one"), 1);
    assert_eq!(kept_echoes::count_of("named"), 1);
    // Taken by the next editor with the name, and left again as it goes:
    // the texts are still on their way.
    let value = RwSignal::new("a: 1\n".to_string());
    let shown = EchoingEditor::mount(value, None, Some("one"));
    assert_eq!(kept_echoes::count_of("one"), 0);
    shown.unmount();
    assert_eq!(kept_echoes::count_of("one"), 1);
}

/// The kept records: one per name, the newest; none for a channel with
/// nothing on its way; taken once; and gone when the channel that left
/// one goes on in a new editor (a binding's re-mount took it over).
#[test]
fn a_kept_record_is_one_per_name_and_goes_when_its_texts_come_back_elsewhere() {
    let channel = |text: &str| HostChannel::new(true, text, None, None);
    let quiet = channel("a\n");
    quiet.leave("rules");
    assert_eq!(kept_echoes::count_of("rules"), 0);
    let first = channel("a\n");
    first.sent("ab\n");
    let second = channel("a\n");
    second.sent("ac\n");
    second.sent("acd\n");
    first.leave("rules");
    second.leave("rules");
    assert_eq!(kept_echoes::count_of("rules"), 1);
    assert_eq!(HostChannel::left("rules"), second.handoff());
    assert_eq!(HostChannel::left("rules"), None);
    first.leave("rules");
    first.leave("also");
    first.remount("ab\n");
    assert_eq!(kept_echoes::count_of("rules"), 0);
    assert_eq!(kept_echoes::count_of("also"), 0);
    // A record left with nothing on its way replaces the older one.
    second.leave("rules");
    quiet.leave("rules");
    assert_eq!(HostChannel::left("rules"), None);
}
