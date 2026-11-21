//! The `ConfigEditor` class: the crate's component mounted into an element,
//! `update` writing the signals it reads.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::PoisonError;

use leptos_config_editor::js_support::{self, HostChannel, HostUpdate};
use leptos_config_editor::{EditorHandle, Language};

use crate::schema_formats::{SOURCE_KEYS, read_spec};
use js_sys::Function;
use leptos::prelude::*;
use wasm_bindgen::prelude::*;

use crate::anchors::{anchor_of, revision_of, with_lists};
use crate::handle_calls;
use crate::languages::{language_named, writable_edit_as};
use crate::mount::{self, IN_CALLBACK, Latest, Setup, State, call_host};
use crate::options::{
    EDITOR_OPTIONS, EDITOR_REMOUNT_OPTIONS, bool_option, function_option, string_option,
    warn_unknown_options,
};
use crate::protect::{paths_from, secrets_from};
use crate::settings::{settings_from, settings_result};
use crate::told::{self, Told};
use crate::typings::{
    EditorOptions, EditorUpdate, LineRanges, PathResult, RangeResult, SettingsResult,
};
use crate::update::{
    Fixed, callback_change, flag_change, given, plain_object, refuse_remount, settings_change,
};
use crate::values::{context_arg, field, text_arg};
use leptos_config_editor::js_support::echo_clock;

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
    /// The document when `destroy` ran, so `value` still answers
    /// afterwards: the channel's own copy when it holds that text (see
    /// [`ConfigEditor::kept_once`]).
    final_value: RefCell<Option<std::sync::Arc<str>>>,
    /// The editor's entry in [`MOUNTED`] until `destroy`. Both are cells:
    /// `destroy` takes `&self`, so a callback that a method of this
    /// instance fired (`save()` calling `onSave`) can call it.
    id: std::cell::Cell<Option<u32>>,
    /// Calls made while one of this editor's flushes input (see `reentry`).
    reentry: crate::reentry::Reentry,
    /// The options it cannot change, as given (see `update`).
    built_from: crate::update::BuiltFrom,
    /// The editor's channel to its host (see `host_channel`), shared with
    /// the mounted view, and kept after `destroy` for an editor that takes
    /// it over (`channel`).
    channel: Shared,
}

/// A channel shared by the editor, its mounted view and an editor that
/// takes it over (the core's record of the document's host).
pub(crate) type Shared = HostChannel;

/// An editor's channel to its host: the texts `onChange` sent that the
/// host has not handed back yet, and the host's read-only ranges as given.
/// A framework binding re-creating its editor (a new `language`,
/// `fileName`, …) passes the old editor's `channel` to the new one, so the
/// echoes still on their way are told apart there too, and none rolls
/// back what was typed.
#[wasm_bindgen(js_name = HostChannel)]
pub struct EditorChannel(Shared);

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

/// What a `ConfigEditor` is built from (`EDITOR_REMOUNT_OPTIONS`), with
/// the values that are each one's default: an editor built without a
/// `fileName` shows its language's (`untitled.yaml`).
fn editor_fixed(language: Language) -> Vec<Fixed> {
    EDITOR_REMOUNT_OPTIONS
        .iter()
        .map(|key| match *key {
            "fileName" => Fixed::or(
                key,
                vec![
                    JsValue::from_str(""),
                    JsValue::from_str(&language.default_file_name()),
                ],
            ),
            "className" => Fixed::or(key, vec![JsValue::from_str("")]),
            "echoes" => Fixed::or(key, vec![JsValue::FALSE]),
            _ => Fixed::plain(key),
        })
        .collect()
}

impl ConfigEditor {
    /// The editor shows `value`, the host's document, now.
    fn show(&self, value: String) {
        *self
            .latest
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = value;
    }

    /// Takes `update` through the editor's one entry (see the core's
    /// `host_inputs`); `true` when its value replaced the document.
    fn take(&self, update: HostUpdate) -> Result<bool, JsError> {
        let value = update.value.clone();
        let taken = match js_support::host_update(&self.state.handle, update) {
            Some(taken) => taken,
            None => return Ok(false),
        }
        .map_err(|error| JsError::new(&error.to_string()))?;
        for warning in &taken.warnings {
            web_sys::console::warn_1(&format!("config-editor: {warning}").into());
        }
        if taken.shown
            && let Some(value) = value
        {
            self.show(value);
        }
        Ok(taken.shown)
    }

    /// `text`, the document a destroyed editor answers with, kept once: the
    /// channel (kept for an editor that takes it over) holds the document
    /// too, and when it is `text` that copy is shared.
    fn kept_once(&self, text: String) -> std::sync::Arc<str> {
        let held = self.channel.document();
        if *held == *text {
            held
        } else {
            std::sync::Arc::from(text)
        }
    }

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
            Some(text) => Some(
                read_spec(&options, &SOURCE_KEYS, "")?
                    .compile(&text)
                    .map_err(|error| JsError::new(&error.to_string()))?,
            ),
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
        let echoes = bool_option(&options, "echoes")?.unwrap_or(false);
        let on_change = function_option(&options, "onChange")?;
        let on_edit_as_change = function_option(&options, "onEditAsChange")?;
        let on_sync_change = function_option(&options, "onSyncChange")?;
        let on_change_summary = function_option(&options, "onChangeSummary")?;
        let on_problems = function_option(&options, "onProblems")?;
        let on_path_change = function_option(&options, "onPathChange")?;
        let on_save = function_option(&options, "onSave")?;
        let on_dirty_change = function_option(&options, "onDirtyChange")?;
        let on_baseline_conflict = function_option(&options, "onBaselineConflict")?;
        // The texts the lists name places in, and the revision `value` came
        // with (a fresh editor showing an echo of one that is gone).
        let value_revision = revision_of(&options);
        let ranges_for = anchor_of(&options, "rangesFor")?;
        let diagnostics_for = anchor_of(&options, "diagnosticsFor")?;
        let on_schema_warning = function_option(&options, "onSchemaWarning")?;
        let read_only_paths = paths_from(&field(&options, "readOnlyPaths"), "readOnlyPaths")?;
        warn_unknown_options(&options, EDITOR_OPTIONS);
        // The editor replaces whatever the element held, editors included.
        replace_editors_in(&element);
        element.set_text_content(None);

        let latest: Latest = std::sync::Arc::new(std::sync::Mutex::new(value.clone()));
        let latest_for_view = latest.clone();
        let owner = Owner::new();
        let secrets = owner.with(|| secrets_from(&field(&options, "secrets"), "secrets"))?;
        // The channel: taken over (its echoes, host log and lists go on;
        // the lists the binding renders again are no news), or new, with
        // the texts still on their way of the editor destroyed in this
        // element (see `handoff`). The lists are read in the texts they
        // name before the editor mounts; one naming a revision this channel
        // never sent throws.
        let left = crate::handoff::take(&element);
        let taken = taken_channel(&options)?;
        let fresh = taken.is_none();
        let channel = match taken {
            Some(EditorChannel(channel)) => {
                channel.remount(&value);
                channel
            }
            None => HostChannel::new(echoes, &value, value_revision, left),
        };
        let head = HostUpdate {
            value: fresh.then(|| value.clone()),
            value_revision: value_revision.filter(|_| fresh),
            ranges_for,
            diagnostics_for,
            ..HostUpdate::default()
        };
        let update = with_lists(&channel, &options, head, &value, "")?;
        if update.ranges.is_some() || update.diagnostics.is_some() {
            channel
                .take_unmounted(update, &value)
                .map_err(|error| JsError::new(&error.to_string()))?;
        }
        // Made in the editor's owner, as everything the editor holds: made
        // outside it, the handle's values would stay after `destroy`.
        let handle = owner.with(EditorHandle::new);
        js_support::give_channel(&handle, channel.clone());
        let state = owner.with(|| State {
            language,
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
            handle,
            read_only_paths: RwSignal::new(read_only_paths),
            told: StoredValue::new_local(Told::default()),
            channel: StoredValue::new_local(channel.clone()),
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
            final_value: RefCell::new(None),
            id: std::cell::Cell::new(Some(id)),
            reentry: crate::reentry::Reentry::default(),
            built_from: crate::update::BuiltFrom::of(&options, editor_fixed(language)),
            channel,
        })
    }

    /// The document, in `language`.
    #[wasm_bindgen(getter)]
    pub fn value(&self) -> String {
        // Input the browser sent in this task, and an Edit-as edit still
        // waiting out the typing pause, first: the document as shown.
        leptos_config_editor::flush_pending_edits();
        match &*self.final_value.borrow() {
            Some(value) => value.to_string(),
            None => self.latest_text(),
        }
    }

    /// The editor's channel to its host, for an editor that replaces this
    /// one (its `channel` option): still there after `destroy`.
    #[wasm_bindgen(getter)]
    pub fn channel(&self) -> EditorChannel {
        EditorChannel(self.channel.clone())
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
    /// text is unchanged). With `echoes`, a text `onChange` gave that has
    /// not come back yet is its echo and keeps the document. Ranges are in
    /// `rangesFor`, else `value` (an echo included), else the document,
    /// and are moved from that text to the document; `readOnlyRanges` equal
    /// to the last ones, given with no new text, stay where the editor
    /// moved them (see `host_locks`). The text `onChange` gave last, given
    /// while an edit still held is sent first, keeps the document.
    /// `language`, `schema`, `fileName`, `className`,
    /// `secrets` and `echoes` are what the editor is built from: they throw (create
    /// a new editor instead), but for the value it was built with given again
    /// (`Object.is`; `language` naming the editor's own; an option left out
    /// was built with its default: `null`, `echoes: false`, …).
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
        refuse_remount(&options, &remount, &self.built_from, "ConfigEditor")?;
        warn_unknown_options(&options, EDITOR_OPTIONS);
        let value = given(&options, "value")
            .map(|value| text_arg(&value, "value"))
            .transpose()?;
        // Input the browser sent in this task, and an Edit-as edit still
        // waiting out the typing pause when the document is replaced, first
        // (the texts it sends from `flushed` on: the host did not know them).
        // What the host set from `onChange` meanwhile is newer than this
        // call, and stays.
        self.reentry.note_options(&options);
        let flushed = echo_clock();
        // Whether `value` is the text the host was told last, before the
        // flush below sends it what the user typed since (see `echo` below).
        let told = value.as_deref().is_some_and(|value| {
            *value == *self.latest.lock().unwrap_or_else(PoisonError::into_inner)
        });
        let newer = self.reentry.flush(|| {
            if value.is_some() {
                leptos_config_editor::flush_pending_edits();
            } else {
                leptos_config_editor::js_support::flush_pending_input();
            }
        });
        if self.id.get().is_none() {
            return Ok(());
        }
        let options = crate::reentry::without_newer(&options, &newer);
        let value = value.filter(|_| !newer.contains("value"));
        // The revision `onChange` sent the value with, when the host passes
        // it back: its echo is matched exactly; the texts its lists name.
        let value_revision = revision_of(&options);
        let ranges_for = anchor_of(&options, "rangesFor")?;
        let diagnostics_for = anchor_of(&options, "diagnosticsFor")?;
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
        // The lists, read in the texts they name places in (checked before
        // anything changes).
        let host = with_lists(
            &self.channel,
            &options,
            HostUpdate {
                value: value.clone(),
                value_revision,
                ranges_for,
                diagnostics_for,
                flushed: Some(flushed),
                stale: told,
                ..HostUpdate::default()
            },
            &self.latest_text(),
            "",
        )?;
        let read_only_paths = given(&options, "readOnlyPaths")
            .map(|list| paths_from(&list, "readOnlyPaths"))
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

        // Everything checked: apply, the host's text and lists through the
        // editor's one entry (the text the host was told last, given while
        // the flush above sent it a newer one, is older than the typing).
        if host.value.is_some() || host.ranges.is_some() || host.diagnostics.is_some() {
            self.take(host)?;
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
        if let Some(paths) = read_only_paths {
            let _ = state.read_only_paths.try_set(paths);
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

    /// Replaces the document with `value`, the host's own text whatever it
    /// is: with `echoes`, `update({ value })` takes a text `onChange` gave
    /// that has not come back yet for its echo; this never does (a revert to
    /// such a text). Echoes still on their way are skipped when they come.
    /// The caret stays on the same text when possible, and the read-only
    /// ranges on the text they cover (see `host_locks`); `onChange` does
    /// not hear it. Returns whether it replaced the document: `false` when
    /// it is that text already (a second Discard: nothing changes, and a
    /// framework binding reports nothing), or the host gave a newer one
    /// meanwhile.
    #[wasm_bindgen(js_name = replaceValue)]
    pub fn replace_value(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] value: JsValue,
    ) -> Result<bool, JsError> {
        let value = text_arg(&value, "value")?;
        self.reentry.note("value");
        let newer = self
            .reentry
            .flush(leptos_config_editor::flush_pending_edits);
        // A text the host gave from `onChange` meanwhile is newer.
        if self.id.get().is_none() || newer.contains("value") || value == self.latest_text() {
            return Ok(false);
        }
        // The host's own text: what it holds, no echo to wait for, recorded
        // as sent (its late listing is told apart, and `sentRevision` names
        // it). The locks are carried into it from the ranges as given.
        self.take(HostUpdate {
            value: Some(value),
            reset: true,
            ..HostUpdate::default()
        })
    }

    /// Makes the host's own change of its text, as CodeMirror's dispatch:
    /// `changes` (string indices `from`..`to`, sorted and apart, of the text
    /// they were made on: `options.revision`, a text `onChange` gave, else
    /// the host's text now) replaced by their `insert`. What the user typed
    /// since is kept, the host's change rebased over it (where both wrote
    /// at one place, the host's first); the read-only ranges, folds, secret
    /// placeholders and the caret follow it exactly, and Undo does not undo
    /// it. `onChange` hears the result only when it holds edits the host
    /// did not know. Lists given in `options` are read in the new document
    /// (or where `rangesFor` names). Returns the revision of the new
    /// document (pass it back as `valueRevision`), `null` when nothing
    /// changed. A revision out of date or never sent, or changes that do
    /// not fit the text, throw and change nothing.
    #[wasm_bindgen(js_name = applyChanges)]
    pub fn apply_changes(
        &self,
        #[wasm_bindgen(unchecked_param_type = "TextChange[]")] changes: JsValue,
        #[wasm_bindgen(unchecked_optional_param_type = "ApplyChangesOptions")] options: JsValue,
    ) -> Result<Option<f64>, JsError> {
        let edits = crate::anchors::changes_from(&changes)?;
        let options = if options.is_undefined() || options.is_null() {
            js_sys::Object::new().into()
        } else {
            options
        };
        let base = crate::anchors::revision_named(&options, "revision");
        let ranges_for = anchor_of(&options, "rangesFor")?;
        let diagnostics_for = anchor_of(&options, "diagnosticsFor")?;
        let _ = self
            .reentry
            .flush(leptos_config_editor::js_support::flush_pending_input);
        if self.id.get().is_none() {
            return Ok(None);
        }
        let revision = js_support::apply_changes(
            &self.state.handle,
            &leptos_config_editor::js_support::HostEdits::Utf16(edits),
            base,
        )
        .map_err(|error| JsError::new(&error.to_string()))?;
        if let Some(text) = self.state.handle.value() {
            self.show(text);
        }
        let lists = with_lists(
            &self.channel,
            &options,
            HostUpdate {
                ranges_for,
                diagnostics_for,
                ..HostUpdate::default()
            },
            &self.latest_text(),
            "applyChanges: ",
        )?;
        if lists.ranges.is_some() || lists.diagnostics.is_some() {
            self.take(lists)?;
        }
        Ok(revision.map(|revision| revision as f64))
    }

    /// The revision of the text sent last while it is on its way (with
    /// `echoes`), a reset `replaceValue` made included: pass it back as
    /// `valueRevision` with that text. `undefined` when every text came
    /// back.
    #[wasm_bindgen(getter, js_name = sentRevision)]
    pub fn sent_revision(&self) -> Option<f64> {
        self.channel
            .latest_revision()
            .map(|revision| revision as f64)
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
        Ok(leptos_config_editor::unified_diff(
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
        leptos_config_editor::js_support::flush_pending_input();
        if self.state.on_save.try_with_value(Option::is_some) != Some(true) {
            return false;
        }
        untrack(|| self.state.handle.save())
    }

    /// Whether the document differs from the baseline.
    #[wasm_bindgen(getter, js_name = isDirty)]
    pub fn is_dirty(&self) -> bool {
        leptos_config_editor::js_support::flush_pending_input();
        untrack(|| self.state.handle.is_dirty())
    }

    /// Unmounts the editor. The instance cannot be used afterwards. An edit
    /// still waiting to be saved (while editing as another language) is sent
    /// to `onChange` first.
    pub fn destroy(&self) {
        // Input the browser sent in this task first (see `flush_pending_input`).
        leptos_config_editor::js_support::flush_pending_input();
        let Some((mounted, owner, element)) = self
            .id
            .take()
            .and_then(|id| MOUNTED.with(|editors| editors.borrow_mut().remove(&id)))
        else {
            return;
        };
        // The document is kept once, in `final_value`: `latest` lets go of
        // its copy once the view (which writes it) is gone. The texts still
        // on their way (an edit sent as it unmounts included) are left in
        // its element, for the next editor made there.
        let latest = self.latest.clone();
        let channel = self.channel.clone();
        let unmount = move || {
            drop(mounted);
            owner.cleanup();
            crate::handoff::leave(&element, channel.handoff());
            take_text(&latest)
        };
        if IN_CALLBACK.with(|depth| depth.get()) > 0 {
            // Called from a callback: the editor's handler is still running,
            // so it unmounts once that returns, calling nothing more.
            self.state.silence();
            *self.final_value.borrow_mut() = Some(self.kept_once(self.latest_text()));
            let deferred = Closure::once_into_js(move || drop(unmount()));
            if let Ok(queue) = field(&js_sys::global(), "queueMicrotask").dyn_into::<Function>() {
                let _ = queue.call1(&JsValue::NULL, &deferred);
            }
        } else {
            let text = unmount();
            *self.final_value.borrow_mut() = Some(self.kept_once(text));
        }
    }
}

/// The `channel` option: an editor's channel, taken over (the object
/// given is the new editor's from then on).
fn taken_channel(options: &JsValue) -> Result<Option<EditorChannel>, JsError> {
    use wasm_bindgen::convert::TryFromJsValue;
    let given = field(options, "channel");
    if given.is_undefined() || given.is_null() {
        return Ok(None);
    }
    EditorChannel::try_from_js_value(given)
        .map(Some)
        .map_err(|_| JsError::new("channel must be an editor's `channel` (a HostChannel)"))
}

/// The document `latest` holds, taken out of it.
fn take_text(latest: &Latest) -> String {
    std::mem::take(&mut *latest.lock().unwrap_or_else(PoisonError::into_inner))
}
