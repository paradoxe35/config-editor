//! The `ConfigDocuments` class: the crate's documents view (a tab strip over
//! one editor per document) mounted into an element, `update` writing the
//! signals it reads.

use std::collections::HashMap;

use config_editor_leptos::js_support::Timer;
use config_editor_leptos::{
    CloseRequest, ConfigDocuments as DocumentsView, Diagnostic, Document, DocumentsHandle,
    EditorSettings, Language, SaveRequest,
};
use js_sys::Function;
use leptos::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::catalog::language_name;
use crate::documents_list::{Kept, Listed, documents_from};
use crate::echoes::Echoes;
use crate::editor::{MOUNTED, Mounted, NEXT_ID, replace_editors_in};
use crate::editor_view::DocumentEditorView;
use crate::languages::{language_named, writable_edit_as};
use crate::mount::{IN_CALLBACK, call_host, call_host_with, saving_settings};
use crate::options::{bool_option, function_option, string_option, warn_unknown_options};
use crate::settings::{settings_from, settings_over, settings_result};
use crate::told::{self, Told};
use crate::typings::{DocumentEditorResult, DocumentsOptions, DocumentsUpdate, SettingsResult};
use crate::update::{Fixed, callback_change, given, plain_object, refuse_remount, settings_change};
use crate::values::{diagnostics_to_js, field, text_arg, to_js};

mod echoing;

/// The options `new ConfigDocuments` takes apart from the settings.
pub(crate) const DOCUMENTS_OPTIONS: &[&str] = &[
    "documents",
    "activeId",
    "className",
    "label",
    "echoes",
    "onSelect",
    "onClose",
    "onAdd",
    "onChange",
    "onSave",
    "onDirtyChange",
    "onProblems",
    "onEditAsChange",
    "onSyncChange",
];

/// The options the view is built from, which `update` refuses.
const DOCUMENTS_REMOUNT_OPTIONS: &[&str] = &["className", "label", "echoes"];

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
    /// What reading the list keeps (see `Kept`).
    kept: StoredValue<Kept, LocalStorage>,
    /// Whether `onSave` is set: only then does `Ctrl+S` save.
    pub(crate) saves: RwSignal<bool>,
    on_select: StoredValue<Option<Function>, LocalStorage>,
    on_close: StoredValue<Option<Function>, LocalStorage>,
    on_add: StoredValue<Option<Function>, LocalStorage>,
    on_change: StoredValue<Option<Function>, LocalStorage>,
    on_save: StoredValue<Option<Function>, LocalStorage>,
    on_dirty_change: StoredValue<Option<Function>, LocalStorage>,
    on_problems: StoredValue<Option<Function>, LocalStorage>,
    on_edit_as_change: StoredValue<Option<Function>, LocalStorage>,
    on_sync_change: StoredValue<Option<Function>, LocalStorage>,
    /// Whether the view was mounted with `onClose`: closable documents'
    /// tabs have close buttons.
    closes: bool,
    /// Whether the view was mounted with `onAdd`: the strip has a "+".
    adds: bool,
    /// Each document's text and baseline as the host last saw them.
    texts: StoredValue<HashMap<String, Texts>, LocalStorage>,
    /// What the state callbacks last reported of each document (see `told`).
    told: StoredValue<HashMap<String, Told>, LocalStorage>,
    /// Whether the host lists every text `onChange` gives back (`echoes`).
    echoing: bool,
    /// With `echoing`, each document's texts `onChange` sent that the host
    /// has not listed back yet (see `echoes`).
    echoes: StoredValue<HashMap<String, Echoes>, LocalStorage>,
    /// The check that the host came back to each document (see `settle`).
    settle: crate::settle::Slot,
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
            self.on_add,
            self.on_save,
            self.on_dirty_change,
            self.on_problems,
            self.on_edit_as_change,
            self.on_sync_change,
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

    /// The language of the document `id`, while it is listed.
    fn language_of(&self, id: &str) -> Option<Language> {
        self.documents
            .try_with_untracked(|documents| {
                documents
                    .iter()
                    .find(|document| document.id == id)
                    .map(Document::resolved_language)
            })
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
        let mut kept = self
            .kept
            .try_update_value(std::mem::take)
            .unwrap_or_default();
        // Whether a document's editor holds the text listed for it.
        let texts = self.texts;
        let holds = |id: &str, value: &str| {
            texts
                .try_with_value(|texts| {
                    texts
                        .get(id)
                        .is_some_and(|known| crate::remap::reads_as(&known.value, value))
                })
                .unwrap_or(false)
        };
        let listed = documents_from(list, view, &mut kept, &holds);
        if listed.is_err() {
            kept.abandon();
        }
        let _ = self.kept.try_set_value(kept);
        listed
    }

    fn show(&self, listed: Listed) {
        remember_texts(self.texts, &listed.documents);
        // A document closed takes what its editor reported with it: a
        // document listed again with its id is a new one.
        let _ = self.told.try_update_value(|told| {
            told.retain(|id, _| listed.documents.iter().any(|document| document.id == *id));
        });
        let _ = self.overrides.try_set_value(listed.overrides);
        let _ = self.documents.try_set(listed.documents);
    }
}

/// Records what a state callback reports of the document `id`, while it is
/// listed (`show` forgets it once it is not).
fn note(state: State, id: &str, change: impl FnOnce(&mut Told)) {
    if state.texts_of(id).is_none() {
        return;
    }
    let _ = state.told.try_update_value(|told| {
        change(told.entry(id.to_string()).or_default());
    });
}

/// Keeps each listed document's text and baseline (see [`Texts`]).
fn remember_texts(
    texts: StoredValue<HashMap<String, Texts>, LocalStorage>,
    documents: &[Document],
) {
    let _ = texts.try_update_value(|texts| {
        let listed: std::collections::HashSet<&str> = documents
            .iter()
            .map(|document| document.id.as_str())
            .collect();
        texts.retain(|id, _| listed.contains(id.as_str()));
        // A repeated id is ignored, as the view ignores it (it shows and
        // saves the first document listed with it).
        let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for document in documents {
            if !seen.insert(document.id.as_str()) {
                continue;
            }
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
    /// `onClose` was given: closable documents' tabs get close buttons.
    closes: bool,
    /// `onAdd` was given: the strip gets a "+".
    adds: bool,
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
            // The host lists this string as the document's value next.
            let value = JsValue::from_str(&text);
            if state.echoing {
                let _ = state.echoes.try_update_value(|echoes| {
                    echoes
                        .entry(id.clone())
                        .or_insert_with(|| Echoes::new(true))
                        .sent(&text);
                });
            }
            let _ = state.kept.try_update_value(|kept| {
                kept.texts.remember(&id, "value", value.clone(), text);
            });
            call_host_with(&callback, &[JsValue::from_str(&id), value]);
        } else if state.echoing {
            let _ = state.echoes.try_update_value(|echoes| {
                if let Some(echo) = echoes.get_mut(&id) {
                    echo.unsent();
                }
            });
        }
    };
    let saved = move |(id, request): (String, SaveRequest)| {
        if let Some(callback) = host(state.on_save) {
            DELIVERED.with(|delivered| {
                if let Some(ids) = delivered.borrow_mut().as_mut() {
                    ids.push(id.clone());
                }
            });
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
    let problems_changed = move |(id, problems): (String, Vec<Diagnostic>)| {
        note(state, &id, |told| told.problems = Some(problems.clone()));
        if let Some(callback) = host(state.on_problems) {
            call_host_with(
                &callback,
                &[JsValue::from_str(&id), diagnostics_to_js(problems)],
            );
        }
    };
    let dirty_changed = move |(id, dirty): (String, bool)| {
        note(state, &id, |told| told.dirty = Some(dirty));
        if let Some(callback) = host(state.on_dirty_change) {
            call_host_with(
                &callback,
                &[JsValue::from_str(&id), JsValue::from_bool(dirty)],
            );
        }
    };
    let language_changed = move |(id, language): (String, Language)| {
        note(state, &id, |told| told.language = Some(language));
        if let Some(callback) = host(state.on_edit_as_change) {
            call_host_with(
                &callback,
                &[
                    JsValue::from_str(&id),
                    JsValue::from_str(&language_name(language)),
                ],
            );
        }
    };
    let sync_changed = move |(id, in_sync): (String, bool)| {
        note(state, &id, |told| told.in_sync = Some(in_sync));
        if let Some(callback) = host(state.on_sync_change) {
            call_host_with(
                &callback,
                &[JsValue::from_str(&id), JsValue::from_bool(in_sync)],
            );
        }
    };
    let add = setup.adds.then(|| {
        Callback::new(move |()| {
            if let Some(callback) = host(state.on_add) {
                call_host_with(&callback, &[]);
            }
        })
    });
    let close = setup.closes.then(|| {
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
            nostrip:on_add=add
            on_change=changed
            on_save=saved
            on_dirty_change=dirty_changed
            on_problems=problems_changed
            on_edit_as_change=language_changed
            on_sync_change=sync_changed
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
///   { id: "main", fileName: "config.yaml", value: mainText, schema },
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
    /// The view's entry in [`MOUNTED`] until `destroy`: a cell, as
    /// `destroy` takes `&self` so a callback that a method of this instance
    /// fired (`saveAll()` calling `onSave`) can call it.
    id: std::cell::Cell<Option<u32>>,
    /// Calls made while one of the view's flushes input (see `reentry`).
    reentry: crate::reentry::Reentry,
    /// The options it cannot change, as given (see `update`).
    built_from: crate::update::BuiltFrom,
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
        let mut kept = Kept::default();
        let listed = match field(&options, "documents") {
            list if list.is_undefined() || list.is_null() => Listed::default(),
            list => owner.with(|| documents_from(&list, &settings, &mut kept, &|_, _| false))?,
        };
        let on_select = function_option(&options, "onSelect")?;
        let on_close = function_option(&options, "onClose")?;
        let on_add = function_option(&options, "onAdd")?;
        let on_change = function_option(&options, "onChange")?;
        let on_save = function_option(&options, "onSave")?;
        let on_dirty_change = function_option(&options, "onDirtyChange")?;
        let on_problems = function_option(&options, "onProblems")?;
        let on_edit_as_change = function_option(&options, "onEditAsChange")?;
        let on_sync_change = function_option(&options, "onSyncChange")?;
        let echoing = bool_option(&options, "echoes")?.unwrap_or(false);
        let setup = Setup {
            active: string_option(&options, "activeId")?,
            class: string_option(&options, "className")?.unwrap_or_default(),
            label: string_option(&options, "label")?,
            closes: on_close.is_some(),
            adds: on_add.is_some(),
        };
        // `activeId` names a document of the list, as `update` checks.
        if let Some(id) = &setup.active
            && !listed.documents.iter().any(|document| document.id == *id)
        {
            return Err(JsError::new(&format!(
                "activeId: no document has the id {id:?}"
            )));
        }
        warn_unknown_options(&options, DOCUMENTS_OPTIONS);
        replace_editors_in(&element);
        element.set_text_content(None);

        let state = owner.with(|| State {
            documents: RwSignal::new(listed.documents),
            settings: RwSignal::new(settings),
            overrides: StoredValue::new_local(listed.overrides),
            kept: StoredValue::new_local(kept),
            saves: RwSignal::new(on_save.is_some()),
            on_select: StoredValue::new_local(on_select),
            on_close: StoredValue::new_local(on_close),
            on_add: StoredValue::new_local(on_add),
            on_change: StoredValue::new_local(on_change),
            on_save: StoredValue::new_local(on_save),
            on_dirty_change: StoredValue::new_local(on_dirty_change),
            on_problems: StoredValue::new_local(on_problems),
            on_edit_as_change: StoredValue::new_local(on_edit_as_change),
            on_sync_change: StoredValue::new_local(on_sync_change),
            closes: setup.closes,
            adds: setup.adds,
            texts: StoredValue::new_local(HashMap::new()),
            told: StoredValue::new_local(HashMap::new()),
            echoing,
            echoes: StoredValue::new_local(HashMap::new()),
            settle: StoredValue::new_local(Timer::default()),
            handle: DocumentsHandle::new(),
        });
        remember_texts(state.texts, &state.documents.get_untracked());
        if echoing {
            let given: HashMap<String, Echoes> = state.documents.with_untracked(|documents| {
                documents
                    .iter()
                    .map(|document| {
                        let mut echo = Echoes::new(true);
                        echo.given(&document.value);
                        (document.id.clone(), echo)
                    })
                    .collect()
            });
            let _ = state.echoes.try_set_value(given);
        }
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
            id: std::cell::Cell::new(Some(id)),
            reentry: crate::reentry::Reentry::default(),
            built_from: crate::update::BuiltFrom::of(
                &options,
                vec![
                    Fixed::or("className", vec![JsValue::from_str("")]),
                    Fixed::plain("label"),
                    Fixed::or("echoes", vec![JsValue::FALSE]),
                ],
            ),
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
        let (sent, delivered) = delivering(|| untrack(|| self.state.handle.save(&id)));
        Ok(sent && delivered.contains(&id))
    }

    /// Saves every document with unsaved edits through `onSave`; returns
    /// the ids sent.
    #[wasm_bindgen(js_name = saveAll)]
    pub fn save_all(&self) -> Vec<String> {
        config_editor_leptos::js_support::flush_pending_input();
        if self.state.on_save.try_with_value(Option::is_some) != Some(true) {
            return Vec::new();
        }
        let (sent, delivered) = delivering(|| untrack(|| self.state.handle.save_all()));
        sent.into_iter()
            .filter(|id| delivered.contains(id))
            .collect()
    }

    /// Edits the document `id` as `language` (its own language to go back),
    /// as a new `editAs` in the list does, and also when `language` is what
    /// its `editAs` already says: a list with an unchanged `editAs` changes
    /// nothing, so after an undo stepped back across the switch this asks
    /// for it again. A switch the text cannot make yet waits; asking for
    /// the language on screen ends one waiting. Give the document the same
    /// `editAs` in your list, or the next `update({ documents })` switches
    /// back. `false` when no document has that id.
    #[wasm_bindgen(js_name = editAs)]
    pub fn edit_as(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] id: JsValue,
        #[wasm_bindgen(unchecked_param_type = "LanguageName")] language: JsValue,
    ) -> Result<bool, JsError> {
        config_editor_leptos::js_support::flush_pending_input();
        let id = text_arg(&id, "id")?;
        let language = language_named(&text_arg(&language, "language")?)?;
        let Some(own) = self.state.language_of(&id) else {
            return Ok(false);
        };
        let language = writable_edit_as(own, language)?;
        Ok(untrack(|| self.state.handle.edit_as(&id, language)))
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
    /// that document (`null` when no document has that id). Until the
    /// document has been shown once, which mounts its editor, its `value`,
    /// `text`, `unifiedDiff`, `settings`, `isDirty` and `save` answer for
    /// the document as listed and its paths, folds and `reveal` answer
    /// `null` (or `false`); once the document is removed all of them do
    /// (one listed again with the same id is another document).
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

    /// Replaces the text of the document `id` with `value`, the host's own
    /// whatever it is: with `echoes`, a document's `value` in
    /// `update({ documents })` that its editor gave and has not seen come
    /// back yet is its echo; this never is (a revert to such a text). List
    /// the document with that text from then on. Its editor keeps the
    /// caret on the same text when possible; `onChange` does not hear it.
    /// `false` when no document has that id.
    #[wasm_bindgen(js_name = replaceValue)]
    pub fn replace_value(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] id: JsValue,
        #[wasm_bindgen(unchecked_param_type = "string")] value: JsValue,
    ) -> Result<bool, JsError> {
        let id = text_arg(&id, "id")?;
        let value = text_arg(&value, "value")?;
        let key = format!("value:{id}");
        self.reentry.note(&key);
        let newer = self
            .reentry
            .flush(config_editor_leptos::flush_pending_edits);
        if self.id.get().is_none() || self.state.texts_of(&id).is_none() {
            return Ok(false);
        }
        // The host gave the document a text (or the list) from `onChange`
        // meanwhile: newer than this call.
        if newer.contains(&key) || newer.contains("documents") {
            return Ok(true);
        }
        let _ = self.state.texts.try_update_value(|texts| {
            if let Some(known) = texts.get_mut(&id) {
                known.value.clone_from(&value);
            }
        });
        self.state.given(&id, &value);
        let edits = self.edits();
        let _ = self.state.documents.try_update(|documents| {
            keep_edits(documents, &edits);
            if let Some(document) = documents.iter_mut().find(|document| document.id == id) {
                document.value = value;
            }
        });
        if let Some(documents) = self.state.documents.try_get_untracked() {
            self.owner.with(|| {
                config_editor_leptos::js_support::take_documents(&self.state.handle, &documents);
            });
        }
        Ok(true)
    }

    /// Changes the view, live: `documents`, `activeId`, the settings and
    /// the callbacks, any of them. A key left out or `undefined` stays as
    /// it is; `null` removes a callback or returns a setting to its default.
    /// Settings merge key by key (`strings` text by text, `colors` token by
    /// token), under each
    /// document's own `settings`. A new `documents` list replaces the list:
    /// a document keeps its editor (caret, undo history, scroll, folds)
    /// while its `id` stays listed, its text, baseline, file name and
    /// read-only state update in place, and a new `language` or `schema`
    /// mounts a new editor. With `echoes`, a document's `value` that its
    /// editor gave and has not seen listed back yet is its echo and keeps
    /// what the editor holds (see `replaceValue`). `activeId` shows that document, leaving the
    /// focus where it is (an id not listed throws). `className`, `label` and `echoes`, and an `onClose` or
    /// `onAdd` for a view mounted without one (its tabs have no close
    /// buttons, its strip no "+"), throw: create a new view instead (the value the
    /// view was built with, given again, changes nothing; one left out was built with its default:
    /// `null`, `echoes: false`, `className: ""`). Every key is checked before any applies.
    pub fn update(&self, options: DocumentsUpdate) -> Result<(), JsError> {
        let options: JsValue = options.into();
        plain_object(
            &options,
            "update takes an object of options: { documents?, activeId?, theme?, onSave?, … }",
        )?;
        refuse_remount(
            &options,
            DOCUMENTS_REMOUNT_OPTIONS,
            &self.built_from,
            "ConfigDocuments",
        )?;
        warn_unknown_options(&options, DOCUMENTS_OPTIONS);
        // Input the browser sent in this task first (the texts it sends from
        // `flushed` on: the host did not know them). What the host set from
        // `onChange` meanwhile is newer than this call, and stays.
        self.reentry.note_options(&options);
        let flushed = crate::echoes::clock();
        let newer = self
            .reentry
            .flush(config_editor_leptos::js_support::flush_pending_input);
        if self.id.get().is_none() {
            return Ok(());
        }
        let options = crate::reentry::without_newer(&options, &newer);
        let replaced: Vec<&str> = newer
            .iter()
            .filter_map(|key| key.strip_prefix("value:"))
            .collect();
        let active = given(&options, "activeId")
            .map(|id| text_arg(&id, "activeId"))
            .transpose()?;
        let mut callbacks = Vec::new();
        for (name, slot) in [
            ("onSelect", self.state.on_select),
            ("onClose", self.state.on_close),
            ("onAdd", self.state.on_add),
            ("onChange", self.state.on_change),
            ("onDirtyChange", self.state.on_dirty_change),
            ("onProblems", self.state.on_problems),
            ("onEditAsChange", self.state.on_edit_as_change),
            ("onSyncChange", self.state.on_sync_change),
        ] {
            callbacks.push((name, callback_change(&options, name)?, slot));
        }
        if !self.state.closes && matches!(callbacks[1].1, Some(Some(_))) {
            return Err(JsError::new(
                "onClose cannot be added to a ConfigDocuments mounted without one: its tabs are \
                 built without close buttons. Create a new ConfigDocuments with onClose",
            ));
        }
        if !self.state.adds && matches!(callbacks[2].1, Some(Some(_))) {
            return Err(JsError::new(
                "onAdd cannot be added to a ConfigDocuments mounted without one: its tab strip is \
                 built without the \"+\" button. Create a new ConfigDocuments with onAdd",
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
            Some(list) if list.is_null() => Some(Listed::default()),
            Some(list) => Some(self.owner.with(|| self.state.read(&list, &view))?),
            None => None,
        };
        // A document `replaceValue` gave a text meanwhile keeps it.
        let listed = listed.map(|mut listed| {
            if !replaced.is_empty() {
                let edits = self.edits();
                for document in &mut listed.documents {
                    if replaced.contains(&document.id.as_str())
                        && let Some(value) = edits
                            .get(&document.id)
                            .and_then(|edited| edited.value.as_ref())
                    {
                        document.value.clone_from(value);
                    }
                }
            }
            listed
        });
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
        let mut resend = (Vec::new(), false);
        if let Some(mut listed) = listed {
            resend = self.state.keep_moved_past(&mut listed, flushed);
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
            let edits = self.edits();
            let _ = self.state.documents.try_update(|documents| {
                keep_edits(documents, &edits);
                for document in documents.iter_mut() {
                    if let Some((_, settings)) = own.iter().find(|(id, _)| *id == document.id) {
                        document.settings = Some(settings.clone());
                    }
                }
            });
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
            let _ = self.state.saves.try_set(callback.is_some());
            let _ = self.state.on_save.try_set_value(callback);
        }
        if let Some(id) = active {
            self.state.handle.show(&id);
        }
        self.tell_added(added);
        // A host left on an echo of a text the user typed past hears the
        // document again: now, or once a pause is over if it is still on it
        // then (the document's echo may have been lost in a render of the
        // host's that took a later text too).
        let (resend, settles) = resend;
        self.state.tell_again(resend, settles);
        Ok(())
    }

    /// What each open document holds that its list may not say, for
    /// [`keep_edits`].
    fn edits(&self) -> HashMap<String, Edited> {
        let ids: Vec<String> = self
            .state
            .documents
            .try_with_untracked(|documents| {
                documents
                    .iter()
                    .map(|document| document.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        ids.into_iter()
            .filter_map(|id| {
                let edit_as = config_editor_leptos::js_support::edit_as(&self.state.handle, &id)?;
                let value = self
                    .state
                    .handle
                    .editor(&id)
                    .and_then(|editor| editor.value());
                Some((id, Edited { value, edit_as }))
            })
            .collect()
    }

    /// A state callback added by an update hears each listed document's
    /// state at once (in the list's order).
    fn tell_added(&self, added: Vec<(&'static str, Function)>) {
        if added.is_empty() {
            return;
        }
        let ids: Vec<String> = self
            .state
            .documents
            .try_with_untracked(|documents| {
                documents
                    .iter()
                    .map(|document| document.id.clone())
                    .collect()
            })
            .unwrap_or_default();
        let told = self.state.told.try_get_value().unwrap_or_default();
        for (name, callback) in added {
            for id in &ids {
                if let Some(argument) = told.get(id).and_then(|told| told.argument(name)) {
                    call_host_with(&callback, &[JsValue::from_str(id), argument]);
                }
            }
        }
    }

    /// Unmounts the view and its editors. The instance cannot be used
    /// afterwards.
    pub fn destroy(&self) {
        config_editor_leptos::js_support::flush_pending_input();
        crate::settle::cancel(self.state.settle);
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

/// What an open document holds that its list may not say.
struct Edited {
    /// The text its editor holds (pending input applied), once shown.
    value: Option<String>,
    /// The language it is edited as, as its list or `editAs` asked last.
    edit_as: Option<config_editor_leptos::Language>,
}

/// Lists each document with the text its editor holds and the language it
/// is edited as (`edits`): a list the view gives its documents again (a
/// `replaceValue` of another one, the view's settings) is no discard of
/// their edits, nor of an `editAs` the handle made (QA 30, L30-4), as a
/// list the host gives is.
fn keep_edits(documents: &mut [Document], edits: &HashMap<String, Edited>) {
    for document in documents {
        if let Some(edited) = edits.get(&document.id) {
            if let Some(value) = &edited.value {
                document.value.clone_from(value);
            }
            document.edit_as = edited.edit_as;
        }
    }
}

thread_local! {
    /// The ids whose text reached the host's `onSave` while a
    /// [`delivering`] runs.
    static DELIVERED: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Runs `save`, and returns with it the ids `onSave` heard meanwhile: a
/// save the view counts as sent may not reach the host (the view destroyed,
/// or `onSave` removed, by a callback of an earlier save).
fn delivering<T>(save: impl FnOnce() -> T) -> (T, Vec<String>) {
    let outer = DELIVERED.with(|delivered| delivered.borrow_mut().replace(Vec::new()));
    let result = save();
    let ids = DELIVERED.with(|delivered| {
        let mine = delivered.borrow_mut().take().unwrap_or_default();
        if let Some(mut outer) = outer {
            outer.extend(mine.iter().cloned());
            *delivered.borrow_mut() = Some(outer);
        }
        mine
    });
    (result, ids)
}
