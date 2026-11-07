//! Mounting the crate's component for a `ConfigEditor`: the live state its
//! setters write, and the props and callbacks wired to JavaScript.

use config_editor_leptos::{
    BaselineConflict, ChangeSummary, ConfigEditor as Editor, EditorHandle, EditorTheme,
    HostDiagnostic, Language, SaveRequest, Schema, SchemaSource, ThemeColors,
};
use js_sys::Function;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::catalog::language_name;
use crate::options::Toggles;
use crate::values::{ChangeSummaryOut, path_to_js, to_js};

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

/// The live state a mounted editor reads; setters write it.
#[derive(Clone, Copy)]
pub(crate) struct State {
    pub(crate) value: RwSignal<String>,
    pub(crate) baseline: RwSignal<String>,
    pub(crate) edit_as: RwSignal<Option<Language>>,
    pub(crate) theme: RwSignal<EditorTheme>,
    pub(crate) colors: RwSignal<ThemeColors>,
    pub(crate) read_only: RwSignal<bool>,
    pub(crate) on_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_edit_as_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_change_summary: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_path_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_save: StoredValue<Option<Function>, LocalStorage>,
    /// Whether `onSave` is set: only then does `Ctrl+S` save.
    pub(crate) saves: RwSignal<bool>,
    /// The `saveShortcut` option: `Ctrl+S` saves while this and `saves` hold.
    pub(crate) save_shortcut: RwSignal<bool>,
    pub(crate) on_dirty_change: StoredValue<Option<Function>, LocalStorage>,
    pub(crate) on_baseline_conflict: StoredValue<Option<Function>, LocalStorage>,
    /// Whether `onPathChange` is set: only then is the caret's path computed.
    pub(crate) follows_path: RwSignal<bool>,
    pub(crate) handle: EditorHandle,
    pub(crate) host_diagnostics: RwSignal<Vec<HostDiagnostic>>,
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
    pub(crate) toggles: Toggles,
    pub(crate) on_schema_warning: Option<Function>,
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
            call_host(&callback, &JsValue::from_str(&text));
        }
    };
    let switched = move |language: Language| {
        if let Some(callback) = state.on_edit_as_change.try_get_value().flatten() {
            call_host(&callback, &JsValue::from_str(&language_name(language)));
        }
    };
    let summarized = move |summary: ChangeSummary| {
        if let Some(callback) = state.on_change_summary.try_get_value().flatten() {
            call_host(&callback, &to_js(&ChangeSummaryOut::from(summary)));
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
        if let Some(callback) = state.on_dirty_change.try_get_value().flatten() {
            call_host(&callback, &JsValue::from_bool(dirty));
        }
    };
    let conflicted = move |event: BaselineConflict| {
        if let Some(callback) = state.on_baseline_conflict.try_get_value().flatten() {
            call_host(&callback, &JsValue::from_str(conflict_name(event)));
        }
    };
    let save_shortcut = Signal::derive(move || state.saves.get() && state.save_shortcut.get());
    follow_path(state);
    let host_diagnostics = Signal::derive(move || state.host_diagnostics.get());
    let warn_with = StoredValue::new_local(setup.on_schema_warning);
    let schema_warning = move |message: String| match warn_with.try_get_value().flatten() {
        Some(callback) => call_host(&callback, &JsValue::from_str(&message)),
        None => web_sys::console::warn_1(&format!("config-editor: {message}").into()),
    };
    let edit_as = Signal::derive(move || state.edit_as.get());
    let value_signal = Signal::derive(move || state.value.get());
    let baseline = Signal::derive(move || state.baseline.get());
    let theme = Signal::derive(move || state.theme.get());
    let colors = Signal::derive(move || Some(state.colors.get()));
    let read_only = Signal::derive(move || state.read_only.get());
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
            theme=theme
            colors=colors
            read_only=read_only
            styled=setup.toggles.styled
            show_toolbar=setup.toggles.show_toolbar
            show_footer=setup.toggles.show_footer
            enable_search=setup.toggles.search
            enable_go_to_line=setup.toggles.go_to_line
            enable_folding=setup.toggles.folding
            show_diff=setup.toggles.show_diff
            show_diagnostics=setup.toggles.show_diagnostics
            enable_completions=setup.toggles.completions
            auto_trigger_completions=setup.toggles.auto_trigger_completions
            enable_hover_docs=setup.toggles.hover_docs
            enable_highlighting=setup.toggles.highlighting
            enable_auto_closing=setup.toggles.auto_closing
            on_change=changed
            on_edit_as_change=switched
            on_change_summary=summarized
            on_save=saved
            enable_save_shortcut=save_shortcut
            on_dirty_change=dirty_changed
            on_baseline_conflict=conflicted
            handle=state.handle
            host_diagnostics=host_diagnostics
            on_schema_warning=schema_warning
        />
    }
    .into_any()
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
