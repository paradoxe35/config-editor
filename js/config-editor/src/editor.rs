//! The `ConfigEditor` class: the crate's component mounted into an element,
//! with setters writing the signals it reads.

use std::cell::RefCell;
use std::collections::HashMap;

use config_editor_leptos::{EditorHandle, Language, Schema};
use js_sys::{Array, Function};
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::handle_calls;
use crate::host_diagnostics::host_diagnostics_from;
use crate::languages::{language_named, writable_edit_as};
use crate::mount::{self, IN_CALLBACK, Latest, Setup, State};
use crate::options::{
    CALLBACK_KEYS, EDITOR_OPTIONS, bool_option, function_option, string_option, warn_unknown_keys,
    warn_unknown_options,
};
use crate::protect::{paths_from, ranges_from, secrets_from};
use crate::settings::{settings_from, settings_result};
use crate::typings::{
    EditorOptions, EditorUpdate, LineRanges, PathResult, RangeResult, Settings, SettingsResult,
};
use crate::values::{context_arg, field, text_arg};

/// A config editor mounted into an element.
///
/// ```js
/// import init, { ConfigEditor } from "@pngwasi/config-editor";
/// await init();
/// const editor = new ConfigEditor(document.getElementById("config"), {
///   language: "yaml",
///   value: "listen: 7835\n",
///   schema: schemaJson,
///   onChange: (text) => save(text),
/// });
/// ```
#[wasm_bindgen]
pub struct ConfigEditor {
    state: State,
    latest: Latest,
    /// The document when `destroy` ran, so `value` still answers afterwards.
    final_value: Option<String>,
    /// The editor's entry in [`MOUNTED`] until `destroy`.
    id: Option<u32>,
}

/// The unmount handle, reactive owner, and element of an editor.
pub(crate) type Mounted = (Box<dyn std::any::Any>, Owner, web_sys::HtmlElement);

/// Unmounts the editors mounted in `element` or inside it: mounting a new one
/// there replaces them (as the element's content), and they would otherwise
/// stay alive, unseen, until the page closes.
pub(crate) fn replace_editors_in(element: &web_sys::HtmlElement) {
    let replaced: Vec<Mounted> = MOUNTED.with(|editors| {
        let mut editors = editors.borrow_mut();
        let ids: Vec<u32> = editors
            .iter()
            .filter(|(_, (_, _, mounted_in))| element.contains(Some(mounted_in)))
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter_map(|id| editors.remove(&id))
            .collect()
    });
    for (mounted, owner, _) in replaced {
        drop(mounted);
        owner.cleanup();
    }
}

thread_local! {
    /// Mounted editors, kept here rather than in the JavaScript object: an
    /// instance the page no longer references is garbage-collected (and its
    /// Rust side dropped), but the editor stays on the page until `destroy`.
    pub(crate) static MOUNTED: RefCell<HashMap<u32, Mounted>> = RefCell::new(HashMap::new());
    pub(crate) static NEXT_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

#[wasm_bindgen]
impl ConfigEditor {
    /// Mounts an editor into `element` (its content is replaced).
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "HTMLElement")] element: JsValue,
        options: EditorOptions,
    ) -> Result<ConfigEditor, JsError> {
        let element: web_sys::HtmlElement = element.dyn_into().map_err(|_| {
            JsError::new(
                "ConfigEditor needs an HTMLElement to mount into (got null or another value)",
            )
        })?;
        let options: JsValue = options.into();
        let language = match string_option(&options, "language")? {
            Some(name) => language_named(&name)?,
            None => Language::default(),
        };
        let edit_as = string_option(&options, "editAs")?
            .map(|name| language_named(&name).and_then(writable_edit_as))
            .transpose()?
            .filter(|edit| *edit != language);
        let settings = settings_from(&options)?;
        let schema = match string_option(&options, "schema")? {
            Some(json) => {
                Some(Schema::compile(&json).map_err(|error| JsError::new(&error.to_string()))?)
            }
            None => None,
        };
        let value = string_option(&options, "value")?.unwrap_or_default();
        let baseline = string_option(&options, "baseline")?.unwrap_or_else(|| value.clone());
        let file_name = string_option(&options, "fileName")?;
        let class = string_option(&options, "className")?;
        let read_only = bool_option(&options, "readOnly")?.unwrap_or(false);
        let on_change = function_option(&options, "onChange")?;
        let on_edit_as_change = function_option(&options, "onEditAsChange")?;
        let on_change_summary = function_option(&options, "onChangeSummary")?;
        let on_path_change = function_option(&options, "onPathChange")?;
        let on_save = function_option(&options, "onSave")?;
        let on_dirty_change = function_option(&options, "onDirtyChange")?;
        let on_baseline_conflict = function_option(&options, "onBaselineConflict")?;
        let host_diagnostics = match field(&options, "hostDiagnostics") {
            list if list.is_undefined() || list.is_null() => Vec::new(),
            list => host_diagnostics_from(&list, &value)?,
        };
        let on_schema_warning = function_option(&options, "onSchemaWarning")?;
        let read_only_paths = paths_from(&field(&options, "readOnlyPaths"), "readOnlyPaths")?;
        let read_only_ranges =
            ranges_from(&field(&options, "readOnlyRanges"), &value, "readOnlyRanges")?;
        warn_unknown_options(&options, EDITOR_OPTIONS);
        // The editor replaces whatever the element held, editors included.
        replace_editors_in(&element);
        element.set_text_content(None);

        let latest: Latest = std::sync::Arc::new(std::sync::Mutex::new(value.clone()));
        let latest_for_view = latest.clone();
        let owner = Owner::new();
        let secrets = owner.with(|| secrets_from(&field(&options, "secrets")))?;
        let state = owner.with(|| State {
            value: RwSignal::new(value.clone()),
            baseline: RwSignal::new(baseline),
            edit_as: RwSignal::new(edit_as),
            settings: RwSignal::new(settings),
            read_only: RwSignal::new(read_only),
            on_change: StoredValue::new_local(on_change),
            follows_path: RwSignal::new(on_path_change.is_some()),
            on_edit_as_change: StoredValue::new_local(on_edit_as_change),
            on_change_summary: StoredValue::new_local(on_change_summary),
            on_path_change: StoredValue::new_local(on_path_change),
            saves: RwSignal::new(on_save.is_some()),
            on_save: StoredValue::new_local(on_save),
            on_dirty_change: StoredValue::new_local(on_dirty_change),
            on_baseline_conflict: StoredValue::new_local(on_baseline_conflict),
            handle: EditorHandle::new(),
            host_diagnostics: RwSignal::new(host_diagnostics),
            read_only_paths: RwSignal::new(read_only_paths),
            read_only_ranges: RwSignal::new(read_only_ranges),
        });
        let setup = Setup {
            language,
            value,
            schema,
            file_name: file_name.unwrap_or_default(),
            class: class.unwrap_or_default(),
            on_schema_warning,
            secrets,
        };
        let mount_element = element.clone();
        let mounted = owner.with(|| {
            leptos::mount::mount_to(element, move || mount::view(state, latest_for_view, setup))
        });
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1));
            id
        });
        MOUNTED.with(|editors| {
            editors
                .borrow_mut()
                .insert(id, (Box::new(mounted), owner, mount_element));
        });
        Ok(ConfigEditor {
            state,
            latest,
            final_value: None,
            id: Some(id),
        })
    }

    /// The document, in `language`.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> String {
        // Input the browser sent in this task, and an Edit-as edit still
        // waiting out the typing pause, first: the document as shown.
        config_editor_leptos::flush_pending_edits();
        match &self.final_value {
            Some(value) => value.clone(),
            None => self
                .latest
                .lock()
                .map(|text| text.clone())
                .unwrap_or_default(),
        }
    }

    /// Replaces the document (as a host update; the caret is kept when the
    /// text is unchanged).
    #[wasm_bindgen(js_name = setValue)]
    pub fn set_value(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] value: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task, and an Edit-as edit still
        // waiting out the typing pause, first: the reset replaces them.
        config_editor_leptos::flush_pending_edits();
        if self.id.is_none() {
            return Ok(());
        }
        let value = text_arg(&value, "value")?;
        *self
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = value.clone();
        let _ = self.state.value.try_set(value);
        Ok(())
    }

    /// Sets the text the diff gutter compares against.
    #[wasm_bindgen(js_name = setBaseline)]
    pub fn set_baseline(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] baseline: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let _ = self
            .state
            .baseline
            .try_set(text_arg(&baseline, "baseline")?);
        Ok(())
    }

    /// Edits in another language, or in `language` again with `null`.
    #[wasm_bindgen(js_name = setEditAs)]
    pub fn set_edit_as(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string | null | undefined")] language: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let language = if language.is_null() || language.is_undefined() {
            None
        } else {
            Some(writable_edit_as(language_named(&text_arg(
                &language, "editAs",
            )?)?)?)
        };
        let _ = self.state.edit_as.try_set(language);
        Ok(())
    }

    /// The settings in effect: every key of `EditorSettings`, set.
    #[wasm_bindgen(getter)]
    pub fn settings(&self) -> SettingsResult {
        settings_result(&self.state.settings.try_get_untracked().unwrap_or_default())
    }

    /// Replaces the settings, live (the caret, undo history and folds
    /// stay); keys left out take their defaults, as in the constructor. To
    /// change one: `editor.setSettings({ ...editor.settings, folding: false })`.
    #[wasm_bindgen(js_name = setSettings)]
    pub fn set_settings(&self, settings: Settings) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let settings: JsValue = settings.into();
        if !settings.is_object() || Array::is_array(&settings) {
            return Err(JsError::new(
                "setSettings takes an object: { showFooter?, folding?, theme?, … }",
            ));
        }
        warn_unknown_keys(
            &settings,
            &crate::settings::setting_keys()
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            "setting",
        );
        let settings = settings_from(&settings)?;
        if self
            .state
            .settings
            .try_with_untracked(|current| *current != settings)
            == Some(true)
        {
            let _ = self.state.settings.try_set(settings);
        }
        Ok(())
    }

    #[wasm_bindgen(js_name = setReadOnly)]
    #[allow(non_snake_case, reason = "the parameter name shows in TypeScript")]
    pub fn set_read_only(
        &self,
        #[wasm_bindgen(unchecked_param_type = "boolean")] readOnly: JsValue,
    ) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let read_only = readOnly
            .as_bool()
            .ok_or_else(|| JsError::new("readOnly must be a boolean"))?;
        let _ = self.state.read_only.try_set(read_only);
        Ok(())
    }

    /// Replaces callbacks (`onChange`, `onSave`, …; see `CallbackUpdate`).
    #[wasm_bindgen(js_name = setCallbacks)]
    pub fn set_callbacks(&self, callbacks: EditorUpdate) -> Result<(), JsError> {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let callbacks: JsValue = callbacks.into();
        if !callbacks.is_object() || Array::is_array(&callbacks) {
            return Err(JsError::new(
                "setCallbacks takes an object: { onChange?, onEditAsChange? }",
            ));
        }
        warn_unknown_keys(&callbacks, CALLBACK_KEYS, "callback");
        // Check both before changing either.
        let read = |name: &str| -> Result<Option<Option<Function>>, JsError> {
            let value = field(&callbacks, name);
            if value.is_undefined() {
                Ok(None)
            } else if value.is_null() {
                Ok(Some(None))
            } else {
                value
                    .dyn_into::<Function>()
                    .map(|callback| Some(Some(callback)))
                    .map_err(|_| JsError::new(&format!("{name} must be a function or null")))
            }
        };
        // A function replaces a callback; `null` removes it; absent keeps it.
        let on_change = read("onChange")?;
        let on_edit_as_change = read("onEditAsChange")?;
        let on_change_summary = read("onChangeSummary")?;
        let on_path_change = read("onPathChange")?;
        let on_save = read("onSave")?;
        let on_dirty_change = read("onDirtyChange")?;
        let on_baseline_conflict = read("onBaselineConflict")?;
        if let Some(callback) = on_save {
            let _ = self.state.saves.try_set(callback.is_some());
            let _ = self.state.on_save.try_set_value(callback);
        }
        if let Some(callback) = on_dirty_change {
            let _ = self.state.on_dirty_change.try_set_value(callback);
        }
        if let Some(callback) = on_baseline_conflict {
            let _ = self.state.on_baseline_conflict.try_set_value(callback);
        }
        if let Some(callback) = on_change {
            let _ = self.state.on_change.try_set_value(callback);
        }
        if let Some(callback) = on_edit_as_change {
            let _ = self.state.on_edit_as_change.try_set_value(callback);
        }
        if let Some(callback) = on_change_summary {
            let _ = self.state.on_change_summary.try_set_value(callback);
        }
        if let Some(callback) = on_path_change {
            let _ = self.state.follows_path.try_set(callback.is_some());
            let _ = self.state.on_path_change.try_set_value(callback);
        }
        Ok(())
    }

    /// The unified diff (`diff -u`) of the baseline against the document,
    /// with `context` unchanged lines around each change (default 3); empty
    /// when they are the same.
    #[wasm_bindgen(js_name = unifiedDiff)]
    pub fn unified_diff(
        &self,
        #[wasm_bindgen(unchecked_optional_param_type = "number")] context: JsValue,
    ) -> Result<String, JsError> {
        let context = context_arg(&context)?;
        let current = self.value();
        let baseline = self
            .state
            .baseline
            .try_get_untracked()
            .unwrap_or_else(|| current.clone());
        Ok(config_editor_leptos::unified_diff(
            &baseline, &current, context,
        ))
    }

    /// Replaces the host's problems (a server's validation), shown next to
    /// the editor's own; `[]` clears them.
    #[wasm_bindgen(js_name = setHostDiagnostics)]
    pub fn set_host_diagnostics(
        &self,
        #[wasm_bindgen(unchecked_param_type = "HostDiagnostic[]")] diagnostics: JsValue,
    ) -> Result<(), JsError> {
        config_editor_leptos::flush_pending_input();
        let host_text = self
            .latest
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default();
        let diagnostics = if diagnostics.is_undefined() || diagnostics.is_null() {
            Vec::new()
        } else {
            host_diagnostics_from(&diagnostics, &host_text)?
        };
        let _ = self.state.host_diagnostics.try_set(diagnostics);
        Ok(())
    }

    /// Replaces the fields the user cannot change (on top of the schema's
    /// `readOnly` ones); `[]` clears them.
    #[wasm_bindgen(js_name = setReadOnlyPaths)]
    pub fn set_read_only_paths(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath[]")] paths: JsValue,
    ) -> Result<(), JsError> {
        config_editor_leptos::flush_pending_input();
        let paths = paths_from(&paths, "readOnlyPaths")?;
        let _ = self.state.read_only_paths.try_set(paths);
        Ok(())
    }

    /// Replaces the stretches of the document (in `language`) the user
    /// cannot change; `[]` clears them.
    #[wasm_bindgen(js_name = setReadOnlyRanges)]
    pub fn set_read_only_ranges(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TextRangeInput[]")] ranges: JsValue,
    ) -> Result<(), JsError> {
        config_editor_leptos::flush_pending_input();
        let host_text = self
            .latest
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default();
        let ranges = ranges_from(&ranges, &host_text, "readOnlyRanges")?;
        let _ = self.state.read_only_ranges.try_set(ranges);
        Ok(())
    }

    /// Where the field at `path` is written in the text on screen, or `null`.
    #[wasm_bindgen(js_name = rangeOf)]
    pub fn range_of(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<RangeResult, JsError> {
        handle_calls::range_of(Some(self.state.handle), &path)
    }

    /// The path of what the text on screen holds at string index `offset`.
    #[wasm_bindgen(js_name = pathAt)]
    pub fn path_at(
        &self,
        #[wasm_bindgen(unchecked_param_type = "number")] offset: JsValue,
    ) -> Result<PathResult, JsError> {
        handle_calls::path_at(Some(self.state.handle), &offset)
    }

    /// The path of what the caret is on.
    #[wasm_bindgen(getter, js_name = currentPath)]
    pub fn current_path(&self) -> PathResult {
        handle_calls::current_path(Some(self.state.handle))
    }

    /// The text on screen: in the `editAs` language while editing as another
    /// one (what `rangeOf` and `pathAt` index into). After `destroy()`, the
    /// last document.
    #[wasm_bindgen(getter)]
    pub fn text(&self) -> String {
        handle_calls::text(Some(self.state.handle)).unwrap_or_else(|| self.value())
    }

    /// The folded blocks, as one-based `{ startLine, endLine }`: the line
    /// that stays visible and the last hidden one.
    #[wasm_bindgen(getter, js_name = foldedLines)]
    pub fn folded_lines(&self) -> LineRanges {
        handle_calls::folded_lines(Some(self.state.handle))
    }

    /// Scrolls to the field at `path`, selects it and focuses the editor;
    /// `false` when the document has no such field.
    pub fn reveal(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<bool, JsError> {
        handle_calls::reveal(Some(self.state.handle), &path)
    }

    /// Folds the block the field at `path` opens; `false` when there is none
    /// to fold.
    pub fn fold(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<bool, JsError> {
        handle_calls::fold(Some(self.state.handle), &path)
    }

    /// Opens the folds around the field at `path`; `false` when none was
    /// closed.
    pub fn unfold(
        &self,
        #[wasm_bindgen(unchecked_param_type = "FieldPath")] path: JsValue,
    ) -> Result<bool, JsError> {
        handle_calls::unfold(Some(self.state.handle), &path)
    }

    /// Folds every block (`Ctrl+K Ctrl+0`).
    #[wasm_bindgen(js_name = foldAll)]
    pub fn fold_all(&self) {
        handle_calls::fold_all(Some(self.state.handle));
    }

    /// Opens every fold (`Ctrl+K Ctrl+J`).
    #[wasm_bindgen(js_name = unfoldAll)]
    pub fn unfold_all(&self) {
        handle_calls::unfold_all(Some(self.state.handle));
    }

    /// Focuses the editor, keeping its selection.
    pub fn focus(&self) {
        handle_calls::focus(Some(self.state.handle));
    }

    /// Saves as `Ctrl+S` does: calls `onSave` with the document in
    /// `language`. `false` when nothing was sent: without `onSave`, after
    /// `destroy`, or while an `editAs` edit cannot convert back (the caret
    /// goes to the problem).
    pub fn save(&self) -> bool {
        config_editor_leptos::flush_pending_input();
        if self.state.on_save.try_with_value(Option::is_some) != Some(true) {
            return false;
        }
        untrack(|| self.state.handle.save())
    }

    /// Whether the document differs from the baseline.
    #[wasm_bindgen(getter, js_name = isDirty)]
    pub fn is_dirty(&self) -> bool {
        config_editor_leptos::flush_pending_input();
        untrack(|| self.state.handle.is_dirty())
    }

    /// Unmounts the editor. The instance cannot be used afterwards. An edit
    /// still waiting to be saved (while editing as another language) is sent
    /// to `onChange` first.
    pub fn destroy(&mut self) {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::flush_pending_input();
        let Some((mounted, owner, _)) = self
            .id
            .take()
            .and_then(|id| MOUNTED.with(|editors| editors.borrow_mut().remove(&id)))
        else {
            return;
        };
        let unmount = move || {
            drop(mounted);
            owner.cleanup();
        };
        if IN_CALLBACK.with(|depth| depth.get()) > 0 {
            // Called from a callback: the editor's handler is still running,
            // so it unmounts once that returns.
            self.final_value = Some(
                self.latest
                    .lock()
                    .map(|text| text.clone())
                    .unwrap_or_default(),
            );
            let deferred = Closure::once_into_js(unmount);
            if let Ok(queue) = field(&js_sys::global(), "queueMicrotask").dyn_into::<Function>() {
                let _ = queue.call1(&JsValue::NULL, &deferred);
            }
        } else {
            unmount();
            self.final_value = Some(
                self.latest
                    .lock()
                    .map(|text| text.clone())
                    .unwrap_or_default(),
            );
        }
    }
}
