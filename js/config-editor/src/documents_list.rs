//! Reading the documents JavaScript lists into the crate's `Document`s.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use js_sys::{Array, Object};
use leptos_config_editor::js_support::{
    Handoff, HostChannel, HostUpdate, Listed as ListedItems, TextLines,
};
use leptos_config_editor::{
    Document, EditorSettings, Language, Schema, SecretOptions, TextPosition, TextRange,
};
use wasm_bindgen::prelude::*;

use crate::anchors::{anchor_of, revision_of, with_lists};
use crate::given::Given;
use crate::languages::{language_for_path, language_named, writable_edit_as};
use crate::options::{bool_option, warn_unknown_keys};
use crate::protect::{paths_from, secrets_from};
use crate::schema_formats::{SOURCE_KEYS, read_spec};
use crate::schema_spec::{SpecKey, registrations};
use crate::settings::{setting_keys, settings_over};
use crate::values::{field, json_from_js, optional_text, text_arg};

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
    "schemaFormat",
    "schemaResources",
    "schemaOptions",
    "editAs",
    "editAsText",
    "settings",
    "hostDiagnostics",
    "readOnlyPaths",
    "readOnlyRanges",
    "rangesFor",
    "diagnosticsFor",
    "valueRevision",
    "secrets",
];

/// What a list from JavaScript reads into: the documents, and each one's
/// own `settings` as given (laid over the view's again when those change).
#[derive(Default)]
pub(crate) struct Listed {
    pub(crate) documents: Vec<Document>,
    pub(crate) overrides: Vec<(String, JsValue)>,
    /// The update each document listed before gives its host's record:
    /// its value with the lists read in the texts they name (see the
    /// core's `host_inputs`), which the view hands its editor. A document
    /// listed for the first time has none here: its record was made with
    /// them.
    pub(crate) updates: HashMap<String, HostUpdate>,
    /// The records made for the documents listed for the first time.
    pub(crate) fresh: Vec<(String, HostChannel)>,
    /// The documents whose `readOnlyRanges` could not be read in their
    /// text: one document's bad ranges hold no other document back (see
    /// `with_protection`), so they are reported once the list is taken.
    pub(crate) refused: Vec<JsError>,
}

/// What reading the list keeps from one reading to the next: each
/// document's secret options, built once per id as its editor reads them
/// when it mounts; its texts, and its schema compiled, each read again only
/// when the host gives another string (see `given`).
#[derive(Default)]
pub(crate) struct Kept {
    secrets: HashMap<String, SecretOptions>,
    pub(crate) texts: Given<String>,
    schemas: KeptSchemas,
    /// Whether the host lists every text `onChange` gives back (`echoes`).
    pub(crate) echoing: bool,
    /// Each document's record of its host (the core's), shared with its
    /// editor once it mounts.
    pub(crate) channels: HashMap<String, HostChannel>,
    /// The texts still on their way of the documents no longer listed, by
    /// id: a document listed again with its id (shown again) takes them
    /// over, so their late echoes are told apart there (see the core's
    /// `HostChannel::handoff`). They go with the view.
    handed: HashMap<String, Handoff>,
}

impl Kept {
    /// The list could not be read: what the last one read stays.
    pub(crate) fn abandon(&mut self) {
        self.texts.abandon();
        self.schemas.read.clear();
    }
}

/// Each document's schema as the last list gave it, compiled, and those of
/// the list being read (which replace them once it is read whole).
#[derive(Default)]
struct KeptSchemas {
    last: HashMap<String, KeptSchema>,
    read: HashMap<String, KeptSchema>,
}

/// A document's schema: what it was given (the `schema` and
/// `schemaResources` values as passed, the format's name and the options as
/// JSON text) and what that compiled to.
#[derive(Clone)]
struct KeptSchema {
    text: JsValue,
    resources: JsValue,
    format: Option<String>,
    options: String,
    key: SpecKey,
    schema: Schema,
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
/// compile throws, as a `ConfigEditor`'s does), see [`Kept`]. `current(id)`
/// is the text the view holds for document `id`, if it was listed before:
/// its ranges are read as its channel decides (see `HostChannel::ranges`).
pub(crate) fn documents_from<'a>(
    list: &JsValue,
    view: &EditorSettings,
    kept: &mut Kept,
    current: &dyn Fn(&str) -> Option<&'a str>,
) -> Result<Listed, JsError> {
    if !Array::is_array(list) {
        return Err(JsError::new(
            "documents must be an array of { id, fileName, value, … }",
        ));
    }
    let list: &Array = list.unchecked_ref();
    let mut documents = Vec::with_capacity(list.length() as usize);
    let mut overrides = Vec::new();
    let mut updates = HashMap::new();
    let mut fresh = Vec::new();
    let mut refused = Vec::new();
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
        let schema = schema_of(kept, &id, &item, &name(""))?;
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
        let protected = with_protection(document, &item, index, kept, current)?;
        document = protected.document;
        if let Some(update) = protected.update {
            updates.insert(document.id.clone(), update);
        }
        if let Some(channel) = protected.fresh {
            fresh.push((document.id.clone(), channel));
        }
        refused.extend(protected.refused);
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
    let unlisted: Vec<String> = kept
        .channels
        .keys()
        .filter(|id| !documents.iter().any(|document| document.id == **id))
        .cloned()
        .collect();
    for id in unlisted {
        if let Some(handoff) = kept.channels.remove(&id).and_then(|gone| gone.handoff()) {
            kept.handed.insert(id, handoff);
        }
    }
    kept.texts.finish();
    kept.schemas.last = std::mem::take(&mut kept.schemas.read);
    Ok(Listed {
        documents,
        overrides,
        updates,
        fresh,
        refused,
    })
}

/// A document with its host problems, read-only fields and secrets.
struct Protected {
    document: Document,
    /// The update for its host's record, when it was listed before (`None`
    /// inside its lists when they could not be read).
    update: Option<HostUpdate>,
    /// Its record, when it is listed for the first time.
    fresh: Option<HostChannel>,
    /// Why its lists could not be read.
    refused: Option<JsError>,
}

/// A document's host problems, read-only fields and secrets. Its lists are
/// read in the texts they name (`rangesFor`, `diagnosticsFor`, else its
/// value, see the core's `host_inputs`), against its host's record: a
/// document listed before gets the update for it, one listed for the
/// first time a new record that holds them.
///
/// Lists that cannot be read (a revision never sent, ranges past the text
/// they name) are returned as the error, and hold back no other document:
/// one listed before is taken as listed without them (its locks stay);
/// one listed for the first time is locked whole, as nothing tells which
/// of it the host meant to lock.
fn with_protection<'a>(
    mut document: Document,
    item: &JsValue,
    index: usize,
    kept: &mut Kept,
    current: &dyn Fn(&str) -> Option<&'a str>,
) -> Result<Protected, JsError> {
    let name = |key: &str| format!("documents[{index}].{key}");
    let prefix = name("");
    let paths = paths_from(&field(item, "readOnlyPaths"), &name("readOnlyPaths"))?;
    document = document.read_only_paths(paths);
    let value_revision = revision_of(item);
    let ranges_for = anchor_of(item, "rangesFor")
        .map_err(|error| prefixed(&format!("documents[{index}]"), error))?;
    let diagnostics_for = anchor_of(item, "diagnosticsFor")
        .map_err(|error| prefixed(&format!("documents[{index}]"), error))?;
    let before = current(&document.id).zip(kept.channels.get(&document.id).cloned());
    let (channel, host_text, fresh) = match before {
        Some((current, channel)) => (channel, current.to_string(), false),
        None => (
            HostChannel::new(
                kept.echoing,
                &document.value,
                value_revision,
                kept.handed.remove(&document.id),
            ),
            document.value.clone(),
            true,
        ),
    };
    let head = HostUpdate {
        value: Some(document.value.clone()),
        value_revision,
        ranges_for,
        diagnostics_for,
        ..HostUpdate::default()
    };
    let mut refused = None;
    let update = with_lists(&channel, item, head, &host_text, &prefix)
        .map_err(|error| refused = Some(error))
        .ok();
    if !fresh {
        let update = update.or_else(|| {
            Some(HostUpdate {
                value: Some(document.value.clone()),
                value_revision,
                ..HostUpdate::default()
            })
        });
        return Ok(Protected {
            document: with_secrets(document, item, index, kept)?,
            update,
            fresh: None,
            refused,
        });
    }
    let update = update.unwrap_or_else(|| HostUpdate {
        ranges: Some(ListedItems::new(vec![whole(&document.value)])),
        ..HostUpdate::default()
    });
    if update.ranges.is_some() || update.diagnostics.is_some() {
        let update = HostUpdate {
            value: None,
            value_revision: None,
            ..update
        };
        if let Err(error) = channel.take_unmounted(update, &document.value) {
            refused.get_or_insert_with(|| {
                prefixed(
                    &format!("documents[{index}]"),
                    JsError::new(&error.to_string()),
                )
            });
        }
    }
    kept.channels.insert(document.id.clone(), channel.clone());
    Ok(Protected {
        document: with_secrets(document, item, index, kept)?,
        update: None,
        fresh: Some(channel),
        refused,
    })
}

/// `document` with its `secrets` (built once per id).
fn with_secrets(
    mut document: Document,
    item: &JsValue,
    index: usize,
    kept: &mut Kept,
) -> Result<Document, JsError> {
    let name = |key: &str| format!("documents[{index}].{key}");
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
    Ok(document)
}

/// A range over all of `text`.
fn whole(text: &str) -> TextRange {
    let lines = TextLines::new(text);
    let last = lines.count();
    TextRange::new(
        TextPosition::new(1, 1),
        TextPosition::new(last, lines.width(last) + 1),
    )
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

/// Document `id`'s schema (`item`'s `schema`, in its `schemaFormat`, with
/// its `schemaResources` and `schemaOptions`): compiled once per schema,
/// shared by the documents giving the same one, and read again only when
/// the host gives another text or resources object, format or options.
/// Errors name the key after `prefix` (`documents[0].`).
fn schema_of(
    kept: &mut Kept,
    id: &str,
    item: &JsValue,
    prefix: &str,
) -> Result<Option<Schema>, JsError> {
    let given = field(item, "schema");
    if given.is_undefined() || given.is_null() {
        return Ok(None);
    }
    let name = |key: &str| format!("{prefix}{key}");
    let format = optional_text(&field(item, "schemaFormat"), &name("schemaFormat"))?;
    let resources = field(item, "schemaResources");
    let options_value = field(item, "schemaOptions");
    let options = if options_value.is_undefined() || options_value.is_null() {
        "{}".to_string()
    } else {
        json_from_js(&options_value, 0)
            .map_err(|error| JsError::new(&format!("{}: {error}", name("schemaOptions"))))?
            .to_string()
    };
    let same = |known: &KeptSchema| {
        Object::is(&known.text, &given)
            && Object::is(&known.resources, &resources)
            && known.format == format
            && known.options == options
            && known.key.4 == registrations()
    };
    if let Some(known) = kept.schemas.last.get(id).filter(|known| same(known)) {
        let known = known.clone();
        let schema = known.schema.clone();
        kept.schemas.read.insert(id.to_string(), known);
        return Ok(Some(schema));
    }
    let text = text_arg(&given, &name("schema"))?;
    let spec = read_spec(item, &SOURCE_KEYS, prefix)?;
    let key = spec.key(&text);
    let shared = kept
        .schemas
        .read
        .values()
        .chain(kept.schemas.last.values())
        .find(|known| known.key == key)
        .map(|known| known.schema.clone());
    let schema = match shared {
        Some(schema) => schema,
        None => spec
            .compile(&text)
            .map_err(|error| JsError::new(&format!("{}: {error}", name("schema"))))?,
    };
    kept.schemas.read.insert(
        id.to_string(),
        KeptSchema {
            text: given,
            resources,
            format,
            options,
            key,
            schema: schema.clone(),
        },
    );
    Ok(Some(schema))
}

/// `error` with `name` in front, so it says which document it is about.
fn prefixed(name: &str, error: JsError) -> JsError {
    let message = JsValue::from(error);
    let text = field(&message, "message").as_string().unwrap_or_default();
    JsError::new(&format!("{name}: {text}"))
}
