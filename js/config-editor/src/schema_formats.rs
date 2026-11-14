//! Schema formats: their names (`schemaFormat`, `schemaFormats`,
//! `schemaFormatForFile`), formats written in JavaScript
//! (`registerSchemaFormat`), and reading a schema's format, resources and
//! options from what JavaScript passes.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

use config_editor_leptos::{
    CompiledSchema, LossEffect, SchemaCompiler, SchemaError, SchemaFormat, SchemaFormatInfo,
    SchemaInput, SchemaLoss, SchemaResources,
};
use js_sys::{Array, Function, Object};
use serde::Serialize;
use serde_json::{Map, Value};
use wasm_bindgen::prelude::*;

use crate::languages::interned;
use crate::options::warn_unknown_keys;
use crate::schema_spec::{SchemaSpec, note_registration};
use crate::typings::NameResult;
use crate::values::{
    field, json_from_js, json_to_js, optional_text, string_field, text_arg, to_js,
};

#[wasm_bindgen(typescript_custom_section)]
const TYPES: &str = r#"
/** A built-in schema format (`"json-schema"`, `"json-schema-yaml"`, `"xsd"`) or one from `registerSchemaFormat`. */
export type SchemaFormatName = string;

/** A schema format, as `schemaFormats()` lists it. */
export interface SchemaFormatInfo {
  /** What `schemaFormat` takes. */
  name: SchemaFormatName;
  /** What messages call it, such as `"XSD"`. */
  label: string;
  /** File extensions, without the dot; `schemaFormatForFile` matches the longest. */
  extensions: string[];
}

/** What a schema's format could not express in JSON Schema: a construct not checked, or checked more loosely or approximately. Each is also a schema warning (`onSchemaWarning`, `schemaWarnings`). */
export interface SchemaLoss {
  /** What is lost, such as `xs:key "userId" is not checked`. */
  message: string;
  /** The construct, such as `"xs:key"`. */
  construct: string;
  /** How much of it is lost. */
  effect: "not-checked" | "loosened" | "approximated";
  /** The file it is in, when not the schema's own text. */
  location?: string;
  /** One-based line. */
  line?: number;
  /** One-based character column. */
  column?: number;
}

/** A schema format written in JavaScript, for `registerSchemaFormat`. */
export interface CustomSchemaFormat {
  /** What `schemaFormat` names it by (lowercased). Not a built-in format's name. */
  name: string;
  /** What messages call it. Default: `name`. */
  label?: string;
  /** File extensions, without the dot, for `schemaFormatForFile`. */
  extensions?: string[];
  /** The JSON Schema `text` says, as an object or JSON text, with what it loses and the checks of what JSON Schema cannot hold (`checks`, run each time the editor validates). Called synchronously when an editor mounts, and by the tools. Throw to refuse the text: the message is shown as the schema's error. `resources` are the files it may refer to, by location; `options` the host's `schemaOptions`. */
  compile(text: string, resources: Record<string, string>, options: Record<string, unknown>): { schema: object | string; losses?: SchemaLossInput[]; checks?: SchemaFormatCheck[] };
  /** The locations `text` refers to (files it includes), for the host to fetch into `schemaResources`. */
  imports?(text: string): string[];
}

/** A loss as a `CustomSchemaFormat` reports it: `construct` defaults to `""`, `effect` to `"not-checked"`. */
export interface SchemaLossInput {
  message: string;
  construct?: string;
  effect?: "not-checked" | "loosened" | "approximated";
  location?: string;
  line?: number;
  column?: number;
}

/** A schema's format, the files it refers to and the format's options, as `validate`, `convert` and `schemaWarnings` take them (the same keys as the editor's options). */
export interface SchemaSourceOptions {
  /** The schema's format. Default `"json-schema"`; never guessed from the text. */
  schemaFormat?: SchemaFormatName;
  /** The files the schema refers to (an include, an import, an external `$ref`), by location: the editor fetches nothing. */
  schemaResources?: Record<string, string>;
  /** Options of the format, such as an XSD's `{ root: "project" }`. */
  schemaOptions?: Record<string, unknown>;
}

/** What `compileSchema` takes besides the text. */
export interface CompileSchemaOptions {
  /** The schema's format. Default `"json-schema"`. */
  format?: SchemaFormatName;
  /** The files the schema refers to, by location. */
  resources?: Record<string, string>;
  /** Options of the format. */
  options?: Record<string, unknown>;
}

/** What `compileSchema` gives: the JSON Schema (as JSON text, to cache or ship) and what the format lost. */
export interface CompiledSchema {
  schema: string;
  losses: SchemaLoss[];
}
"#;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(typescript_type = "CustomSchemaFormat")]
    pub type CustomSchemaFormat;
    #[wasm_bindgen(typescript_type = "SchemaFormatInfo[]")]
    pub type SchemaFormatInfos;
    #[wasm_bindgen(typescript_type = "CompiledSchema")]
    pub type CompiledSchemaResult;
}

thread_local! {
    /// Formats registered from JavaScript, by name.
    static REGISTERED: RefCell<HashMap<String, SchemaFormat>> = RefCell::new(HashMap::new());
    /// The same formats as they were registered, to take a new definition.
    static OWN: RefCell<HashMap<String, &'static JsSchemaFormat>> = RefCell::new(HashMap::new());
}

/// The format `name` names: a registered one, else a built-in one.
pub(crate) fn format_named(name: &str) -> Result<SchemaFormat, JsError> {
    let key = name.trim().to_lowercase();
    if let Some(format) = REGISTERED.with(|formats| formats.borrow().get(&key).copied()) {
        return Ok(format);
    }
    name.parse::<SchemaFormat>().map_err(|error| {
        let mut message = error.to_string();
        let registered: Vec<String> =
            REGISTERED.with(|formats| formats.borrow().keys().cloned().collect());
        if !registered.is_empty() {
            message = format!("{message}; registered: {}", registered.join(", "));
        }
        JsError::new(&message)
    })
}

/// The keys a [`SchemaSpec`] is read from.
pub(crate) struct SpecKeys {
    pub(crate) format: &'static str,
    pub(crate) resources: &'static str,
    pub(crate) options: &'static str,
}

/// The editor's, a document's and the headless tools' keys.
pub(crate) const SOURCE_KEYS: SpecKeys = SpecKeys {
    format: "schemaFormat",
    resources: "schemaResources",
    options: "schemaOptions",
};

/// `compileSchema`'s keys.
const COMPILE_KEYS: SpecKeys = SpecKeys {
    format: "format",
    resources: "resources",
    options: "options",
};

/// Read from `object`'s `keys` (absent ones take the defaults); errors
/// name the key after `prefix` (`"documents[0]."`).
pub(crate) fn read_spec(
    object: &JsValue,
    keys: &SpecKeys,
    prefix: &str,
) -> Result<SchemaSpec, JsError> {
    if object.is_undefined() || object.is_null() {
        return Ok(SchemaSpec::json_schema());
    }
    let name = |key: &str| format!("{prefix}{key}");
    let format = match optional_text(&field(object, keys.format), &name(keys.format))? {
        Some(format) => format_named(&format)?,
        None => SchemaFormat::JSON_SCHEMA,
    };
    Ok(SchemaSpec {
        format,
        resources: resources_from(&field(object, keys.resources), &name(keys.resources))?,
        options: options_from(&field(object, keys.options), &name(keys.options))?,
    })
}

/// The resources object (`{ location: text }`) named `name`.
pub(crate) fn resources_from(value: &JsValue, name: &str) -> Result<SchemaResources, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(SchemaResources::new());
    }
    if !value.is_object() || Array::is_array(value) {
        return Err(JsError::new(&format!(
            "{name} must be an object of texts by location: {{ \"common.xsd\": text }}"
        )));
    }
    Object::entries(value.unchecked_ref())
        .iter()
        .map(|entry| {
            let entry: Array = entry.unchecked_into();
            let location = entry.get(0).as_string().unwrap_or_default();
            let text = entry
                .get(1)
                .as_string()
                .ok_or_else(|| JsError::new(&format!("{name}[\"{location}\"] must be a string")))?;
            Ok((location, text))
        })
        .collect()
}

/// The options object named `name`, as JSON.
fn options_from(value: &JsValue, name: &str) -> Result<Map<String, Value>, JsError> {
    if value.is_undefined() || value.is_null() {
        return Ok(Map::new());
    }
    match json_from_js(value, 0) {
        Ok(Value::Object(options)) => Ok(options),
        Ok(_) => Err(JsError::new(&format!("{name} must be an object"))),
        Err(error) => Err(JsError::new(&format!("{name}: {error}"))),
    }
}

/// Keys a `registerSchemaFormat` definition can hold.
const FORMAT_DEFINITION_KEYS: &[&str] = &["name", "label", "extensions", "compile", "imports"];

/// A schema format implemented in JavaScript. Each name has one, for good
/// (formats live as long as the page): registering the name again puts the
/// new definition in it.
struct JsSchemaFormat {
    definition: RefCell<JsValue>,
    info: Cell<SchemaFormatInfo>,
}

// SAFETY: wasm32 without the `atomics` target feature runs on one thread, so
// the JS object is never touched from another thread. The trait requires
// Send + Sync for hosts with threads.
#[cfg(not(target_feature = "atomics"))]
unsafe impl Send for JsSchemaFormat {}
#[cfg(not(target_feature = "atomics"))]
unsafe impl Sync for JsSchemaFormat {}

impl JsSchemaFormat {
    /// The definition's `name` method, if it has one.
    fn method(&self, name: &str) -> Option<(JsValue, Function)> {
        let definition = self.definition.borrow().clone();
        let method = field(&definition, name).dyn_into::<Function>().ok()?;
        Some((definition, method))
    }
}

/// What a thrown value says.
fn thrown_message(error: &JsValue) -> String {
    string_field(error, "message")
        .or_else(|| error.as_string())
        .unwrap_or_else(|| "it threw".to_string())
}

impl SchemaCompiler for JsSchemaFormat {
    fn info(&self) -> SchemaFormatInfo {
        self.info.get()
    }

    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        let label = self.info.get().label;
        let (definition, compile) = self
            .method("compile")
            .ok_or_else(|| SchemaError(format!("{label} has no compile()")))?;
        let resources = Object::new();
        for (location, text) in input.resources.iter() {
            let _ = js_sys::Reflect::set(
                &resources,
                &JsValue::from_str(location),
                &JsValue::from_str(text),
            );
        }
        let options = json_to_js(&Value::Object(input.options.clone()));
        let result = compile
            .call3(
                &definition,
                &JsValue::from_str(input.text),
                &resources,
                &options,
            )
            .map_err(|error| SchemaError(thrown_message(&error)))?;
        let shape = || {
            SchemaError(format!(
                "{label}'s compile() must return {{ schema, losses?, checks? }}"
            ))
        };
        if !result.is_object() {
            return Err(shape());
        }
        let schema = field(&result, "schema");
        let schema = match schema.as_string() {
            Some(json) => serde_json::from_str(&json).map_err(|error| {
                SchemaError(format!("{label}'s compile() gave no JSON: {error}"))
            })?,
            None if schema.is_object() => json_from_js(&schema, 0)
                .map_err(|error| SchemaError(format!("{label}'s compile() schema: {error}")))?,
            None => return Err(shape()),
        };
        let losses = losses_from(&field(&result, "losses"))
            .map_err(|error| SchemaError(format!("{label}'s compile() losses: {error}")))?;
        let checks = crate::schema_checks::checks_from(&field(&result, "checks"), label)
            .map_err(|error| SchemaError(format!("{label}'s compile() checks: {error}")))?;
        Ok(CompiledSchema::new(schema)
            .with_losses(losses)
            .with_checks(checks))
    }

    fn imports(&self, text: &str) -> Vec<String> {
        let Some((definition, imports)) = self.method("imports") else {
            return Vec::new();
        };
        let Ok(result) = imports.call1(&definition, &JsValue::from_str(text)) else {
            return Vec::new();
        };
        if !Array::is_array(&result) {
            return Vec::new();
        }
        Array::from(&result)
            .iter()
            .filter_map(|location| location.as_string())
            .collect()
    }
}

/// The losses a JavaScript compiler returned.
fn losses_from(value: &JsValue) -> Result<Vec<SchemaLoss>, String> {
    if value.is_undefined() || value.is_null() {
        return Ok(Vec::new());
    }
    if !Array::is_array(value) {
        return Err("must be an array".to_string());
    }
    Array::from(value)
        .iter()
        .enumerate()
        .map(|(index, loss)| {
            let message = string_field(&loss, "message")
                .ok_or_else(|| format!("[{index}] needs a message"))?;
            let effect = match string_field(&loss, "effect") {
                Some(effect) => effect.parse::<LossEffect>()?,
                None => LossEffect::NotChecked,
            };
            let construct = string_field(&loss, "construct").unwrap_or_default();
            let mut out = SchemaLoss::new(construct, effect, message);
            let number = |key: &str| {
                field(&loss, key)
                    .as_f64()
                    .filter(|number| *number >= 1.0 && number.fract() == 0.0)
                    .map(|number| number as usize)
            };
            out.line = number("line");
            out.column = number("column");
            out.location = string_field(&loss, "location");
            Ok(out)
        })
        .collect()
}

/// Registers a schema format implemented in JavaScript and returns its
/// name for `schemaFormat` (lowercased). Registering the same name again
/// replaces its definition, and takes no more memory.
#[cfg(not(target_feature = "atomics"))]
#[wasm_bindgen(js_name = registerSchemaFormat)]
pub fn register_schema_format(definition: CustomSchemaFormat) -> Result<String, JsError> {
    let definition: JsValue = definition.into();
    if !definition.is_object() {
        return Err(JsError::new(
            "a schema format is an object: { name, compile, … }",
        ));
    }
    let name = string_field(&definition, "name")
        .map(|name| name.trim().to_lowercase())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| JsError::new("a schema format needs a name"))?;
    if name.parse::<SchemaFormat>().is_ok() {
        return Err(JsError::new(&format!(
            "\"{name}\" is a built-in schema format"
        )));
    }
    warn_unknown_keys(&definition, FORMAT_DEFINITION_KEYS, "schema format field");
    if !field(&definition, "compile").is_function() {
        return Err(JsError::new(
            "a schema format needs compile(text, resources, options)",
        ));
    }
    let imports = field(&definition, "imports");
    if !imports.is_undefined() && !imports.is_null() && !imports.is_function() {
        return Err(JsError::new("imports must be a function"));
    }
    let label = match optional_text(&field(&definition, "label"), "label")? {
        Some(label) if !label.trim().is_empty() => label.trim().to_string(),
        Some(_) => return Err(JsError::new("label must not be empty")),
        None => name.clone(),
    };
    let extensions_value = field(&definition, "extensions");
    let extensions: Vec<String> = if extensions_value.is_undefined() || extensions_value.is_null() {
        Vec::new()
    } else if Array::is_array(&extensions_value) {
        Array::from(&extensions_value)
            .iter()
            .map(|extension| {
                extension
                    .as_string()
                    .map(|text| text.trim().trim_start_matches('.').to_lowercase())
                    .filter(|text| !text.is_empty())
                    .ok_or_else(|| JsError::new("extensions must hold non-empty strings"))
            })
            .collect::<Result<_, _>>()?
    } else {
        return Err(JsError::new(
            "extensions must be an array of strings, e.g. [\"jtd.json\"]",
        ));
    };
    let info = SchemaFormatInfo {
        name: interned::text(&name),
        label: interned::text(&label),
        extensions: interned::texts(&extensions),
    };
    note_registration();
    match OWN.with(|own| own.borrow().get(&name).copied()) {
        Some(format) => {
            *format.definition.borrow_mut() = definition;
            format.info.set(info);
        }
        None => {
            let format: &'static JsSchemaFormat = Box::leak(Box::new(JsSchemaFormat {
                definition: RefCell::new(definition),
                info: Cell::new(info),
            }));
            OWN.with(|own| own.borrow_mut().insert(name.clone(), format));
            REGISTERED.with(|formats| {
                formats
                    .borrow_mut()
                    .insert(name.clone(), SchemaFormat::new(format));
            });
        }
    }
    Ok(name)
}

#[derive(Serialize)]
struct FormatInfoOut {
    name: &'static str,
    label: &'static str,
    extensions: &'static [&'static str],
}

impl From<SchemaFormat> for FormatInfoOut {
    fn from(format: SchemaFormat) -> Self {
        let info = format.info();
        FormatInfoOut {
            name: info.name,
            label: info.label,
            extensions: info.extensions,
        }
    }
}

/// Every format: the built-in ones this build has, then the registered ones
/// (by name).
fn every_format() -> Vec<SchemaFormat> {
    let mut formats: Vec<SchemaFormat> = SchemaFormat::ALL.to_vec();
    let mut registered: Vec<SchemaFormat> =
        REGISTERED.with(|known| known.borrow().values().copied().collect());
    registered.sort_by_key(|format| format.name());
    formats.extend(registered);
    formats
}

/// The built-in and registered schema formats, each with its name (what
/// `schemaFormat` takes), label and file extensions.
#[wasm_bindgen(js_name = schemaFormats)]
pub fn schema_formats() -> SchemaFormatInfos {
    let formats: Vec<FormatInfoOut> = every_format().into_iter().map(Into::into).collect();
    to_js(&formats).unchecked_into()
}

/// The schema format of a file, from the longest extension of any format
/// that its name ends with (`"app.schema.yaml"` → `"json-schema-yaml"`), a
/// registered format winning a tie; `null` when none claims it. An explicit
/// call: the editor never guesses a schema's format.
#[wasm_bindgen(js_name = schemaFormatForFile)]
pub fn schema_format_for_file(
    #[wasm_bindgen(unchecked_param_type = "string")] path: JsValue,
) -> Result<NameResult, JsError> {
    let path = text_arg(&path, "path")?;
    let file = path
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&path)
        .to_ascii_lowercase();
    let mut best: Option<(usize, bool, SchemaFormat)> = None;
    for format in every_format() {
        let registered = !SchemaFormat::ALL.contains(&format);
        for extension in format.info().extensions {
            let matches = file
                .strip_suffix(extension)
                .and_then(|stem| stem.strip_suffix('.'))
                .is_some_and(|stem| !stem.is_empty());
            let better = best.is_none_or(|(length, was_registered, _)| {
                (extension.len(), registered) > (length, was_registered)
            });
            if matches && better {
                best = Some((extension.len(), registered, format));
            }
        }
    }
    Ok(best
        .map_or(JsValue::NULL, |(_, _, format)| {
            JsValue::from_str(format.name())
        })
        .unchecked_into())
}

#[derive(Serialize)]
struct LossOut {
    message: String,
    construct: String,
    effect: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<usize>,
}

impl From<&SchemaLoss> for LossOut {
    fn from(loss: &SchemaLoss) -> Self {
        LossOut {
            message: loss.message.clone(),
            construct: loss.construct.to_string(),
            effect: loss.effect.as_str(),
            location: loss.location.clone(),
            line: loss.line,
            column: loss.column,
        }
    }
}

/// Compiles a schema written in `format` (default `"json-schema"`) into the
/// JSON Schema the editor uses, with what the format lost: compile at build
/// time and ship plain JSON Schema. An invalid schema throws.
#[wasm_bindgen(js_name = compileSchema)]
pub fn compile_schema(
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "CompileSchemaOptions")] options: JsValue,
) -> Result<CompiledSchemaResult, JsError> {
    let text = text_arg(&text, "text")?;
    if !options.is_undefined() && !options.is_null() {
        warn_unknown_keys(&options, &["format", "resources", "options"], "option");
    }
    let spec = read_spec(&options, &COMPILE_KEYS, "")?;
    let schema = spec
        .compile(&text)
        .map_err(|error| JsError::new(&error.to_string()))?;
    #[derive(Serialize)]
    struct Out {
        schema: String,
        losses: Vec<LossOut>,
    }
    Ok(to_js(&Out {
        schema: schema.json().to_string(),
        losses: schema.losses().iter().map(LossOut::from).collect(),
    })
    .unchecked_into())
}

/// The locations a schema in `format` (default `"json-schema"`) refers to,
/// for the host to fetch and pass as `schemaResources`; call it again on
/// each fetched text for theirs.
#[wasm_bindgen(js_name = schemaImports)]
pub fn schema_imports(
    #[wasm_bindgen(unchecked_param_type = "string")] text: JsValue,
    #[wasm_bindgen(unchecked_optional_param_type = "SchemaFormatName")] format: JsValue,
) -> Result<crate::typings::Names, JsError> {
    let text = text_arg(&text, "text")?;
    let format = match optional_text(&format, "format")? {
        Some(format) => format_named(&format)?,
        None => SchemaFormat::JSON_SCHEMA,
    };
    let mut seen = HashSet::new();
    let imports: Vec<String> = format
        .imports(&text)
        .into_iter()
        .filter(|location| seen.insert(location.clone()))
        .collect();
    Ok(to_js(&imports).unchecked_into())
}
