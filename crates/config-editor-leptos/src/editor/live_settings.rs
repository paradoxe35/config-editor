//! The host's [`EditorSettings`] signal, split into one memo per setting:
//! each part reads only its own, so a change re-runs only what it affects,
//! and every setting switches live (no part takes one once at mount).

use leptos::prelude::*;

use super::state::EditorState;
use super::{changes, folding, search};
use crate::reactive::Relay;
use crate::{EditorSettings, EditorStrings, EditorTheme, ThemeColors};
use std::sync::Arc;

/// The settings as the editor's parts read them.
#[derive(Clone, Copy)]
pub(super) struct LiveSettings {
    pub(super) theme: Memo<EditorTheme>,
    pub(super) colors: Memo<ThemeColors>,
    pub(super) styled: Memo<bool>,
    pub(super) toolbar: Memo<bool>,
    pub(super) footer: Memo<bool>,
    pub(super) diagnostics: Memo<bool>,
    pub(super) diff: Memo<bool>,
    pub(super) completions: Memo<bool>,
    pub(super) auto_trigger: Memo<bool>,
    /// Hover docs, while there is a schema to read them from.
    pub(super) hover_docs: Memo<bool>,
    pub(super) highlighting: Memo<bool>,
    pub(super) auto_closing: Memo<bool>,
    pub(super) search: Memo<bool>,
    pub(super) go_to_line: Memo<bool>,
    pub(super) folding: Memo<bool>,
    pub(super) save_shortcut: Memo<bool>,
    /// Every built-in text, for the editor's long-lived parts (memos,
    /// views mounted once).
    pub(super) strings: Memo<EditorStrings>,
    /// The same for views that come and go: each reads its own copy (see
    /// `reactive`).
    pub(super) strings_relay: Relay<Arc<EditorStrings>>,
}

impl LiveSettings {
    pub(super) fn new(settings: Signal<EditorSettings>, has_schema: bool) -> Self {
        let flag = move |read: fn(&EditorSettings) -> bool| Memo::new(move |_| settings.with(read));
        let strings = Memo::new(move |_| settings.with(|settings| settings.strings.clone()));
        Self {
            theme: Memo::new(move |_| settings.with(|settings| settings.theme)),
            colors: Memo::new(move |_| settings.with(|settings| settings.colors.clone())),
            styled: flag(|settings| settings.styled),
            toolbar: flag(|settings| settings.show_toolbar),
            footer: flag(|settings| settings.show_footer),
            diagnostics: flag(|settings| settings.show_diagnostics),
            diff: flag(|settings| settings.show_diff),
            completions: flag(|settings| settings.completions),
            auto_trigger: flag(|settings| settings.auto_trigger_completions),
            hover_docs: Memo::new(move |_| {
                has_schema && settings.with(|settings| settings.hover_docs)
            }),
            highlighting: flag(|settings| settings.highlighting),
            auto_closing: flag(|settings| settings.auto_closing),
            search: flag(|settings| settings.search),
            go_to_line: flag(|settings| settings.go_to_line),
            folding: flag(|settings| settings.folding),
            save_shortcut: flag(|settings| settings.save_shortcut),
            strings,
            strings_relay: Relay::new(move || Arc::new(strings.get())),
        }
    }

    /// The colors as the root's inline style, when any is set.
    pub(super) fn color_style(&self) -> Option<String> {
        self.colors
            .with(|colors| (!colors.is_empty()).then(|| colors.to_style()))
    }
}

/// What a setting switched off leaves behind goes with it: open folds,
/// close the find and go-to-line widgets and the change preview, hide the
/// hover card. The parts themselves already read their memo (suggestions,
/// problems, the diff, highlighting).
pub(super) fn follow(state: EditorState) {
    let settings = state.config.settings;
    folding::follow_settings(state);
    search::follow_settings(state);
    changes::follow_settings(state);
    on_switch_off(settings.hover_docs, move || state.hide_hover());
    warn_about_strings(settings.strings);
}

/// A console warning for each placeholder a host's text uses that its
/// default does not have (it would be shown as written).
fn warn_about_strings(strings: Memo<EditorStrings>) {
    Effect::new(move |_| {
        for problem in strings.with(EditorStrings::check) {
            leptos::logging::warn!("config-editor: {problem}");
        }
    });
}

/// Runs `off` each time `setting` turns from on to off.
pub(super) fn on_switch_off(setting: Memo<bool>, off: impl Fn() + 'static) {
    Effect::new(move |was_on: Option<bool>| {
        let on = setting.get();
        if was_on == Some(true) && !on {
            untrack(&off);
        }
        on
    });
}
