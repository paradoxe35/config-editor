//! The `ConfigDocuments` class: the crate's documents view (a tab strip over
//! one editor per document) mounted into an element, with setters writing
//! the signals it reads.

use config_editor_leptos::{
    CloseRequest, ConfigDocuments as DocumentsView, Document, DocumentsHandle, EditorTheme,
    SaveRequest, ThemeColors,
};
use js_sys::{Array, Function};
use leptos::prelude::*;
use serde::Serialize;
use wasm_bindgen::prelude::*;

use crate::catalog::{language_name, theme_named};
use crate::editor::{MOUNTED, Mounted, NEXT_ID, colors_from, replace_editors_in};
use crate::languages::{language_named, writable_edit_as};
use crate::mount::{IN_CALLBACK, call_host, call_host_with};
use crate::options::{bool_option, function_option, string_option, warn_unknown_keys};
use crate::typings::{ColorMap, DocumentList, DocumentsOptions, DocumentsUpdate};
use crate::values::{field, optional_text, text_arg, to_js};

/// The options `new ConfigDocuments` takes.
pub(crate) const DOCUMENTS_OPTIONS: [&str; 15] = [
    "documents",
    "activeId",
    "theme",
    "colors",
    "styled",
    "showFooter",
    "showDiff",
    "saveShortcut",
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
    "id", "fileName", "value", "language", "baseline", "readOnly", "schema", "editAs",
];

/// The callbacks `setCallbacks` replaces.
const DOCUMENTS_CALLBACKS: &[&str] =
    &["onSelect", "onClose", "onChange", "onSave", "onDirtyChange"];

/// The documents JavaScript lists, as the component takes them. Schemas
/// stay JSON text: the view compiles each distinct text once and shares it.
fn documents_from(list: &JsValue) -> Result<Vec<Document>, JsError> {
    if !Array::is_array(list) {
        return Err(JsError::new(
            "documents must be an array of { id, fileName, value, … }",
        ));
    }
    let list: &Array = list.unchecked_ref();
    let mut documents = Vec::with_capacity(list.length() as usize);
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
        documents.push(document);
    }
    Ok(documents)
}

/// What `onClose` receives.
#[derive(Serialize)]
struct CloseOut {
    id: String,
    dirty: bool,
}

/// The live state a mounted view reads; setters write it.
#[derive(Clone, Copy)]
struct State {
    documents: RwSignal<Vec<Document>>,
    theme: RwSignal<EditorTheme>,
    colors: RwSignal<ThemeColors>,
    save_shortcut: RwSignal<bool>,
    /// Whether `onSave` is set: only then does `Ctrl+S` save.
    saves: RwSignal<bool>,
    on_select: StoredValue<Option<Function>, LocalStorage>,
    on_close: StoredValue<Option<Function>, LocalStorage>,
    on_change: StoredValue<Option<Function>, LocalStorage>,
    on_save: StoredValue<Option<Function>, LocalStorage>,
    on_dirty_change: StoredValue<Option<Function>, LocalStorage>,
    handle: DocumentsHandle,
}

/// What the component takes once, at mount.
struct Setup {
    active: Option<String>,
    styled: bool,
    show_footer: bool,
    show_diff: bool,
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
    let theme = Signal::derive(move || state.theme.get());
    let colors = Signal::derive(move || Some(state.colors.get()));
    let save_shortcut = Signal::derive(move || state.saves.get() && state.save_shortcut.get());
    view! {
        <DocumentsView
            documents=documents
            nostrip:initial_active=setup.active
            on_select=selected
            nostrip:on_close=close
            on_change=changed
            on_save=saved
            on_dirty_change=dirty_changed
            enable_save_shortcut=save_shortcut
            theme=theme
            colors=colors
            styled=setup.styled
            show_footer=setup.show_footer
            show_diff=setup.show_diff
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
/// import init, { ConfigDocuments } from "@pngwasi/config-editor";
/// await init();
/// let files = [
///   { id: "main", fileName: "burrow.yaml", value: mainText, schema },
///   { id: "tls", fileName: "conf.d/tls.toml", value: tlsText, schema },
/// ];
/// const tabs = new ConfigDocuments(document.getElementById("files"), {
///   documents: files,
///   onChange: (id, value) => {
///     files = files.map((file) => (file.id === id ? { ...file, value } : file));
///     tabs.setDocuments(files);
///   },
///   onSave: (id, value) => upload(id, value),
///   onClose: ({ id, dirty }) => { if (!dirty || confirm("Discard changes?")) close(id); },
/// });
/// ```
#[wasm_bindgen]
pub struct ConfigDocuments {
    state: State,
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
        let documents = match field(&options, "documents") {
            list if list.is_undefined() || list.is_null() => Vec::new(),
            list => documents_from(&list)?,
        };
        let theme = match string_option(&options, "theme")? {
            Some(name) => theme_named(&name)?,
            None => EditorTheme::default(),
        };
        let colors = colors_from(&field(&options, "colors"))?;
        let flag = |name: &str, default: bool| {
            Ok::<_, JsError>(bool_option(&options, name)?.unwrap_or(default))
        };
        let setup_flags = (
            flag("styled", true)?,
            flag("showFooter", false)?,
            flag("showDiff", true)?,
        );
        let save_shortcut = flag("saveShortcut", true)?;
        let on_select = function_option(&options, "onSelect")?;
        let on_close = function_option(&options, "onClose")?;
        let on_change = function_option(&options, "onChange")?;
        let on_save = function_option(&options, "onSave")?;
        let on_dirty_change = function_option(&options, "onDirtyChange")?;
        let setup = Setup {
            active: string_option(&options, "activeId")?,
            styled: setup_flags.0,
            show_footer: setup_flags.1,
            show_diff: setup_flags.2,
            class: string_option(&options, "className")?.unwrap_or_default(),
            label: string_option(&options, "label")?,
            closable: on_close.is_some(),
        };
        warn_unknown_keys(&options, &DOCUMENTS_OPTIONS, "option");
        replace_editors_in(&element);
        element.set_text_content(None);

        let owner = Owner::new();
        let state = owner.with(|| State {
            documents: RwSignal::new(documents),
            theme: RwSignal::new(theme),
            colors: RwSignal::new(colors),
            save_shortcut: RwSignal::new(save_shortcut),
            saves: RwSignal::new(on_save.is_some()),
            on_select: StoredValue::new_local(on_select),
            on_close: StoredValue::new_local(on_close),
            on_change: StoredValue::new_local(on_change),
            on_save: StoredValue::new_local(on_save),
            on_dirty_change: StoredValue::new_local(on_dirty_change),
            handle: DocumentsHandle::new(),
        });
        let mount_element = element.clone();
        let mounted = owner.with(|| leptos::mount::mount_to(element, move || view(state, setup)));
        let id = NEXT_ID.with(|next| {
            let id = next.get();
            next.set(id.wrapping_add(1));
            id
        });
        let entry: Mounted = (Box::new(mounted), owner, mount_element);
        MOUNTED.with(|views| {
            views.borrow_mut().insert(id, entry);
        });
        Ok(ConfigDocuments {
            state,
            id: Some(id),
        })
    }

    /// Replaces the list of documents. A document keeps its editor (caret,
    /// undo history, scroll, folds) while its `id` stays listed; its text,
    /// baseline, file name and read-only state update in place, and a new
    /// `language` or `schema` mounts a new editor.
    #[wasm_bindgen(js_name = setDocuments)]
    pub fn set_documents(&self, documents: DocumentList) -> Result<(), JsError> {
        config_editor_leptos::flush_pending_input();
        let documents = documents_from(&documents.into())?;
        let _ = self.state.documents.try_set(documents);
        Ok(())
    }

    /// Shows the document `id` and focuses its editor; `false` when no
    /// document has that id.
    pub fn select(
        &self,
        #[wasm_bindgen(unchecked_param_type = "string")] id: JsValue,
    ) -> Result<bool, JsError> {
        config_editor_leptos::flush_pending_input();
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
        config_editor_leptos::flush_pending_input();
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
        config_editor_leptos::flush_pending_input();
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
        config_editor_leptos::flush_pending_input();
        if self.state.on_save.try_with_value(Option::is_some) != Some(true) {
            return Vec::new();
        }
        untrack(|| self.state.handle.save_all())
    }

    /// Focuses the active document's editor, keeping its selection.
    pub fn focus(&self) {
        self.state.handle.focus();
    }

    /// Switches the theme; `null` returns to the default one.
    #[wasm_bindgen(js_name = setTheme)]
    pub fn set_theme(
        &self,
        #[wasm_bindgen(unchecked_param_type = "ThemeName | null | undefined")] theme: JsValue,
    ) -> Result<(), JsError> {
        let theme = match optional_text(&theme, "theme")? {
            Some(name) => theme_named(&name)?,
            None => EditorTheme::default(),
        };
        let _ = self.state.theme.try_set(theme);
        Ok(())
    }

    /// Replaces the color overrides (the `tab-*` tokens included); `{}`
    /// clears them.
    #[wasm_bindgen(js_name = setColors)]
    pub fn set_colors(&self, colors: ColorMap) -> Result<(), JsError> {
        let _ = self.state.colors.try_set(colors_from(&colors.into())?);
        Ok(())
    }

    /// Whether `Ctrl+S` / `Cmd+S` saves (with `onSave` set).
    #[wasm_bindgen(js_name = setSaveShortcut)]
    pub fn set_save_shortcut(
        &self,
        #[wasm_bindgen(unchecked_param_type = "boolean")] enabled: JsValue,
    ) -> Result<(), JsError> {
        let enabled = enabled
            .as_bool()
            .ok_or_else(|| JsError::new("saveShortcut must be a boolean"))?;
        let _ = self.state.save_shortcut.try_set(enabled);
        Ok(())
    }

    /// Replaces callbacks: a function replaces, `null` removes, absent
    /// keeps. Close buttons show only when `onClose` was given to the
    /// constructor.
    #[wasm_bindgen(js_name = setCallbacks)]
    pub fn set_callbacks(&self, callbacks: DocumentsUpdate) -> Result<(), JsError> {
        let callbacks: JsValue = callbacks.into();
        if !callbacks.is_object() || Array::is_array(&callbacks) {
            return Err(JsError::new(
                "setCallbacks takes an object: { onChange?, onSave?, … }",
            ));
        }
        warn_unknown_keys(&callbacks, DOCUMENTS_CALLBACKS, "callback");
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
        let updates = [
            (read("onSelect")?, self.state.on_select),
            (read("onClose")?, self.state.on_close),
            (read("onChange")?, self.state.on_change),
            (read("onDirtyChange")?, self.state.on_dirty_change),
        ];
        let on_save = read("onSave")?;
        for (update, slot) in updates {
            if let Some(callback) = update {
                let _ = slot.try_set_value(callback);
            }
        }
        if let Some(callback) = on_save {
            let _ = self.state.saves.try_set(callback.is_some());
            let _ = self.state.on_save.try_set_value(callback);
        }
        Ok(())
    }

    /// Unmounts the view and its editors. The instance cannot be used
    /// afterwards.
    pub fn destroy(&mut self) {
        config_editor_leptos::flush_pending_input();
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
            // Called from a callback: the view's handler is still running.
            let deferred = Closure::once_into_js(unmount);
            if let Ok(queue) = field(&js_sys::global(), "queueMicrotask").dyn_into::<Function>() {
                let _ = queue.call1(&JsValue::NULL, &deferred);
            }
        } else {
            unmount();
        }
    }
}
