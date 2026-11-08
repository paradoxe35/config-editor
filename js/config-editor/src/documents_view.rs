//! The `ConfigDocuments` class: the crate's documents view (a tab strip over
//! one editor per document) mounted into an element, `update` writing the
//! signals it reads.

use std::collections::HashMap;

use config_editor_leptos::{
    CloseRequest, ConfigDocuments as DocumentsView, Document, DocumentsHandle, EditorSettings,
    SaveRequest, SecretOptions,
};
use js_sys::{Array, Function, Object};
use leptos::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::catalog::language_name;
use crate::editor::{MOUNTED, Mounted, NEXT_ID, replace_editors_in};
use crate::editor_view::DocumentEditorView;
use crate::host_diagnostics::host_diagnostics_from;
use crate::languages::{language_named, writable_edit_as};
use crate::mount::{IN_CALLBACK, call_host, call_host_with, saving_settings};
use crate::options::{
    bool_option, function_option, string_option, warn_unknown_keys, warn_unknown_options,
};
use crate::protect::{paths_from, ranges_from, secrets_from};
use crate::settings::{setting_keys, settings_from, settings_over, settings_result};
use crate::typings::{DocumentEditorResult, DocumentsOptions, DocumentsUpdate, SettingsResult};
use crate::update::{callback_change, given, plain_object, refuse_remount, settings_change};
use crate::values::{field, optional_text, text_arg, to_js};

/// The options `new ConfigDocuments` takes apart from the settings.
pub(crate) const DOCUMENTS_OPTIONS: &[&str] = &[
    "documents",
    "activeId",
    "className",
    "label",
    "onSelect",
    "onClose",
    "onChange",
    "onSave",
    "onDirtyChange",
];

/// The keys of one document.
const DOCUMENT_KEYS: &[&str] = &[
    "id",
    "fileName",
    "value",
    "language",
    "baseline",
    "readOnly",
    "schema",
    "editAs",
    "settings",
    "hostDiagnostics",
    "readOnlyPaths",
    "readOnlyRanges",
    "secrets",
];

/// The options the view is built from, which `update` refuses.
const DOCUMENTS_REMOUNT_OPTIONS: &[&str] = &["className", "label"];

/// What a list from JavaScript reads into: the documents, and each one's
/// own `settings` as given (laid over the view's again when those change).
struct Listed {
    documents: Vec<Document>,
    overrides: Vec<(String, JsValue)>,
}

/// The documents JavaScript lists, as the component takes them. Schemas
/// stay JSON text: the view compiles each distinct text once and shares it.
/// A document's `settings` go over `view`; its `secrets` are built once per
/// id (`secrets`), as its editor reads them when it mounts.
fn documents_from(
    list: &JsValue,
    view: &EditorSettings,
    secrets: &mut HashMap<String, SecretOptions>,
) -> Result<Listed, JsError> {
    if !Array::is_array(list) {
        return Err(JsError::new(
            "documents must be an array of { id, fileName, value, … }",
        ));
    }
    let list: &Array = list.unchecked_ref();
    let mut documents = Vec::with_capacity(list.length() as usize);
    let mut overrides = Vec::new();
    for (index, item) in list.iter().enumerate() {
        let name = |key: &str| format!("documents[{index}].{key}");
        if !item.is_object() || Array::is_array(&item) {
            return Err(JsError::new(&format!(
                "documents[{index}] must be an object: {{ id, fileName, value, … }}"
            )));
        }
        warn_unknown_keys(&item, DOCUMENT_KEYS, "document key");
        let id = text_arg(&field(&item, "id"), &name("id"))?;
        let file_name = text_arg(&field(&item, "fileName"), &name("fileName"))?;
        let value = optional_text(&field(&item, "value"), &name("value"))?.unwrap_or_default();
        let mut document = Document::new(id, file_name, value);
        if let Some(language) = optional_text(&field(&item, "language"), &name("language"))? {
            document = document.language(language_named(&language)?);
        }
        if let Some(baseline) = optional_text(&field(&item, "baseline"), &name("baseline"))? {
            document = document.baseline(baseline);
        }
        if let Some(read_only) = bool_option(&item, "readOnly")? {
            document = document.read_only(read_only);
        }
        if let Some(schema) = optional_text(&field(&item, "schema"), &name("schema"))? {
            document = document.schema(schema);
        }
        if let Some(edit_as) = optional_text(&field(&item, "editAs"), &name("editAs"))? {
            let edit_as = writable_edit_as(language_named(&edit_as)?)?;
            if edit_as != document.resolved_language() {
                document = document.edit_as(edit_as);
            }
        }
        document = with_protection(document, &item, index, secrets)?;
        let own = field(&item, "settings");
        if !own.is_undefined() && !own.is_null() {
            if !own.is_object() || Array::is_array(&own) {
                return Err(JsError::new(&format!(
                    "{}: must be an object of settings",
                    name("settings")
                )));
            }
            let keys = setting_keys();
            let known: Vec<&str> = keys.iter().map(String::as_str).collect();
            warn_unknown_keys(&own, &known, "setting");
            let settings =
                settings_over(view, &own).map_err(|error| prefixed(&name("settings"), error))?;
            // A copy: the host may change its object afterwards.
            let own = Object::assign(&Object::new(), own.unchecked_ref());
            overrides.push((document.id.clone(), own.into()));
            document = document.settings(settings);
        }
        documents.push(document);
    }
    secrets.retain(|id, _| documents.iter().any(|document| document.id == *id));
    Ok(Listed {
        documents,
        overrides,
    })
}

/// A document's host problems, read-only fields and secrets.
fn with_protection(
    mut document: Document,
    item: &JsValue,
    index: usize,
    secrets: &mut HashMap<String, SecretOptions>,
) -> Result<Document, JsError> {
    let name = |key: &str| format!("documents[{index}].{key}");
    let diagnostics = field(item, "hostDiagnostics");
    if !diagnostics.is_undefined() && !diagnostics.is_null() {
        let list = host_diagnostics_from(&diagnostics, &document.value)
            .map_err(|error| prefixed(&name("hostDiagnostics"), error))?;
        document = document.host_diagnostics(list);
    }
    let paths = paths_from(&field(item, "readOnlyPaths"), &name("readOnlyPaths"))?;
    let ranges = ranges_from(
        &field(item, "readOnlyRanges"),
        &document.value,
        &name("readOnlyRanges"),
    )?;
    document = document.read_only_paths(paths).read_only_ranges(ranges);
    let given = field(item, "secrets");
    if !given.is_undefined() && !given.is_null() {
        let options = match secrets.get(&document.id) {
            Some(options) => options.clone(),
            None => {
                let options = secrets_from(&given)?;
                secrets.insert(document.id.clone(), options.clone());
                options
            }
        };
        document = document.secrets(options);
    }
    Ok(document)
}

/// `error` with `name` in front, so it says which document it is about.
fn prefixed(name: &str, error: JsError) -> JsError {
    let message = JsValue::from(error);
    let text = field(&message, "message").as_string().unwrap_or_default();
    JsError::new(&format!("{name}: {text}"))
}

/// What `onClose` receives.
#[derive(Serialize)]
struct CloseOut {
    id: String,
    dirty: bool,
}

/// The live state a mounted view reads; `update` writes it.
#[derive(Clone, Copy)]
pub(crate) struct State {
    documents: RwSignal<Vec<Document>>,
    /// The view's settings, the default of every document.
    settings: RwSignal<EditorSettings>,
    /// Each document's own `settings`, as JavaScript gave them.
    overrides: StoredValue<Vec<(String, JsValue)>, LocalStorage>,
    /// Each document's secret options, built once (see `documents_from`).
    secrets: StoredValue<HashMap<String, SecretOptions>, LocalStorage>,
    /// Whether `onSave` is set: only then does `Ctrl+S` save.
    pub(crate) saves: RwSignal<bool>,
    on_select: StoredValue<Option<Function>, LocalStorage>,
    on_close: StoredValue<Option<Function>, LocalStorage>,
    on_change: StoredValue<Option<Function>, LocalStorage>,
    on_save: StoredValue<Option<Function>, LocalStorage>,
    on_dirty_change: StoredValue<Option<Function>, LocalStorage>,
    /// Whether the view was mounted with `onClose`: tabs have close buttons.
    closable: bool,
    /// Each document's text and baseline as the host last saw them.
    texts: StoredValue<HashMap<String, Texts>, LocalStorage>,
    pub(crate) handle: DocumentsHandle,
}

/// A document's text (the host's, then each edit's) and the baseline it is
/// diffed against (given, else the text it was added with), as the view's
/// own entries keep them.
#[derive(Clone)]
pub(crate) struct Texts {
    pub(crate) value: String,
    pub(crate) baseline: String,
    /// Which listing of the id this is: a document removed and listed again
    /// with the same id is a new one, which an `EditorView` of the old one
    /// does not read.
    pub(crate) listing: u64,
}

thread_local! {
    /// The last [`Texts::listing`] given.
    static LISTINGS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

impl State {
    /// Stops the callbacks once `destroy` is called (from inside one, the
    /// view unmounts a moment later): none fires again, but `onChange` with
    /// an edit still waiting to be saved as the view unmounts.
    fn silence(&self) {
        for callback in [
            self.on_select,
            self.on_close,
            self.on_save,
            self.on_dirty_change,
        ] {
            let _ = callback.try_set_value(None);
        }
    }

    /// The document `id`'s text and baseline, while it is listed.
    pub(crate) fn texts_of(&self, id: &str) -> Option<Texts> {
        self.texts
            .try_with_value(|texts| texts.get(id).cloned())
            .flatten()
    }

    /// The settings of the document `id`'s editor: its own over the view's.
    pub(crate) fn settings_of(&self, id: &str) -> Option<EditorSettings> {
        let own = self
            .documents
            .try_with_untracked(|documents| {
                documents
                    .iter()
                    .find(|document| document.id == id)
                    .map(|document| document.settings.clone())
            })
            .flatten()?;
        own.or_else(|| self.settings.try_get_untracked())
    }

    /// Reads a list from JavaScript against the view's settings `view`.
    fn read(&self, list: &JsValue, view: &EditorSettings) -> Result<Listed, JsError> {
        let mut secrets = self.secrets.try_get_value().unwrap_or_default();
        let listed = documents_from(list, view, &mut secrets)?;
        let _ = self.secrets.try_set_value(secrets);
        Ok(listed)
    }

    fn show(&self, listed: Listed) {
        remember_texts(self.texts, &listed.documents);
        let _ = self.overrides.try_set_value(listed.overrides);
        let _ = self.documents.try_set(listed.documents);
    }
}

/// Keeps each listed document's text and baseline (see [`Texts`]).
fn remember_texts(
    texts: StoredValue<HashMap<String, Texts>, LocalStorage>,
    documents: &[Document],
) {
    let _ = texts.try_update_value(|texts| {
        texts.retain(|id, _| documents.iter().any(|document| document.id == *id));
        for document in documents {
            let baseline = document
                .baseline
                .clone()
                .or_else(|| texts.get(&document.id).map(|known| known.baseline.clone()))
                .unwrap_or_else(|| document.value.clone());
            let listing = texts.get(&document.id).map_or_else(
                || {
                    LISTINGS.with(|last| {
                        last.set(last.get() + 1);
                        last.get()
                    })
                },
                |known| known.listing,
            );
            texts.insert(
                document.id.clone(),
                Texts {
                    value: document.value.clone(),
                    baseline,
                    listing,
                },
            );
        }
    });
}

/// What the component takes once, at mount.
struct Setup {
    active: Option<String>,
    class: String,
    label: Option<String>,
    /// `onClose` was given: tabs get close buttons.
    closable: bool,
}

fn view(state: State, setup: Setup) -> AnyView {
    let host =
        |callback: StoredValue<Option<Function>, LocalStorage>| callback.try_get_value().flatten();
    let selected = move |id: String| {
        if let Some(callback) = host(state.on_select) {
            call_host(&callback, &JsValue::from_str(&id));
        }
    };
    let changed = move |(id, text): (String, String)| {
        let _ = state.texts.try_update_value(|texts| {
            if let Some(known) = texts.get_mut(&id) {
                known.value.clone_from(&text);
            }
        });
        if let Some(callback) = host(state.on_change) {
            call_host_with(
                &callback,
                &[JsValue::from_str(&id), JsValue::from_str(&text)],
            );
        }
    };
    let saved = move |(id, request): (String, SaveRequest)| {
        if let Some(callback) = host(state.on_save) {
            call_host_with(
                &callback,
                &[
                    JsValue::from_str(&id),
                    JsValue::from_str(&request.text),
                    JsValue::from_str(&language_name(request.language)),
                ],
            );
        }
    };
    let dirty_changed = move |(id, dirty): (String, bool)| {
        if let Some(callback) = host(state.on_dirty_change) {
            call_host_with(
                &callback,
                &[JsValue::from_str(&id), JsValue::from_bool(dirty)],
            );
        }
    };
    let close = setup.closable.then(|| {
        Callback::new(move |request: CloseRequest| {
            if let Some(callback) = host(state.on_close) {
                let out = CloseOut {
                    id: request.id,
                    dirty: request.dirty,
                };
                call_host(&callback, &to_js(&out));
            }
        })
    });
    let documents = Signal::derive(move || state.documents.get());
    let settings = saving_settings(state.settings, state.saves);
    view! {
        <DocumentsView
            documents=documents
            nostrip:initial_active=setup.active
            on_select=selected
            nostrip:on_close=close
            on_change=changed
            on_save=saved
            on_dirty_change=dirty_changed
            settings=settings
            class=setup.class
            nostrip:label=setup.label
            handle=state.handle
        />
    }
    .into_any()
}

/// Several documents as tabs, one editor per document, mounted into an
/// element.
///
/// ```js
/// import { ConfigDocuments, loadConfigEditor } from "@pngwasi/config-editor";
/// await loadConfigEditor();
/// let files = [
///   { id: "main", fileName: "burrow.yaml", value: mainText, schema },
///   { id: "tls", fileName: "conf.d/tls.toml", value: tlsText, schema },
/// ];
/// const tabs = new ConfigDocuments(document.getElementById("files"), {
///   documents: files,
///   onChange: (id, value) => {
///     files = files.map((file) => (file.id === id ? { ...file, value } : file));
///     tabs.update({ documents: files });
///   },
///   onSave: (id, value) => upload(id, value),
///   onClose: ({ id, dirty }) => { if (!dirty || confirm("Discard changes?")) close(id); },
/// });
/// ```
#[wasm_bindgen]
pub struct ConfigDocuments {
    state: State,
    /// The view's reactive owner, which a document's `secrets` check lives in.
    owner: Owner,
    /// The view's entry in [`MOUNTED`] until `destroy`.
    id: Option<u32>,
}

#[wasm_bindgen]
impl ConfigDocuments {
    /// Mounts the view into `element` (its content is replaced).
    #[wasm_bindgen(constructor)]
    pub fn new(
        #[wasm_bindgen(unchecked_param_type = "HTMLElement")] element: JsValue,
        options: DocumentsOptions,
    ) -> Result<ConfigDocuments, JsError> {
        let element: web_sys::HtmlElement = element.dyn_into().map_err(|_| {
            JsError::new(
                "ConfigDocuments needs an HTMLElement to mount into (got null or another value)",
            )
        })?;
        let options: JsValue = options.into();
        let settings = settings_from(&options)?;
        let owner = Owner::new();
        let mut secrets = HashMap::new();
        let listed = match field(&options, "documents") {
            list if list.is_undefined() || list.is_null() => Listed {
                documents: Vec::new(),
                overrides: Vec::new(),
            },
            list => owner.with(|| documents_from(&list, &settings, &mut secrets))?,
        };
        let on_select = function_option(&options, "onSelect")?;
        let on_close = function_option(&options, "onClose")?;
        let on_change = function_option(&options, "onChange")?;
        let on_save = function_option(&options, "onSave")?;
        let on_dirty_change = function_option(&options, "onDirtyChange")?;
        let setup = Setup {
            active: string_option(&options, "activeId")?,
            class: string_option(&options, "className")?.unwrap_or_default(),
            label: string_option(&options, "label")?,
            closable: on_close.is_some(),
        };
        warn_unknown_options(&options, DOCUMENTS_OPTIONS);
        replace_editors_in(&element);
        element.set_text_content(None);

        let state = owner.with(|| State {
            documents: RwSignal::new(listed.documents),
            settings: RwSignal::new(settings),
            overrides: StoredValue::new_local(listed.overrides),
            secrets: StoredValue::new_local(secrets),
            saves: RwSignal::new(on_save.is_some()),
            on_select: StoredValue::new_local(on_select),
            on_close: StoredValue::new_local(on_close),
            on_change: StoredValue::new_local(on_change),
            on_save: StoredValue::new_local(on_save),
            on_dirty_change: StoredValue::new_local(on_dirty_change),
            closable: setup.closable,
            texts: StoredValue::new_local(HashMap::new()),
            handle: DocumentsHandle::new(),
        });
        remember_texts(state.texts, &state.documents.get_untracked());
        let mount_element = element.clone();
        let mounted = owner.with(|| leptos::mount::mount_to(element, move || view(state, setup)));
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1));
            id
        });
        let entry: Mounted = (Box::new(mounted), owner.clone(), mount_element);
        MOUNTED.with(|views| {
            views.borrow_mut().insert(id, entry);
        });
        Ok(ConfigDocuments {
            state,
            owner,
            id: Some(id),
        })
    }

    /// Shows the document `id` and focuses its editor; `false` when no
    /// document has that id.
    pub fn select(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] id: JsValue,
    ) -> Result<bool, JsError> {
        config_editor_leptos::js_support::flush_pending_input();
        Ok(self.state.handle.select(&text_arg(&id, "id")?))
    }

    /// The active document's id (`undefined` while none is open).
    #[wasm_bindgen(getter, js_name = activeId)]
    pub fn active_id(&self) -> Option<String> {
        untrack(|| self.state.handle.active())
    }

    /// Whether the document `id` differs from its baseline.
    #[wasm_bindgen(js_name = isDirty)]
    pub fn is_dirty(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] id: JsValue,
    ) -> Result<bool, JsError> {
        config_editor_leptos::js_support::flush_pending_input();
        let id = text_arg(&id, "id")?;
        Ok(untrack(|| self.state.handle.is_dirty(&id)))
    }

    /// Saves the document `id` as `Ctrl+S` in its editor does: calls
    /// `onSave`. `false` when nothing was sent (no `onSave`, no such
    /// document, or an `editAs` edit that cannot convert back yet).
    pub fn save(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] id: JsValue,
    ) -> Result<bool, JsError> {
        config_editor_leptos::js_support::flush_pending_input();
        let id = text_arg(&id, "id")?;
        if self.state.on_save.try_with_value(Option::is_some) != Some(true) {
            return Ok(false);
        }
        Ok(untrack(|| self.state.handle.save(&id)))
    }

    /// Saves every document with unsaved edits through `onSave`; returns
    /// the ids sent.
    #[wasm_bindgen(js_name = saveAll)]
    pub fn save_all(&self) -> Vec<String> {
        config_editor_leptos::js_support::flush_pending_input();
        if self.state.on_save.try_with_value(Option::is_some) != Some(true) {
            return Vec::new();
        }
        untrack(|| self.state.handle.save_all())
    }

    /// Focuses the active document's editor, keeping its selection.
    pub fn focus(&self) {
        self.state.handle.focus();
    }

    /// The view's settings in effect: every key of `EditorSettings`, set.
    #[wasm_bindgen(getter)]
    pub fn settings(&self) -> SettingsResult {
        settings_result(&self.state.settings.try_get_untracked().unwrap_or_default())
    }

    /// The editor of the document `id`: paths, folds, focus and saving for
    /// that document (`null` when no document has that id). Its
    /// queries answer `null` (or `false`) until the document has been shown
    /// once, which mounts its editor, and again once the document is
    /// removed (one listed again with the same id is another document).
    pub fn editor(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] id: JsValue,
    ) -> Result<DocumentEditorResult, JsError> {
        let id = text_arg(&id, "id")?;
        Ok(match self.state.texts_of(&id) {
            Some(texts) => JsValue::from(DocumentEditorView::new(self.state, id, texts.listing)),
            None => JsValue::NULL,
        }
        .unchecked_into())
    }

    /// Changes the view, live: `documents`, `activeId`, the settings and
    /// the callbacks, any of them. A key left out or `undefined` stays as
    /// it is; `null` removes a callback or returns a setting to its default.
    /// Settings merge key by key (`strings` text by text), under each
    /// document's own `settings`. A new `documents` list replaces the list:
    /// a document keeps its editor (caret, undo history, scroll, folds)
    /// while its `id` stays listed, its text, baseline, file name and
    /// read-only state update in place, and a new `language` or `schema`
    /// mounts a new editor. `activeId` shows that document, leaving the
    /// focus where it is (an id not listed throws). `className` and `label`, and an `onClose` for a
    /// view mounted without one (its tabs have no close buttons), throw:
    /// create a new view instead. Every key is checked before any applies.
    pub fn update(&self, options: DocumentsUpdate) -> Result<(), JsError> {
        let options: JsValue = options.into();
        plain_object(
            &options,
            "update takes an object of options: { documents?, activeId?, theme?, onSave?, … }",
        )?;
        refuse_remount(&options, DOCUMENTS_REMOUNT_OPTIONS, "ConfigDocuments")?;
        warn_unknown_options(&options, DOCUMENTS_OPTIONS);
        config_editor_leptos::js_support::flush_pending_input();
        if self.id.is_none() {
            return Ok(());
        }
        let active = given(&options, "activeId")
            .map(|id| text_arg(&id, "activeId"))
            .transpose()?;
        let mut callbacks = Vec::new();
        for (name, slot) in [
            ("onSelect", self.state.on_select),
            ("onClose", self.state.on_close),
            ("onChange", self.state.on_change),
            ("onDirtyChange", self.state.on_dirty_change),
        ] {
            callbacks.push((callback_change(&options, name)?, slot));
        }
        if !self.state.closable && matches!(callbacks[1].0, Some(Some(_))) {
            return Err(JsError::new(
                "onClose cannot be added to a ConfigDocuments mounted without one: its tabs are \
                 built without close buttons. Create a new ConfigDocuments with onClose",
            ));
        }
        let on_save = callback_change(&options, "onSave")?;
        let current = self.state.settings.try_get_untracked().unwrap_or_default();
        let settings = settings_change(&current, &options)?;
        let view = settings.clone().unwrap_or(current);
        // The new list, read against the new settings; else the documents'
        // own settings laid over the new ones (checked before anything
        // changes).
        let listed = match given(&options, "documents") {
            Some(list) if list.is_null() => Some(Listed {
                documents: Vec::new(),
                overrides: Vec::new(),
            }),
            Some(list) => Some(self.owner.with(|| self.state.read(&list, &view))?),
            None => None,
        };
        let mut own = Vec::new();
        if listed.is_none() && settings.is_some() {
            for (id, given) in &self.state.overrides.try_get_value().unwrap_or_default() {
                own.push((id.clone(), settings_over(&view, given)?));
            }
        }
        // `activeId` names a document of the list (the new one, if given).
        if let Some(id) = &active {
            let listed_ids =
                |documents: &[Document]| documents.iter().any(|document| document.id == *id);
            let known = match &listed {
                Some(listed) => listed_ids(&listed.documents),
                None => self
                    .state
                    .documents
                    .try_with_untracked(|documents| listed_ids(documents))
                    .unwrap_or(false),
            };
            if !known {
                return Err(JsError::new(&format!(
                    "activeId: no document has the id {id:?}"
                )));
            }
        }

        // Everything checked: apply.
        if let Some(settings) = settings {
            let _ = self.state.settings.try_set(settings);
        }
        if let Some(listed) = listed {
            self.state.show(listed);
            // Taken now: `activeId` and every query answer for the new list
            // right after `update` returns.
            if let Some(documents) = self.state.documents.try_get_untracked() {
                self.owner.with(|| {
                    config_editor_leptos::js_support::take_documents(
                        &self.state.handle,
                        &documents,
                    );
                });
            }
        } else if !own.is_empty() {
            let _ = self.state.documents.try_update(|documents| {
                for document in documents.iter_mut() {
                    if let Some((_, settings)) = own.iter().find(|(id, _)| *id == document.id) {
                        document.settings = Some(settings.clone());
                    }
                }
            });
        }
        for (change, slot) in callbacks {
            if let Some(callback) = change {
                let _ = slot.try_set_value(callback);
            }
        }
        if let Some(callback) = on_save {
            let _ = self.state.saves.try_set(callback.is_some());
            let _ = self.state.on_save.try_set_value(callback);
        }
        if let Some(id) = active {
            self.state.handle.show(&id);
        }
        Ok(())
    }

    /// Unmounts the view and its editors. The instance cannot be used
    /// afterwards.
    pub fn destroy(&mut self) {
        config_editor_leptos::js_support::flush_pending_input();
        let Some((mounted, owner, _)) = self
            .id
            .take()
            .and_then(|id| MOUNTED.with(|views| views.borrow_mut().remove(&id)))
        else {
            return;
        };
        let unmount = move || {
            drop(mounted);
            owner.cleanup();
        };
        if IN_CALLBACK.with(|depth| depth.get()) > 0 {
            // Called from a callback: the view's handler is still running,
            // and calls nothing more.
            self.state.silence();
            let deferred = Closure::once_into_js(unmount);
            if let Ok(queue) = field(&js_sys::global(), "queueMicrotask").dyn_into::<Function>() {
                let _ = queue.call1(&JsValue::NULL, &deferred);
            }
        } else {
            unmount();
        }
    }
}
