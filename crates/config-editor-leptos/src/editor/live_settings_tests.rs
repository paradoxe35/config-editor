//! Each setting switches live on a mounted editor state: the parts that read
//! it follow the signal, and switching back restores them. (What happens on
//! screen, and the effects closing widgets, is checked end to end in
//! `e2e/tests/settings.spec.mjs`.)

use leptos::prelude::*;

use super::edit::Edit;
use super::languages::Language;
use super::state::EditorState;
use super::test_state::TestEditor;
use crate::{EditorSettings, EditorTheme, ThemeColor, ThemeColors};

/// A YAML document with a syntax problem on its last line and a block that
/// folds.
const TEXT: &str = "web:\n  http: 80\nbad: [\n";

fn editor(settings: RwSignal<EditorSettings>) -> (Owner, EditorState) {
    TestEditor {
        settings: settings.into(),
        ..TestEditor::new(Language::YAML, TEXT)
    }
    .mount()
}

/// What every switch the state reads shows now, by setting.
fn observed(state: EditorState) -> Vec<(&'static str, bool)> {
    let settings = state.config().settings;
    vec![
        (
            "diagnostics",
            !state.problems().diagnostics.get_untracked().is_empty(),
        ),
        ("diff", state.changes().summary.get_untracked().hunks > 0),
        (
            "folding",
            !state.fold().fold_ranges.get_untracked().is_empty(),
        ),
        ("completions", settings.completions.get_untracked()),
        ("auto_trigger", settings.auto_trigger.get_untracked()),
        ("highlighting", settings.highlighting.get_untracked()),
        ("auto_closing", settings.auto_closing.get_untracked()),
        ("search", settings.search.get_untracked()),
        ("go_to_line", settings.go_to_line.get_untracked()),
        ("toolbar", settings.toolbar.get_untracked()),
        ("footer", settings.footer.get_untracked()),
        ("styled", settings.styled.get_untracked()),
        (
            "save_shortcut",
            state.editing().save_shortcut.get_untracked(),
        ),
    ]
}

/// The settings with every switch the state reads set to `on`.
fn all(on: bool) -> EditorSettings {
    EditorSettings {
        show_toolbar: on,
        show_footer: on,
        show_diagnostics: on,
        show_diff: on,
        completions: on,
        auto_trigger_completions: on,
        hover_docs: on,
        highlighting: on,
        auto_closing: on,
        search: on,
        go_to_line: on,
        folding: on,
        save_shortcut: on,
        styled: on,
        ..EditorSettings::default()
    }
}

#[test]
fn every_switch_follows_the_signal_both_ways() {
    let settings = RwSignal::new(all(true));
    let (_owner, state) = editor(settings);
    // An edit, so there is a diff against the baseline.
    let after = format!("{TEXT}x: 1\n");
    let caret = after.encode_utf16().count();
    assert!(state.commit(Edit::new(TEXT.into(), (caret, caret), after, caret)));
    let failures = |wanted: bool| -> Vec<String> {
        observed(state)
            .into_iter()
            .filter(|(_, on)| *on != wanted)
            .map(|(name, on)| format!("{name} is {on}, wanted {wanted}"))
            .collect()
    };
    assert_eq!(failures(true), Vec::<String>::new());
    settings.set(all(false));
    assert_eq!(failures(false), Vec::<String>::new());
    settings.set(all(true));
    assert_eq!(failures(true), Vec::<String>::new());
}

/// Turns one setting off.
type SwitchOff = fn(&mut EditorSettings);

/// One switch at a time: each changes only what it is for.
#[test]
fn each_switch_changes_only_its_own_part() {
    let switches: [(&str, SwitchOff); 6] = [
        ("diagnostics", |settings| settings.show_diagnostics = false),
        ("diff", |settings| settings.show_diff = false),
        ("folding", |settings| settings.folding = false),
        ("search", |settings| settings.search = false),
        ("toolbar", |settings| settings.show_toolbar = false),
        ("save_shortcut", |settings| settings.save_shortcut = false),
    ];
    let mut failures = Vec::new();
    for (name, switch_off) in switches {
        let settings = RwSignal::new(all(true));
        let (_owner, state) = editor(settings);
        let after = format!("{TEXT}x: 1\n");
        let caret = after.encode_utf16().count();
        assert!(state.commit(Edit::new(TEXT.into(), (caret, caret), after, caret)));
        settings.update(switch_off);
        for (part, on) in observed(state) {
            if on == (part == name) {
                failures.push(format!("{name} off: {part} is {on}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn theme_and_colors_follow_the_signal() {
    let settings = RwSignal::new(EditorSettings::default());
    let (_owner, state) = editor(settings);
    let live = state.config().settings;
    assert_eq!(live.theme.get_untracked(), EditorTheme::VsCodeDark);
    assert_eq!(live.color_style(), None);
    settings.update(|settings| {
        settings.theme = EditorTheme::Nord;
        settings.colors = ThemeColors::new().set(ThemeColor::Key, "#123456");
    });
    assert_eq!(live.theme.get_untracked(), EditorTheme::Nord);
    assert_eq!(
        live.color_style().as_deref(),
        Some("--config-editor-key: #123456;")
    );
}

#[test]
fn hover_docs_need_a_schema() {
    let settings = RwSignal::new(all(true));
    let (_owner, state) = editor(settings);
    assert!(!state.config().settings.hover_docs.get_untracked());
}

/// The text, caret and undo history are the state's own: no setting
/// touches them.
#[test]
fn switching_keeps_the_text_caret_and_history() {
    let settings = RwSignal::new(all(true));
    let (_owner, state) = editor(settings);
    let after = format!("{TEXT}x: 1\n");
    let caret = after.encode_utf16().count();
    assert!(state.commit(Edit::new(TEXT.into(), (caret, caret), after.clone(), caret)));
    for on in [false, true, false] {
        settings.set(all(on));
        settings.update(|settings| settings.theme = EditorTheme::Dracula);
    }
    assert_eq!(state.doc().text.get_untracked(), after);
    assert_eq!(state.doc().caret.get_untracked(), caret);
    assert!(
        state
            .doc()
            .history
            .with_untracked(|history| history.can_undo())
    );
}

/// The built-in texts follow the `strings` setting live: what views read
/// (`state.text`, `state.language_text`) and what the editor words in its
/// memos (a lock's label in the gutter), without mounting again.
#[test]
fn texts_follow_the_strings_setting_live() {
    let settings = RwSignal::new(EditorSettings::default());
    let (_owner, state) = TestEditor {
        settings: settings.into(),
        read_only_paths: vec![super::paths::FieldPath::from("/web")],
        ..TestEditor::new(Language::YAML, TEXT)
    }
    .mount();
    let find = state.text(|strings| &strings.find);
    let source = state.language_text(|strings| &strings.source_label);
    let lock = move || {
        state.view().lines.with_untracked(|lines| {
            lines
                .iter()
                .find_map(|line| line.marks.as_ref().and_then(|marks| marks.lock.clone()))
        })
    };
    assert_eq!(find(), "Find");
    assert_eq!(source(), "YAML source");
    assert_eq!(
        lock().as_deref(),
        Some("Read-only: This field is read-only")
    );
    settings.update(|settings| {
        settings.strings.find = "Suchen".into();
        settings.strings.source_label = "{language}-Quelltext".into();
        settings.strings.read_only_label = "Schreibgeschützt".into();
        settings.strings.read_only_field = "Vom Server".into();
    });
    assert_eq!(find(), "Suchen");
    assert_eq!(source(), "YAML-Quelltext");
    assert_eq!(lock().as_deref(), Some("Schreibgeschützt: Vom Server"));
    settings.set(EditorSettings::default());
    assert_eq!(find(), "Find");
}
