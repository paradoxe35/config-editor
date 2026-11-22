//! Mounting the crate's component for a `ConfigEditor`: the live state its
//! `update` writes, and the props and callbacks wired to JavaScript.

use js_sys::Function;
use leptos::prelude::*;
use leptos_config_editor::js_support::HostChannel;
use leptos_config_editor::{
    BaselineConflict, ChangeSummary, ConfigEditor as Editor, Diagnostic, EditorHandle,
    EditorSettings, FieldPath, Language, SaveRequest, Schema, SchemaSource, SecretOptions,
};
use wasm_bindgen::prelude::*;

use crate::catalog::language_name;
use crate::told::Told;
use crate::values::{ChangeSummaryOut, diagnostics_to_js, path_to_js, to_js};

thread_local! {
    /// How many host callbacks are running: `destroy` from inside one waits
    /// until the editor's own handler has returned.
    pub(crate) static IN_CALLBACK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Counts a host callback starting (`1`) or ending (`-1`). While one runs,
/// the global `Symbol.for("config-editor.inHostCallback")` is `true`:
/// `free()` from inside one frees once it has returned (see `load.js`), as
/// the instance may be in use below it. A flag, not an export: no entry
/// offers it.
fn count_callback(step: isize) {
    let depth = IN_CALLBACK.with(|depth| {
        let next = depth.get().saturating_add_signed(step);
        depth.set(next);
        next
    });
    if depth <= 1 {
        let key = js_sys::Symbol::for_("config-editor.inHostCallback");
        let _ = js_sys::Reflect::set(&js_sys::global(), &key, &JsValue::from_bool(depth > 0));
    }
}

/// Calls a host callback; an exception it throws is logged, not swallowed.
pub(crate) fn call_host(callback: &Function, argument: &JsValue) {
    call_host_with(callback, std::slice::from_ref(argument));
}

/// [`call_host`] with several arguments.
pub(crate) fn call_host_with(callback: &Function, arguments: &[JsValue]) {
    count_callback(1);
    let result = match arguments {
        [first, second, third] => callback.call3(&JsValue::NULL, first, second, third),
        [first, second] => callback.call2(&JsValue::NULL, first, second),
        [first] => callback.call1(&JsValue::NULL, first),
        _ => callback.call0(&JsValue::NULL),
    };
    count_callback(-1);
    if let Err(error) = result {
        web_sys::console::error_2(
            &JsValue::from_str("config-editor: a callback threw"),
            &error,
        );
    }
}

/// The live state a mounted editor reads; `update` writes it.
#[derive(Clone, Copy)]
pub(crate) struct State {
    /// The host's language, which `editAs` switches from.
    pub(crate) language: Language,
    /// The diff baseline: the channel's copy while it is the document
    /// (see [`shared_with`]).
    pub(crate) baseline: RwSignal<std::sync::Arc<str>>,
    pub(crate) edit_as: RwSignal<Option<Language>>,
    pub(crate) settings: RwSignal<EditorSettings>,
    pub(crate) read_only: RwSignal<bool>,
    pub(crate) read_only_reason: RwSignal<Option<String>>,
    pub(crate) on_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_edit_as_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_sync_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_change_summary: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_problems: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_path_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_save: StoredValue<Option<Function>, LocalStorage>,
    /// Whether `onSave` is set: only then does `Ctrl+S` save (with the
    /// `saveShortcut` setting on).
    pub(crate) saves: RwSignal<bool>,
    pub(crate) on_dirty_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_baseline_conflict: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_schema_warning: StoredValue<Option<Function>, LocalStorage>,
    /// Whether `onPathChange` is set: only then is the caret's path computed.
    pub(crate) follows_path: RwSignal<bool>,
    pub(crate) handle: EditorHandle,
    pub(crate) read_only_paths: RwSignal<Vec<FieldPath>>,
    /// The names the host knows (`keyNames`).
    pub(crate) key_names: RwSignal<Vec<leptos_config_editor::KeyNames>>,
    pub(crate) on_go_to_definition: StoredValue<Option<Function>, LocalStorage>,
    /// What the state callbacks last reported (see `told`).
    pub(crate) told: StoredValue<Told, LocalStorage>,
    /// The editor's channel to its host (the core's record: the texts
    /// `onChange` sent that the host has not handed back yet, the host log
    /// and the lists as given), shared with the mounted editor.
    pub(crate) channel: StoredValue<crate::editor::Shared, LocalStorage>,
}

impl State {
    /// Runs `apply` on the editor's channel while it is mounted.
    pub(crate) fn channel<T>(&self, apply: impl FnOnce(&HostChannel) -> T) -> Option<T> {
        self.channel.try_with_value(apply)
    }

    /// Stops the callbacks once `destroy` is called (from inside one, the
    /// editor unmounts a moment later): none fires again, but `onChange`
    /// with an edit still waiting to be saved as the editor unmounts.
    pub(crate) fn silence(&self) {
        for callback in [
            self.on_edit_as_change,
            self.on_sync_change,
            self.on_change_summary,
            self.on_problems,
            self.on_path_change,
            self.on_save,
            self.on_dirty_change,
            self.on_baseline_conflict,
            self.on_schema_warning,
            self.on_go_to_definition,
        ] {
            let _ = callback.try_set_value(None);
        }
    }
}

/// The latest document, outside the reactive arena: an edit saved while the
/// editor unmounts arrives after its signals are gone. The channel's copy
/// when it is that text (see [`shared_with`]).
pub(crate) type Latest = std::sync::Arc<std::sync::Mutex<std::sync::Arc<str>>>;

/// `text`, the document as the host saw it last, as `latest` keeps it: the
/// channel's own copy (the editor's document) when it is the same text, so
/// an open editor keeps one. A host writing other line endings than the
/// editor's LF (or one editing as another language) sees another text,
/// kept as well.
pub(crate) fn shared_with(channel: Option<std::sync::Arc<str>>, text: &str) -> std::sync::Arc<str> {
    match channel {
        Some(document) if *document == *text => document,
        _ => std::sync::Arc::from(text),
    }
}

/// What the component takes once, at mount.
pub(crate) struct Setup {
    pub(crate) language: Language,
    pub(crate) value: String,
    pub(crate) schema: Option<Schema>,
    pub(crate) file_name: String,
    pub(crate) class: String,
    pub(crate) secrets: SecretOptions,
}

/// The component for `state`, its callbacks calling the host's.
pub(crate) fn view(state: State, latest: Latest, setup: Setup) -> AnyView {
    // The callback is taken out first: it may replace itself or
    // destroy the editor.
    let changed = move |text: String| {
        *latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            shared_with(state.channel(HostChannel::document), &text);
        if let Some(callback) = state.on_change.try_get_value().flatten() {
            // Its revision goes with it (the editor recorded it as sent): a
            // host passing it back with the value (`valueRevision`) has its
            // echo matched exactly.
            let revision = state
                .channel(HostChannel::last_sent)
                .flatten()
                .unwrap_or_default();
            call_host_with(
                &callback,
                &[JsValue::from_str(&text), JsValue::from_f64(revision as f64)],
            );
        } else {
            let _ = state.channel(HostChannel::untold);
        }
    };
    let switched = move |language: Language| {
        let _ = state
            .told
            .try_update_value(|told| told.language = Some(language));
        if let Some(callback) = state.on_edit_as_change.try_get_value().flatten() {
            call_host(&callback, &JsValue::from_str(&language_name(language)));
        }
    };
    let sync_changed = move |in_sync: bool| {
        let _ = state
            .told
            .try_update_value(|told| told.in_sync = Some(in_sync));
        if let Some(callback) = state.on_sync_change.try_get_value().flatten() {
            call_host(&callback, &JsValue::from_bool(in_sync));
        }
    };
    let summarized = move |summary: ChangeSummary| {
        let _ = state
            .told
            .try_update_value(|told| told.summary = Some(summary));
        if let Some(callback) = state.on_change_summary.try_get_value().flatten() {
            call_host(&callback, &to_js(&ChangeSummaryOut::from(summary)));
        }
    };
    let problems_changed = move |problems: Vec<Diagnostic>| {
        let _ = state
            .told
            .try_update_value(|told| told.problems = Some(problems.clone()));
        if let Some(callback) = state.on_problems.try_get_value().flatten() {
            call_host(&callback, &diagnostics_to_js(problems));
        }
    };
    let saved = move |request: SaveRequest| {
        if let Some(callback) = state.on_save.try_get_value().flatten() {
            call_host_with(
                &callback,
                &[
                    JsValue::from_str(&request.text),
                    JsValue::from_str(&language_name(request.language)),
                ],
            );
        }
    };
    let dirty_changed = move |dirty: bool| {
        let _ = state.told.try_update_value(|told| told.dirty = Some(dirty));
        if let Some(callback) = state.on_dirty_change.try_get_value().flatten() {
            call_host(&callback, &JsValue::from_bool(dirty));
        }
    };
    let conflicted = move |event: BaselineConflict| {
        if let Some(callback) = state.on_baseline_conflict.try_get_value().flatten() {
            call_host(&callback, &JsValue::from_str(conflict_name(event)));
        }
    };
    follow_path(state);
    let read_only_paths = Signal::derive(move || state.read_only_paths.get());
    let key_names = Signal::derive(move || state.key_names.get());
    let go_to_definition = move |target: leptos_config_editor::GoToDefinition| {
        if let Some(callback) = state.on_go_to_definition.try_get_value().flatten() {
            call_host(
                &callback,
                &crate::key_names::go_to_definition_to_js(&target),
            );
        }
    };
    let schema_warning =
        move |message: String| match state.on_schema_warning.try_get_value().flatten() {
            Some(callback) => call_host(&callback, &JsValue::from_str(&message)),
            None => web_sys::console::warn_1(&format!("config-editor: {message}").into()),
        };
    let edit_as = Signal::derive(move || state.edit_as.get());
    let baseline = Signal::derive(move || state.baseline.with(|baseline| baseline.to_string()));
    let settings = saving_settings(state.settings, state.saves);
    let read_only = Signal::derive(move || state.read_only.get());
    let read_only_reason = Signal::derive(move || state.read_only_reason.get());
    view! {
        <Editor
            language=setup.language
            edit_as=edit_as
            initial_value=setup.value
            baseline=baseline
            nostrip:schema=setup.schema.map(SchemaSource::from)
            file_name=setup.file_name
            class=setup.class
            settings=settings
            read_only=read_only
            read_only_reason=read_only_reason
            on_change=changed
            on_edit_as_change=switched
            on_sync_change=sync_changed
            on_change_summary=summarized
            on_problems=problems_changed
            on_save=saved
            on_dirty_change=dirty_changed
            on_baseline_conflict=conflicted
            handle=state.handle
            on_schema_warning=schema_warning
            read_only_paths=read_only_paths
            secrets=setup.secrets
            key_names=key_names
            on_go_to_definition=go_to_definition
        />
    }
    .into_any()
}

/// `settings` as the component takes them: `Ctrl+S` saves only while an
/// `onSave` is set (`saves`), the component's own callback being always
/// there.
pub(crate) fn saving_settings(
    settings: RwSignal<EditorSettings>,
    saves: RwSignal<bool>,
) -> Signal<EditorSettings> {
    Signal::derive(move || {
        let saves = saves.get();
        settings.with(|settings| EditorSettings {
            save_shortcut: settings.save_shortcut && saves,
            ..settings.clone()
        })
    })
}

/// A conflict event's name in JavaScript.
fn conflict_name(event: BaselineConflict) -> &'static str {
    match event {
        BaselineConflict::Opened => "opened",
        BaselineConflict::Reloaded => "reloaded",
        BaselineConflict::KeptMine => "kept-mine",
        _ => "settled",
    }
}

/// Calls `onPathChange` with the caret's path each time it changes, while one
/// is set (only then is the path computed).
fn follow_path(state: State) {
    Effect::new(move |_| {
        if !state.follows_path.get() {
            return;
        }
        let Some(path) = state.handle.current_path() else {
            return;
        };
        if let Some(callback) = state.on_path_change.try_get_value().flatten() {
            call_host(&callback, &path_to_js(&path));
        }
    });
}
