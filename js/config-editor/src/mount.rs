//! Mounting the crate's component for a `ConfigEditor`: the live state its
//! `update` writes, and the props and callbacks wired to JavaScript.

use config_editor_leptos::{
    BaselineConflict, ChangeSummary, ConfigEditor as Editor, Diagnostic, EditorHandle,
    EditorSettings, FieldPath, HostDiagnostic, Language, SaveRequest, Schema, SchemaSource,
    SecretOptions, TextRange,
};
use js_sys::Function;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::catalog::language_name;
use crate::echoes::Echoes;
use crate::told::Told;
use crate::values::{ChangeSummaryOut, diagnostics_to_js, path_to_js, to_js};

thread_local! {
    /// How many host callbacks are running: `destroy` from inside one waits
    /// until the editor's own handler has returned.
    pub(crate) static IN_CALLBACK: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Calls a host callback; an exception it throws is logged, not swallowed.
pub(crate) fn call_host(callback: &Function, argument: &JsValue) {
    call_host_with(callback, std::slice::from_ref(argument));
}

/// [`call_host`] with several arguments.
pub(crate) fn call_host_with(callback: &Function, arguments: &[JsValue]) {
    IN_CALLBACK.with(|depth| depth.set(depth.get() + 1));
    let result = match arguments {
        [first, second, third] => callback.call3(&JsValue::NULL, first, second, third),
        [first, second] => callback.call2(&JsValue::NULL, first, second),
        [first] => callback.call1(&JsValue::NULL, first),
        _ => callback.call0(&JsValue::NULL),
    };
    IN_CALLBACK.with(|depth| depth.set(depth.get() - 1));
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
    pub(crate) value: RwSignal<String>,
    pub(crate) baseline: RwSignal<String>,
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
    pub(crate) host_diagnostics: RwSignal<Vec<HostDiagnostic>>,
    pub(crate) read_only_paths: RwSignal<Vec<FieldPath>>,
    pub(crate) read_only_ranges: RwSignal<Vec<TextRange>>,
    /// What the state callbacks last reported (see `told`).
    pub(crate) told: StoredValue<Told, LocalStorage>,
    /// The texts `onChange` sent that the host has not handed back yet
    /// (with `echoes`).
    pub(crate) echoes: StoredValue<Echoes, LocalStorage>,
}

impl State {
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
        ] {
            let _ = callback.try_set_value(None);
        }
    }
}

/// The latest document, outside the reactive arena: an edit saved while the
/// editor unmounts arrives after its signals are gone.
pub(crate) type Latest = std::sync::Arc<std::sync::Mutex<String>>;

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
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = text.clone();
        let _ = state.value.try_set(text.clone());
        if let Some(callback) = state.on_change.try_get_value().flatten() {
            let _ = state.echoes.try_update_value(|echoes| echoes.sent(&text));
            call_host(&callback, &JsValue::from_str(&text));
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
    let host_diagnostics = Signal::derive(move || state.host_diagnostics.get());
    let read_only_paths = Signal::derive(move || state.read_only_paths.get());
    let read_only_ranges = Signal::derive(move || state.read_only_ranges.get());
    let schema_warning =
        move |message: String| match state.on_schema_warning.try_get_value().flatten() {
            Some(callback) => call_host(&callback, &JsValue::from_str(&message)),
            None => web_sys::console::warn_1(&format!("config-editor: {message}").into()),
        };
    let edit_as = Signal::derive(move || state.edit_as.get());
    let value_signal = Signal::derive(move || state.value.get());
    let baseline = Signal::derive(move || state.baseline.get());
    let settings = saving_settings(state.settings, state.saves);
    let read_only = Signal::derive(move || state.read_only.get());
    let read_only_reason = Signal::derive(move || state.read_only_reason.get());
    view! {
        <Editor
            language=setup.language
            edit_as=edit_as
            initial_value=setup.value
            value=value_signal
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
            host_diagnostics=host_diagnostics
            on_schema_warning=schema_warning
            read_only_paths=read_only_paths
            read_only_ranges=read_only_ranges
            secrets=setup.secrets
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
