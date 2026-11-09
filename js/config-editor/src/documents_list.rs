//! Reading the documents JavaScript lists into the crate's `Document`s.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use config_editor_leptos::{Document, EditorSettings, Language, Schema, SecretOptions};
use js_sys::{Array, Object};
use wasm_bindgen::prelude::*;

use crate::host_diagnostics::host_diagnostics_from;
use crate::languages::{language_for_path, language_named, writable_edit_as};
use crate::options::{bool_option, warn_unknown_keys};
use crate::protect::{paths_from, ranges_from, secrets_from};
use crate::settings::{setting_keys, settings_over};
use crate::values::{field, optional_text, text_arg};

/// The keys of one document.
const DOCUMENT_KEYS: &[&str] = &[
    "id",
    "fileName",
    "value",
    "language",
    "baseline",
    "readOnly",
    "readOnlyReason",
    "closable",
    "schema",
    "editAs",
    "editAsText",
    "settings",
    "hostDiagnostics",
    "readOnlyPaths",
    "readOnlyRanges",
    "secrets",
];

/// What a list from JavaScript reads into: the documents, and each one's
/// own `settings` as given (laid over the view's again when those change).
pub(crate) struct Listed {
    pub(crate) documents: Vec<Document>,
    pub(crate) overrides: Vec<(String, JsValue)>,
}

/// What reading the list keeps from one reading to the next: each
/// document's secret options, built once per id as its editor reads them
/// when it mounts, and the schemas compiled, by their JSON text.
#[derive(Clone, Default)]
pub(crate) struct Kept {
    secrets: HashMap<String, SecretOptions>,
    schemas: HashMap<String, Schema>,
}

/// A console warning, once per file name, that no language claims
/// `file_name`'s extension, so the document opens in `fallback`.
fn warn_unclaimed(file_name: &str, fallback: Language) {
    thread_local! {
        static WARNED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    }
    if WARNED.with(|warned| warned.borrow_mut().insert(file_name.to_string())) {
        // A name with no extension (`Dockerfile`) is named whole.
        let what = match std::path::Path::new(file_name).extension() {
            Some(extension) => format!("the extension .{}", extension.to_string_lossy()),
            None => format!("the file name {file_name:?} (it has no extension)"),
        };
        web_sys::console::warn_1(&JsValue::from_str(&format!(
            "config-editor: no language claims {what}, so {file_name:?} opens as {}. Give the document a `language`, or register a language for it",
            fallback.label()
        )));
    }
}

/// The documents JavaScript lists, as the component takes them. Schemas
/// stay JSON text: the view compiles each distinct text once and shares it.
///
/// A document's `settings` go over `view`; its `secrets` are built once per
/// id, and its `schema` compiled once per text (a schema that does not
/// compile throws, as a `ConfigEditor`'s does), see [`Kept`].
pub(crate) fn documents_from(
    list: &JsValue,
    view: &EditorSettings,
    kept: &mut Kept,
) -> Result<Listed, JsError> {
    let mut schemas = HashMap::new();
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
        if id.is_empty() {
            return Err(JsError::new(&format!("{} must not be empty", name("id"))));
        }
        let file_name = text_arg(&field(&item, "fileName"), &name("fileName"))?;
        let value = optional_text(&field(&item, "value"), &name("value"))?.unwrap_or_default();
        let mut document = Document::new(id, file_name, value);
        document = match optional_text(&field(&item, "language"), &name("language"))? {
            Some(language) => document.language(language_named(&language)?),
            None => match language_for_path(&document.file_name) {
                Some(language) => document.language(language),
                None => {
                    warn_unclaimed(&document.file_name, document.resolved_language());
                    document
                }
            },
        };
        if let Some(baseline) = optional_text(&field(&item, "baseline"), &name("baseline"))? {
            document = document.baseline(baseline);
        }
        if let Some(read_only) = bool_option(&item, "readOnly")? {
            document = document.read_only(read_only);
        }
        document.read_only_reason =
            optional_text(&field(&item, "readOnlyReason"), &name("readOnlyReason"))?;
        if let Some(closable) = bool_option(&item, "closable")? {
            document = document.closable(closable);
        }
        if let Some(json) = optional_text(&field(&item, "schema"), &name("schema"))? {
            let schema = match schemas.get(&json).or_else(|| kept.schemas.get(&json)) {
                Some(schema) => schema.clone(),
                None => Schema::compile(&json)
                    .map_err(|error| JsError::new(&format!("{}: {error}", name("schema"))))?,
            };
            schemas.insert(json, schema.clone());
            document = document.schema(schema);
        }
        if let Some(edit_as) = optional_text(&field(&item, "editAs"), &name("editAs"))? {
            let edit_as =
                writable_edit_as(document.resolved_language(), language_named(&edit_as)?)?;
            if edit_as != document.resolved_language() {
                document = document.edit_as(edit_as);
            }
        }
        document.edit_as_text = optional_text(&field(&item, "editAsText"), &name("editAsText"))?
            .filter(|_| document.edit_as.is_some());
        document = with_protection(document, &item, index, &mut kept.secrets)?;
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
    kept.secrets
        .retain(|id, _| documents.iter().any(|document| document.id == *id));
    kept.schemas = schemas;
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
