//! Reading the documents JavaScript lists into the crate's `Document`s.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use config_editor_leptos::{Document, EditorSettings, Language, Schema, SecretOptions, TextRange};
use js_sys::{Array, Object};
use wasm_bindgen::prelude::*;

use crate::given::Given;
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
#[derive(Default)]
pub(crate) struct Listed {
    pub(crate) documents: Vec<Document>,
    pub(crate) overrides: Vec<(String, JsValue)>,
    /// The documents listed with the ranges given last and a text other
    /// than the one they hold: the ranges their editors were given, which
    /// stay if that text turns out not to replace theirs (an echo, see
    /// `given_ranges::stay`).
    pub(crate) ranges_again: HashMap<String, Vec<TextRange>>,
}

/// What reading the list keeps from one reading to the next: each
/// document's secret options, built once per id as its editor reads them
/// when it mounts; its texts, and its schema compiled (with its text),
/// each read again only when the host gives another string (see `given`).
#[derive(Default)]
pub(crate) struct Kept {
    secrets: HashMap<String, SecretOptions>,
    pub(crate) texts: Given<String>,
    schemas: Given<(String, Schema)>,
    /// Each document's `readOnlyRanges` as given (see `given_ranges`).
    pub(crate) ranges: crate::given_ranges::Listed,
}

impl Kept {
    /// The list could not be read: what the last one read stays.
    pub(crate) fn abandon(&mut self) {
        self.texts.abandon();
        self.schemas.abandon();
        self.ranges.abandon();
    }
}

/// `message` as a console warning, once: a host hands its list to `update`
/// after every keystroke.
fn warn_once(message: &str) {
    thread_local! {
        static WARNED: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    }
    if WARNED.with(|warned| warned.borrow_mut().insert(message.to_string())) {
        web_sys::console::warn_1(&JsValue::from_str(message));
    }
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
/// stay text: the view compiles each distinct text once and shares it.
///
/// A document's `settings` go over `view`; its `secrets` are built once per
/// id, and its `schema` compiled once per text (a schema that does not
/// compile throws, as a `ConfigEditor`'s does), see [`Kept`]. `replaces(id,
/// value)` says whether the text `value` will replace what the editor of
/// document `id` holds: not that text, an echo, nor the text the host
/// listed last (its ranges given again are read in it only then, see
/// `given_ranges`).
pub(crate) fn documents_from(
    list: &JsValue,
    view: &EditorSettings,
    kept: &mut Kept,
    replaces: &dyn Fn(&str, &str) -> bool,
) -> Result<Listed, JsError> {
    if !Array::is_array(list) {
        return Err(JsError::new(
            "documents must be an array of { id, fileName, value, … }",
        ));
    }
    let list: &Array = list.unchecked_ref();
    let mut documents = Vec::with_capacity(list.length() as usize);
    let mut overrides = Vec::new();
    let mut ranges_again = HashMap::new();
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
        let value = value_of(kept, &id, &item, &name("value"))?;
        let baseline = text_of(kept, &id, &item, "baseline", &name("baseline"))?;
        let schema = schema_of(kept, &id, &field(&item, "schema"), &name("schema"))?;
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
        if let Some(baseline) = baseline {
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
        if let Some(schema) = schema {
            document = document.schema(schema);
        }
        if let Some(edit_as) = optional_text(&field(&item, "editAs"), &name("editAs"))? {
            let edit_as =
                writable_edit_as(document.resolved_language(), language_named(&edit_as)?)?;
            if edit_as != document.resolved_language() {
                document = document.edit_as(edit_as);
            }
        }
        document.edit_as_text = optional_text(&field(&item, "editAsText"), &name("editAsText"))?;
        if document.edit_as_text.is_some() && document.edit_as.is_none() {
            warn_once(&format!(
                "config-editor: {} is ignored: it is a text in the `editAs` language, and the document {:?} is not edited as another language",
                name("editAsText"),
                document.id
            ));
            document.edit_as_text = None;
        }
        let again;
        (document, again) = with_protection(document, &item, index, kept, replaces)?;
        if let Some(ranges) = again {
            ranges_again.insert(document.id.clone(), ranges);
        }
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
    kept.texts.finish();
    kept.schemas.finish();
    kept.ranges.finish();
    Ok(Listed {
        documents,
        overrides,
        ranges_again,
    })
}

/// A document's host problems, read-only fields and secrets. Its ranges
/// as given last, listed again with a text that does not replace the one
/// it holds (that text, an echo), keep where its editor moved them (see
/// `given_ranges`): they are not read again, nor checked, in a text they
/// were not given for (QA 34, HO34-1). Listed with a text that replaces
/// it, they are read in it, and the ones its editor was given are
/// returned too.
fn with_protection(
    mut document: Document,
    item: &JsValue,
    index: usize,
    kept: &mut Kept,
    replaces: &dyn Fn(&str, &str) -> bool,
) -> Result<(Document, Option<Vec<TextRange>>), JsError> {
    let name = |key: &str| format!("documents[{index}].{key}");
    let diagnostics = field(item, "hostDiagnostics");
    if !diagnostics.is_undefined() && !diagnostics.is_null() {
        let list = host_diagnostics_from(&diagnostics, &document.value)
            .map_err(|error| prefixed(&name("hostDiagnostics"), error))?;
        document = document.host_diagnostics(list);
    }
    let paths = paths_from(&field(item, "readOnlyPaths"), &name("readOnlyPaths"))?;
    let given = field(item, "readOnlyRanges");
    let mut again = kept.ranges.again(&document.id, &given);
    let ranges = match again.take() {
        Some(ranges)
            if crate::given_ranges::stay(true, replaces(&document.id, &document.value)) =>
        {
            ranges
        }
        last => {
            again = last;
            ranges_from(&given, &document.value, &name("readOnlyRanges"))?
        }
    };
    kept.ranges.read(&document.id, &given, ranges.clone());
    document = document.read_only_paths(paths).read_only_ranges(ranges);
    let given = field(item, "secrets");
    if !given.is_undefined() && !given.is_null() {
        let options = match kept.secrets.get(&document.id) {
            Some(options) => options.clone(),
            None => {
                let options = secrets_from(&given, &name("secrets"))?;
                kept.secrets.insert(document.id.clone(), options.clone());
                options
            }
        };
        document = document.secrets(options);
    }
    Ok((document, again))
}

/// Document `id`'s `value` (`name` in errors): a string, as a
/// `ConfigEditor`'s (`null` throws); left out, the text the last list gave
/// it or the view handed the host (empty in the first list), as an update
/// without `value` leaves a `ConfigEditor`'s document.
fn value_of(kept: &mut Kept, id: &str, item: &JsValue, name: &str) -> Result<String, JsError> {
    let given = field(item, "value");
    if given.is_undefined() {
        return Ok(kept.texts.carry(id, "value").unwrap_or_default());
    }
    kept.texts
        .get(id, "value", &given, |_| text_arg(&given, name))
}

/// Document `id`'s text `key` of `item` (`name` in errors), if given.
fn text_of(
    kept: &mut Kept,
    id: &str,
    item: &JsValue,
    key: &'static str,
    name: &str,
) -> Result<Option<String>, JsError> {
    let given = field(item, key);
    if given.is_undefined() || given.is_null() {
        return Ok(None);
    }
    kept.texts
        .get(id, key, &given, |_| text_arg(&given, name))
        .map(Some)
}

/// Document `id`'s schema, given as `given` (its JSON or YAML text): compiled once
/// per text, shared by the documents giving the same one.
fn schema_of(
    kept: &mut Kept,
    id: &str,
    given: &JsValue,
    name: &str,
) -> Result<Option<Schema>, JsError> {
    if given.is_undefined() || given.is_null() {
        return Ok(None);
    }
    let (_, schema) = kept.schemas.get(id, "schema", given, |known| {
        let json = text_arg(given, name)?;
        let schema = match known.values().find(|(text, _)| *text == json) {
            Some((_, schema)) => schema.clone(),
            None => {
                Schema::compile(&json).map_err(|error| JsError::new(&format!("{name}: {error}")))?
            }
        };
        Ok((json, schema))
    })?;
    Ok(Some(schema))
}

/// `error` with `name` in front, so it says which document it is about.
fn prefixed(name: &str, error: JsError) -> JsError {
    let message = JsValue::from(error);
    let text = field(&message, "message").as_string().unwrap_or_default();
    JsError::new(&format!("{name}: {text}"))
}
