//! P1: a text the guard holds reaches the host by no route. Every way a
//! text leaves the editor (a change event typed in the host's language, a
//! typing pause's conversion, a switch back with edits unsaved, the
//! unmount's save, `save()`, `value()`, `unified_diff()`) asks the one
//! gate (see `gate`), so each hold kind holds on each route.

use leptos::prelude::*;

use super::super::edit::Edit;
use super::super::handle::EditorHandle;
use super::super::host_editing::SaveRequest;
use super::super::languages::Language;
use super::super::paths::FieldPath;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::{Route, convert_now, save_pending, switch_to};

/// What makes the text on screen one the host may not have.
#[derive(Clone, Copy, Debug)]
enum Hold {
    /// The locked key written again: a duplicate shadows the locked value.
    ShadowedLock,
    /// A placeholder the host gave as a field's whole value, with text
    /// beside it.
    GluedPlaceholder,
}

/// One way a text leaves the editor.
#[derive(Clone, Copy, Debug)]
enum Way {
    Typed,
    Pause,
    SwitchBack,
    Unmount,
    Save,
    Value,
    UnifiedDiff,
}

const TOKEN: &str = "«secret 3f2a91c0»";

/// The host's text. A placeholder the host glued text to itself is its
/// own; an edit beside it is the user's, and waits.
fn host(hold: Hold) -> String {
    match hold {
        Hold::ShadowedLock => "port: 80\nname: a\n".to_string(),
        Hold::GluedPlaceholder => format!("key: {TOKEN}Z\nname: a\n"),
    }
}

/// The text on screen, in `language`, holding `hold`.
fn held_text(hold: Hold, language: Language) -> String {
    match (hold, language.label()) {
        (Hold::ShadowedLock, "YAML") => "port: 80\nname: a\nport: 81\n".to_string(),
        (Hold::ShadowedLock, _) => {
            "{\n  \"port\": 80,\n  \"name\": \"a\",\n  \"port\": 81\n}\n".to_string()
        }
        (Hold::GluedPlaceholder, "YAML") => format!("key: {TOKEN}Z\nname: b\n"),
        (Hold::GluedPlaceholder, _) => {
            format!("{{\n  \"key\": \"{TOKEN}Z\",\n  \"name\": \"b\"\n}}\n")
        }
    }
}

struct Mounted {
    _owner: Owner,
    state: EditorState,
    handle: EditorHandle,
    heard: RwSignal<Vec<String>>,
}

fn mount(hold: Hold, edit_as: Option<Language>) -> Mounted {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(Language::YAML, &host(hold));
    editor.edit_as = edit_as;
    editor.on_change = Some(Callback::new(move |text: String| {
        heard.update(|heard| heard.push(text));
    }));
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        heard.update(|heard| heard.push(request.text));
    }));
    match hold {
        Hold::ShadowedLock => editor.read_only_paths = vec![FieldPath::from("/port")],
        Hold::GluedPlaceholder => {
            editor.secrets =
                Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{8}»"));
        }
    }
    let (owner, state) = editor.mount();
    if let Hold::GluedPlaceholder = hold {
        // The host gave it whole: nothing may be written beside it.
        state.protect().secrets.known.update(|known| {
            for known in known.iter_mut() {
                known.whole = Some(TOKEN.to_string());
            }
        });
    }
    let handle = EditorHandle::new();
    handle.attach(state);
    Mounted {
        _owner: owner,
        state,
        handle,
        heard,
    }
}

/// Puts `text` on screen as the user's paste. Whether the guard took it.
fn paste(state: EditorState, text: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let end = before.encode_utf16().count();
    let caret = text.encode_utf16().count();
    state.commit(Edit::input(
        before,
        (0, end),
        text.to_string(),
        (caret, caret),
        "insertFromPaste".into(),
        0.0,
    ))
}

/// What reached the host by `way`, the text on screen (in a JSON view of
/// the YAML host with `view`) holding `hold`; `None` when the guard refused
/// the edit itself (the hold never formed).
fn reached(hold: Hold, way: Way, view: bool) -> Option<Vec<String>> {
    let language = if view { Language::JSON } else { Language::YAML };
    let mounted = mount(hold, view.then_some(Language::JSON));
    let Mounted {
        state,
        handle,
        heard,
        ..
    } = &mounted;
    let state = *state;
    if !paste(state, &held_text(hold, language)) {
        return None;
    }
    // The change event of the keystroke carries text the host itself glued
    // (see `a_held_text_reaches_the_host_by_no_route`): the route asked
    // for after it is what is judged.
    if let (Hold::GluedPlaceholder, false) = (hold, view) {
        heard.set(Vec::new());
    }
    let mut out = Vec::new();
    match way {
        Way::Typed => {}
        Way::Pause => {
            let text = state.doc().text.get_untracked();
            convert_now(state, text, Language::JSON);
        }
        Way::SwitchBack => switch_to(state, Language::YAML),
        Way::Unmount => save_pending(state),
        Way::Save => {
            state.save();
        }
        Way::Value => out.extend(handle.value()),
        Way::UnifiedDiff => {
            #[cfg(feature = "diff")]
            out.extend(handle.unified_diff(3).filter(|diff| !diff.is_empty()));
        }
    }
    out.extend(heard.get_untracked());
    Some(out)
}

#[test]
fn a_held_text_reaches_the_host_by_no_route() {
    let mut leaks = Vec::new();
    let mut refused = Vec::new();
    let mut held = 0;
    let in_host = [Way::Typed, Way::Save, Way::Value, Way::UnifiedDiff];
    let in_view = [
        Way::Pause,
        Way::SwitchBack,
        Way::Unmount,
        Way::Save,
        Way::Value,
        Way::UnifiedDiff,
    ];
    for hold in [Hold::ShadowedLock, Hold::GluedPlaceholder] {
        let ways = in_host.iter().map(|way| (*way, false));
        for (way, view) in ways.chain(in_view.iter().map(|way| (*way, true))) {
            // A keystroke's change event reads no document (P5): text the
            // host itself glued to a placeholder is its own, and only the
            // routes asking for the document wait on it. The guard refuses
            // the user's own glue at the keystroke.
            if let (Hold::GluedPlaceholder, Way::Typed) = (hold, way) {
                continue;
            }
            match reached(hold, way, view) {
                None => refused.push(format!("{hold:?} by {way:?} (view: {view})")),
                Some(out) if out.is_empty() => held += 1,
                Some(out) => leaks.push(format!("{hold:?} by {way:?} (view: {view}): {out:?}")),
            }
        }
    }
    assert!(leaks.is_empty(), "reached the host:\n{}", leaks.join("\n"));
    // Each hold forms on every route (none is refused at the keystroke).
    assert!(refused.is_empty(), "refused: {refused:?}");
    assert_eq!(held, 19);
}

/// A text the guard takes passes the gate by every route that hands the
/// document out, and reaches `on_change` as typed.
#[test]
fn a_text_the_guard_takes_reaches_the_host_by_every_route() {
    for route in Route::ALL {
        let mounted = mount(Hold::ShadowedLock, None);
        assert!(paste(mounted.state, "port: 80\nname: b\n"));
        assert_eq!(
            mounted.state.to_host(*route).ok().as_deref(),
            Some("port: 80\nname: b\n"),
            "{route:?}"
        );
        assert_eq!(mounted.heard.get_untracked(), ["port: 80\nname: b\n"]);
    }
}
