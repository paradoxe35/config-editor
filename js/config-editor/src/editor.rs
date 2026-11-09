//! The `ConfigEditor` class: the crate's component mounted into an element,
//! `update` writing the signals it reads.

use std::cell::RefCell;
use std::collections::HashMap;

use config_editor_leptos::{EditorHandle, Language, Schema};
use js_sys::Function;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::handle_calls;
use crate::host_diagnostics::host_diagnostics_from;
use crate::languages::{language_named, writable_edit_as};
use crate::mount::{self, IN_CALLBACK, Latest, Setup, State, call_host};
use crate::options::{
    EDITOR_OPTIONS, EDITOR_REMOUNT_OPTIONS, bool_option, function_option, string_option,
    warn_unknown_options,
};
use crate::protect::{paths_from, ranges_from, secrets_from};
use crate::settings::{settings_from, settings_result};
use crate::told::{self, Told};
use crate::typings::{
    EditorOptions, EditorUpdate, LineRanges, PathResult, RangeResult, SettingsResult,
};
use crate::update::{
    callback_change, flag_change, given, plain_object, refuse_remount, settings_change,
};
use crate::values::{context_arg, field, text_arg};

/// A config editor mounted into an element.
///
/// ```js
/// import { ConfigEditor, loadConfigEditor } from "@pngwasi/config-editor";
/// await loadConfigEditor();
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

impl ConfigEditor {
    /// The document as the host last saw it.
    fn latest_text(&self) -> String {
        self.latest
            .lock()
            .map(|text| text.clone())
            .unwrap_or_default()
    }
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
            .map(|name| language_named(&name).and_then(|edit| writable_edit_as(language, edit)))
            .transpose()?
            .filter(|edit| *edit != language);
        let settings = settings_from(&options)?;
        let schema = match string_option(&options, "schema")? {
            Some(json) => {
                Some(Schema::compile(&json).map_err(|error| JsError::new(&error.to_string()))?)
            }
            None => None,
        };
        // A text, as `update` takes it: `null` is no document.
        let text_option = |name: &str| {
            given(&options, name)
                .map(|text| text_arg(&text, name))
                .transpose()
        };
        let value = text_option("value")?.unwrap_or_default();
        let baseline = text_option("baseline")?.unwrap_or_else(|| value.clone());
        let file_name = string_option(&options, "fileName")?;
        let class = string_option(&options, "className")?;
        let read_only = bool_option(&options, "readOnly")?.unwrap_or(false);
        let read_only_reason = string_option(&options, "readOnlyReason")?;
        let on_change = function_option(&options, "onChange")?;
        let on_edit_as_change = function_option(&options, "onEditAsChange")?;
        let on_sync_change = function_option(&options, "onSyncChange")?;
        let on_change_summary = function_option(&options, "onChangeSummary")?;
        let on_problems = function_option(&options, "onProblems")?;
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
        let secrets = owner.with(|| secrets_from(&field(&options, "secrets"), "secrets"))?;
        let state = owner.with(|| State {
            language,
            value: RwSignal::new(value.clone()),
            baseline: RwSignal::new(baseline),
            edit_as: RwSignal::new(edit_as),
            settings: RwSignal::new(settings),
            read_only: RwSignal::new(read_only),
            read_only_reason: RwSignal::new(read_only_reason),
            on_change: StoredValue::new_local(on_change),
            follows_path: RwSignal::new(on_path_change.is_some()),
            on_edit_as_change: StoredValue::new_local(on_edit_as_change),
            on_sync_change: StoredValue::new_local(on_sync_change),
            on_change_summary: StoredValue::new_local(on_change_summary),
            on_problems: StoredValue::new_local(on_problems),
            on_path_change: StoredValue::new_local(on_path_change),
            saves: RwSignal::new(on_save.is_some()),
            on_save: StoredValue::new_local(on_save),
            on_dirty_change: StoredValue::new_local(on_dirty_change),
            on_baseline_conflict: StoredValue::new_local(on_baseline_conflict),
            on_schema_warning: StoredValue::new_local(on_schema_warning),
            handle: EditorHandle::new(),
            host_diagnostics: RwSignal::new(host_diagnostics),
            read_only_paths: RwSignal::new(read_only_paths),
            read_only_ranges: RwSignal::new(read_only_ranges),
            told: StoredValue::new_local(Told::default()),
        });
        let setup = Setup {
            language,
            value,
            schema,
            file_name: file_name.unwrap_or_default(),
            class: class.unwrap_or_default(),
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
            None => self.latest_text(),
        }
    }

    /// The settings in effect: every key of `EditorSettings`, set.
    #[wasm_bindgen(getter)]
    pub fn settings(&self) -> SettingsResult {
        settings_result(&self.state.settings.try_get_untracked().unwrap_or_default())
    }

    /// Changes what the editor shows, live (the caret, undo history and
    /// folds stay): the constructor's options, any of them. A key left out
    /// or `undefined` stays as it is; `null` removes a callback, clears
    /// `hostDiagnostics`, `readOnlyPaths` and `readOnlyRanges`, turns
    /// `editAs` and `readOnly` off and returns a setting to its default.
    /// Settings merge key by key, `strings` text by text, `colors` token by
    /// token. A new `value`
    /// replaces the document as a host update (the caret kept when the
    /// text is unchanged). `language`, `schema`, `fileName`, `className`
    /// and `secrets` are what the editor is built from: they throw (create
    /// a new editor instead), but for `language` naming the editor's own.
    /// Every key is checked before any applies.
    pub fn update(&self, options: EditorUpdate) -> Result<(), JsError> {
        let options: JsValue = options.into();
        plain_object(
            &options,
            "update takes an object of options: { value?, readOnly?, theme?, onChange?, … }",
        )?;
        // The language it already has changes nothing (a framework passing
        // all its props again).
        let same_language = given(&options, "language")
            .and_then(|name| name.as_string())
            .and_then(|name| language_named(&name).ok())
            .is_some_and(|language| language == self.state.language);
        let remount: Vec<&str> = EDITOR_REMOUNT_OPTIONS
            .iter()
            .copied()
            .filter(|key| !(same_language && *key == "language"))
            .collect();
        refuse_remount(&options, &remount, "ConfigEditor")?;
        warn_unknown_options(&options, EDITOR_OPTIONS);
        let value = given(&options, "value")
            .map(|value| text_arg(&value, "value"))
            .transpose()?;
        // Input the browser sent in this task, and an Edit-as edit still
        // waiting out the typing pause when the document is replaced, first.
        if value.is_some() {
            config_editor_leptos::flush_pending_edits();
        } else {
            config_editor_leptos::js_support::flush_pending_input();
        }
        if self.id.is_none() {
            return Ok(());
        }
        let baseline = given(&options, "baseline")
            .map(|baseline| text_arg(&baseline, "baseline"))
            .transpose()?;
        let edit_as = given(&options, "editAs")
            .map(|language| {
                if language.is_null() {
                    Ok(None)
                } else {
                    language_named(&text_arg(&language, "editAs")?)
                        .and_then(|edit| writable_edit_as(self.state.language, edit))
                        .map(Some)
                }
            })
            .transpose()?;
        let read_only = flag_change(&options, "readOnly")?;
        let read_only_reason = given(&options, "readOnlyReason")
            .map(|reason| {
                if reason.is_null() {
                    Ok(None)
                } else {
                    text_arg(&reason, "readOnlyReason").map(Some)
                }
            })
            .transpose()?;
        let current = self.state.settings.try_get_untracked().unwrap_or_default();
        let settings = settings_change(&current, &options)?;
        // Ranges are in the document as it will be.
        let host_text = value.clone().unwrap_or_else(|| self.latest_text());
        let host_diagnostics = given(&options, "hostDiagnostics")
            .map(|list| {
                if list.is_null() {
                    Ok(Vec::new())
                } else {
                    host_diagnostics_from(&list, &host_text)
                }
            })
            .transpose()?;
        let read_only_paths = given(&options, "readOnlyPaths")
            .map(|list| paths_from(&list, "readOnlyPaths"))
            .transpose()?;
        let read_only_ranges = given(&options, "readOnlyRanges")
            .map(|list| ranges_from(&list, &host_text, "readOnlyRanges"))
            .transpose()?;
        let state = self.state;
        let mut callbacks = Vec::new();
        for (name, slot) in [
            ("onChange", state.on_change),
            ("onEditAsChange", state.on_edit_as_change),
            ("onSyncChange", state.on_sync_change),
            ("onChangeSummary", state.on_change_summary),
            ("onProblems", state.on_problems),
            ("onDirtyChange", state.on_dirty_change),
            ("onBaselineConflict", state.on_baseline_conflict),
            ("onSchemaWarning", state.on_schema_warning),
        ] {
            callbacks.push((name, callback_change(&options, name)?, slot));
        }
        let on_path_change = callback_change(&options, "onPathChange")?;
        let on_save = callback_change(&options, "onSave")?;

        // Everything checked: apply.
        if let Some(value) = value {
            *self
                .latest
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = value.clone();
            let _ = state.value.try_set(value);
        }
        if let Some(baseline) = baseline {
            let _ = state.baseline.try_set(baseline);
        }
        if let Some(edit_as) = edit_as {
            let _ = state.edit_as.try_set(edit_as);
        }
        if let Some(read_only) = read_only {
            let _ = state.read_only.try_set(read_only);
        }
        if let Some(reason) = read_only_reason {
            let _ = state.read_only_reason.try_set(reason);
        }
        if let Some(settings) = settings
            && state
                .settings
                .try_with_untracked(|current| *current != settings)
                == Some(true)
        {
            let _ = state.settings.try_set(settings);
        }
        if let Some(diagnostics) = host_diagnostics {
            let _ = state.host_diagnostics.try_set(diagnostics);
        }
        if let Some(paths) = read_only_paths {
            let _ = state.read_only_paths.try_set(paths);
        }
        if let Some(ranges) = read_only_ranges {
            let _ = state.read_only_ranges.try_set(ranges);
        }
        let added = told::added(callbacks.iter().map(|(name, change, slot)| {
            (
                *name,
                change.as_ref(),
                slot.try_with_value(Option::is_some) == Some(true),
            )
        }));
        for (_, change, slot) in callbacks {
            if let Some(callback) = change {
                let _ = slot.try_set_value(callback);
            }
        }
        if let Some(callback) = on_save {
            let _ = state.saves.try_set(callback.is_some());
            let _ = state.on_save.try_set_value(callback);
        }
        if let Some(callback) = on_path_change {
            let _ = state.follows_path.try_set(callback.is_some());
            let _ = state.on_path_change.try_set_value(callback);
        }
        // A state callback added now hears the state at once.
        let told = state.told.try_get_value().unwrap_or_default();
        for (name, callback) in added {
            if let Some(argument) = told.argument(name) {
                call_host(&callback, &argument);
            }
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

    /// Selects `range` (string indices `{ start, end }` into the text on
    /// screen, as `rangeOf` gives them: in the `editAs` language while
    /// editing as another one), scrolls it into view inside the editor and
    /// focuses the editor without moving the page, as `reveal` does for a
    /// field: any stretch, comments and blank lines included. A range past
    /// the end is clamped to it. `false` before the editor mounts and for an
    /// empty document.
    #[wasm_bindgen(js_name = revealRange)]
    pub fn reveal_range(
        &self,
        #[wasm_bindgen(unchecked_param_type = "{ start: number; end: number }")] range: JsValue,
    ) -> Result<bool, JsError> {
        handle_calls::reveal_range(Some(self.state.handle), &range)
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
        config_editor_leptos::js_support::flush_pending_input();
        if self.state.on_save.try_with_value(Option::is_some) != Some(true) {
            return false;
        }
        untrack(|| self.state.handle.save())
    }

    /// Whether the document differs from the baseline.
    #[wasm_bindgen(getter, js_name = isDirty)]
    pub fn is_dirty(&self) -> bool {
        config_editor_leptos::js_support::flush_pending_input();
        untrack(|| self.state.handle.is_dirty())
    }

    /// Unmounts the editor. The instance cannot be used afterwards. An edit
    /// still waiting to be saved (while editing as another language) is sent
    /// to `onChange` first.
    pub fn destroy(&mut self) {
        // Input the browser sent in this task first (see `flush_pending_input`).
        config_editor_leptos::js_support::flush_pending_input();
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
            // so it unmounts once that returns, calling nothing more.
            self.state.silence();
            self.final_value = Some(self.latest_text());
            let deferred = Closure::once_into_js(unmount);
            if let Ok(queue) = field(&js_sys::global(), "queueMicrotask").dyn_into::<Function>() {
                let _ = queue.call1(&JsValue::NULL, &deferred);
            }
        } else {
            unmount();
            self.final_value = Some(self.latest_text());
        }
    }
}
