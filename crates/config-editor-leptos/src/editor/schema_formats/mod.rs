//! Schema formats: what turns a schema written in some format (JSON Schema
//! as JSON or YAML, a host's own) into the JSON Schema the editor uses.
//!
//! A format is a [`SchemaCompiler`] wrapped in a [`SchemaFormat`], as a
//! language is a `ConfigLanguage` wrapped in a `Language`. The compiler
//! reads the text (and the other files it refers to, which the host
//! supplies: nothing is ever fetched), and gives the JSON Schema with what
//! it could not express ([`SchemaLoss`]). The rest of the editor sees only
//! JSON Schema; this module and the format modules are the only places
//! that know a format.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock};

use serde_json::{Map, Value};

use super::diagnostics::{SchemaError, Severity};
use super::language::ConfigLanguage;

mod bundle;
pub(super) mod json;
pub(super) mod openapi;
#[cfg(test)]
mod openapi_tests;
// Only JSON Schema written in YAML uses it so far.
#[cfg(feature = "yaml")]
pub(super) mod written_in;
#[cfg(feature = "xsd")]
mod xsd;

/// A schema format's name, label and file extensions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaFormatInfo {
    /// What hosts select it by, such as `"json-schema"` or `"xsd"`; also the
    /// format's identity.
    pub name: &'static str,
    /// What the editor calls it in messages, such as `"XSD"`.
    pub label: &'static str,
    /// File extensions, lowercase and without the leading dot (`"xsd"`,
    /// `"schema.yaml"`); [`SchemaFormat::from_path`] matches the longest.
    pub extensions: &'static [&'static str],
}

/// Turns a schema written in one format into JSON Schema (draft 2020-12 or
/// earlier, as [`Schema::compile`](crate::Schema::compile) reads it).
///
/// The JSON Schema should use only what completion and hover read (see the
/// README's "Schema formats"): `properties`, `patternProperties`,
/// `additionalProperties`, `items`/`prefixItems`, `anyOf`/`oneOf`/`allOf`,
/// `enum`, `const`, `default`, `examples`, `description`, `deprecated`, and
/// `$ref`s to `#/…` pointers. What the format says that JSON Schema cannot
/// is a [`SchemaLoss`], reported to the host like a schema warning.
///
/// ```
/// use config_editor_leptos::{CompiledSchema, Schema, SchemaCompiler, SchemaError, SchemaFormat,
///     SchemaFormatInfo, SchemaInput};
///
/// /// `name: type` lines: an object with those properties.
/// struct Fields;
/// impl SchemaCompiler for Fields {
///     fn info(&self) -> SchemaFormatInfo {
///         SchemaFormatInfo { name: "fields", label: "Fields", extensions: &["fields"] }
///     }
///     fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
///         let mut properties = serde_json::Map::new();
///         for line in input.text.lines().filter(|line| !line.trim().is_empty()) {
///             let (name, kind) = line.split_once(':')
///                 .ok_or_else(|| SchemaError(format!("no type in \"{line}\"")))?;
///             properties.insert(name.trim().into(), serde_json::json!({"type": kind.trim()}));
///         }
///         Ok(CompiledSchema::new(serde_json::json!({"properties": properties})))
///     }
/// }
///
/// static FIELDS: Fields = Fields;
/// let format = SchemaFormat::new(&FIELDS);
/// let schema = Schema::compile_as(format, &SchemaInput::new("port: integer\n")).unwrap();
/// assert_eq!(schema.json()["properties"]["port"]["type"], "integer");
/// ```
pub trait SchemaCompiler: Send + Sync + 'static {
    /// The format's name, label and file extensions.
    fn info(&self) -> SchemaFormatInfo;

    /// The JSON Schema `input` says, with what it loses.
    ///
    /// # Errors
    ///
    /// A [`SchemaError`] when the text is not a schema of this format, or
    /// refers to a resource `input` does not have (naming its location).
    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError>;

    /// The locations `text` refers to (another file it includes or
    /// imports, an external `$ref`), so a host can fetch them before
    /// compiling. The editor fetches nothing. Called again with each
    /// fetched text, it gives their own references in turn.
    fn imports(&self, text: &str) -> Vec<String> {
        let _ = text;
        Vec::new()
    }
}

/// The files a schema refers to, by location, as the host supplies them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct SchemaResources(BTreeMap<String, String>);

impl SchemaResources {
    /// No resources.
    pub const fn new() -> Self {
        SchemaResources(BTreeMap::new())
    }

    /// Adds `text` at `location` (replacing the text it had).
    pub fn insert(&mut self, location: impl Into<String>, text: impl Into<String>) {
        self.0.insert(location.into(), text.into());
    }

    /// `self` with `text` at `location`.
    #[must_use]
    pub fn with(mut self, location: impl Into<String>, text: impl Into<String>) -> Self {
        self.insert(location, text);
        self
    }

    /// The text at `location`.
    pub fn get(&self, location: &str) -> Option<&str> {
        self.0.get(location).map(String::as_str)
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Each location and its text, in location order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0
            .iter()
            .map(|(location, text)| (location.as_str(), text.as_str()))
    }
}

impl<L: Into<String>, T: Into<String>> FromIterator<(L, T)> for SchemaResources {
    fn from_iter<I: IntoIterator<Item = (L, T)>>(items: I) -> Self {
        SchemaResources(
            items
                .into_iter()
                .map(|(location, text)| (location.into(), text.into()))
                .collect(),
        )
    }
}

/// No resources, for [`SchemaInput::new`].
static NO_RESOURCES: SchemaResources = SchemaResources::new();
/// No options, for [`SchemaInput::new`].
static NO_OPTIONS: LazyLock<Map<String, Value>> = LazyLock::new(Map::new);

/// What a [`SchemaCompiler`] compiles: the schema's text, where it is, the
/// files it may refer to, and the format's options.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct SchemaInput<'a> {
    /// The schema.
    pub text: &'a str,
    /// The text's location (its base URI), against which relative
    /// references resolve.
    pub location: Option<&'a str>,
    /// The other files, by location (see [`SchemaCompiler::imports`]).
    pub resources: &'a SchemaResources,
    /// Options of the format (an XSD's `"root"`), as the host gave them.
    pub options: &'a Map<String, Value>,
}

impl<'a> SchemaInput<'a> {
    /// `text`, with no location, resources or options.
    pub fn new(text: &'a str) -> Self {
        SchemaInput {
            text,
            location: None,
            resources: &NO_RESOURCES,
            options: &NO_OPTIONS,
        }
    }

    /// `self` read at `location`.
    #[must_use]
    pub fn at(mut self, location: &'a str) -> Self {
        self.location = Some(location);
        self
    }

    /// `self` with the files it refers to.
    #[must_use]
    pub fn resources(mut self, resources: &'a SchemaResources) -> Self {
        self.resources = resources;
        self
    }

    /// `self` with the format's options.
    #[must_use]
    pub fn options(mut self, options: &'a Map<String, Value>) -> Self {
        self.options = options;
        self
    }
}

/// What a [`SchemaCompiler`] gives: the JSON Schema, and what the source
/// said that it does not.
#[derive(Clone)]
#[non_exhaustive]
pub struct CompiledSchema {
    /// The JSON Schema.
    pub schema: Value,
    /// What it does not check as the source did.
    pub losses: Vec<SchemaLoss>,
    /// The rules of the format the JSON Schema cannot hold, checked beside
    /// it (see [`SchemaCheck`]).
    pub checks: Vec<Arc<dyn SchemaCheck>>,
}

impl std::fmt::Debug for CompiledSchema {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let checks: Vec<String> = self
            .checks
            .iter()
            .map(|check| check.name().into_owned())
            .collect();
        formatter
            .debug_struct("CompiledSchema")
            .field("schema", &self.schema)
            .field("losses", &self.losses)
            .field("checks", &checks)
            .finish()
    }
}

impl CompiledSchema {
    /// `schema`, with no losses.
    pub fn new(schema: Value) -> Self {
        CompiledSchema {
            schema,
            losses: Vec::new(),
            checks: Vec::new(),
        }
    }

    /// `self` with `losses`.
    #[must_use]
    pub fn with_losses(mut self, losses: Vec<SchemaLoss>) -> Self {
        self.losses = losses;
        self
    }

    /// `self` with `checks`.
    #[must_use]
    pub fn with_checks(mut self, checks: Vec<Arc<dyn SchemaCheck>>) -> Self {
        self.checks = checks;
        self
    }
}

/// A rule of a schema's format that its JSON Schema cannot hold (XSD's
/// element order, its `xs:key`), checked beside it: the editor runs each
/// check where it validates (at the same pause in typing, on the same text
/// and value) and shows its problems with the schema's, placed by pointer
/// as theirs are. It knows nothing of what a check checks. A case a check
/// cannot cover is the format's [`SchemaLoss`], reported when it compiles.
///
/// ```
/// use std::borrow::Cow;
/// use std::sync::Arc;
/// use config_editor_leptos::{CheckProblem, CheckedDocument, SchemaCheck};
///
/// /// Keys in alphabetical order.
/// struct Sorted;
/// impl SchemaCheck for Sorted {
///     fn name(&self) -> Cow<'static, str> {
///         Cow::Borrowed("keys in order")
///     }
///     fn check(&self, document: &CheckedDocument<'_>) -> Vec<CheckProblem> {
///         let Some(map) = document.value.as_object() else { return Vec::new() };
///         let keys: Vec<&String> = map.keys().collect();
///         keys.windows(2)
///             .filter(|pair| pair[0] > pair[1])
///             .map(|pair| CheckProblem::new(format!("/{}", pair[1]), "out of order").at_key())
///             .collect()
///     }
/// }
/// let checks: Vec<Arc<dyn SchemaCheck>> = vec![Arc::new(Sorted)];
/// # let _ = checks;
/// ```
pub trait SchemaCheck: Send + Sync + 'static {
    /// What it checks, in words (`"element order"`).
    fn name(&self) -> Cow<'static, str>;

    /// The problems of `document`, each at the pointer of the value (or
    /// key) it is about.
    fn check(&self, document: &CheckedDocument<'_>) -> Vec<CheckProblem>;
}

/// The document a [`SchemaCheck`] checks.
#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct CheckedDocument<'a> {
    /// Its text, in `language`: the text being validated (a view's, while
    /// editing as another language).
    pub text: &'a str,
    /// The language of `text`.
    pub language: &'a dyn ConfigLanguage,
    /// Its value, as read by the schema.
    pub value: &'a Value,
    /// The compiled JSON Schema (what the format wrote, `$ref`s as
    /// pointers), with any annotations the format added for its checks.
    pub schema: &'a Value,
}

/// A problem a [`SchemaCheck`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CheckProblem {
    /// The JSON pointer of the value it is about.
    pub pointer: String,
    /// What is wrong (the pointer goes before it, as before a schema
    /// problem).
    pub message: String,
    /// Marked on the key rather than the value (an element out of place, a
    /// missing child: its parent's key).
    pub at_key: bool,
    /// How serious it is (default: an error).
    pub severity: Severity,
}

impl CheckProblem {
    /// An error at `pointer`.
    pub fn new(pointer: impl Into<String>, message: impl Into<String>) -> Self {
        CheckProblem {
            pointer: pointer.into(),
            message: message.into(),
            at_key: false,
            severity: Severity::Error,
        }
    }

    /// `self` marked on the key.
    #[must_use]
    pub fn at_key(mut self) -> Self {
        self.at_key = true;
        self
    }

    /// `self` with `severity`.
    #[must_use]
    pub fn severity(mut self, severity: Severity) -> Self {
        self.severity = severity;
        self
    }
}

/// What a schema loses in JSON Schema: a construct of its format that is not
/// checked, or checked more loosely or only approximately.
///
/// Its `Display` (`location:line:column: message`, as much of the place as
/// is known) is what [`Schema::warnings`](crate::Schema::warnings) lists,
/// and `on_schema_warning` receives.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct SchemaLoss {
    /// What is lost, such as `xs:key "userId" is not checked`.
    pub message: String,
    /// The file it is in, when not the schema's own text.
    pub location: Option<String>,
    /// One-based line in that file.
    pub line: Option<usize>,
    /// One-based character column on `line`.
    pub column: Option<usize>,
    /// The construct, such as `xs:key` or `xs:any namespace`.
    pub construct: Cow<'static, str>,
    /// How much of it is lost.
    pub effect: LossEffect,
}

impl SchemaLoss {
    /// A loss of `construct`, at no known place.
    pub fn new(
        construct: impl Into<Cow<'static, str>>,
        effect: LossEffect,
        message: impl Into<String>,
    ) -> Self {
        SchemaLoss {
            message: message.into(),
            location: None,
            line: None,
            column: None,
            construct: construct.into(),
            effect,
        }
    }

    /// `self` at one-based `line` and `column`.
    #[must_use]
    pub fn at(mut self, line: usize, column: usize) -> Self {
        self.line = Some(line);
        self.column = Some(column);
        self
    }

    /// `self` in the file at `location`.
    #[must_use]
    pub fn in_file(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }
}

impl std::fmt::Display for SchemaLoss {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let place: Vec<String> = [
            self.location.clone(),
            self.line.map(|line| line.to_string()),
            self.line.and(self.column).map(|column| column.to_string()),
        ]
        .into_iter()
        .flatten()
        .collect();
        if place.is_empty() {
            formatter.write_str(&self.message)
        } else {
            write!(formatter, "{}: {}", place.join(":"), self.message)
        }
    }
}

/// How much of a construct a [`SchemaLoss`] loses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LossEffect {
    /// It is not checked at all.
    NotChecked,
    /// It is checked, but more values pass than the source allows.
    Loosened,
    /// It is checked by an approximation, which may pass or fail values the
    /// source does not.
    Approximated,
}

impl LossEffect {
    /// `"not-checked"`, `"loosened"` or `"approximated"`.
    pub fn as_str(self) -> &'static str {
        match self {
            LossEffect::NotChecked => "not-checked",
            LossEffect::Loosened => "loosened",
            LossEffect::Approximated => "approximated",
        }
    }
}

impl std::str::FromStr for LossEffect {
    type Err = String;

    /// A name as [`as_str`](Self::as_str) writes it.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        match name.trim() {
            "not-checked" => Ok(LossEffect::NotChecked),
            "loosened" => Ok(LossEffect::Loosened),
            "approximated" => Ok(LossEffect::Approximated),
            _ => Err(format!(
                "unknown loss effect \"{name}\" (expected not-checked, loosened or approximated)"
            )),
        }
    }
}

/// A schema format: a built-in one ([`SchemaFormat::JSON_SCHEMA`], …) or any
/// [`SchemaCompiler`] wrapped with [`SchemaFormat::new`].
///
/// A small `Copy` handle; two handles are equal when their names are. The
/// format of a text is always given, never guessed from it.
#[derive(Clone, Copy)]
pub struct SchemaFormat(&'static dyn SchemaCompiler);

impl SchemaFormat {
    /// JSON Schema written as JSON: the schema as it is.
    pub const JSON_SCHEMA: SchemaFormat = SchemaFormat(&json::JsonSchema);

    /// JSON Schema written as YAML (feature `yaml`), as Kubernetes CRDs and
    /// many repositories keep theirs.
    #[cfg(feature = "yaml")]
    pub const JSON_SCHEMA_YAML: SchemaFormat = SchemaFormat(&super::languages::JSON_SCHEMA_IN_YAML);

    /// XML Schema (XSD 1.0; feature `xsd`), in the XML language's mapping:
    /// see the README's "Schema formats". Options: `root` (the one global
    /// element a document starts with), `configRoot` (an element whose
    /// content is the document), `lax` (content models open to other keys).
    #[cfg(feature = "xsd")]
    pub const XSD: SchemaFormat = SchemaFormat(&xsd::Xsd);

    /// OpenAPI 3.x and Swagger 2.0 documents (JSON, and YAML with feature
    /// `yaml`): the named schema the option `schema` gives (the only one,
    /// without it) is the document's schema; see the README's "Schema
    /// formats".
    pub const OPENAPI: SchemaFormat = SchemaFormat(&super::languages::OPENAPI);

    /// The built-in formats this build enables, in a stable order.
    pub const ALL: &'static [SchemaFormat] = &[
        SchemaFormat::JSON_SCHEMA,
        #[cfg(feature = "yaml")]
        SchemaFormat::JSON_SCHEMA_YAML,
        SchemaFormat::OPENAPI,
        #[cfg(feature = "xsd")]
        SchemaFormat::XSD,
    ];

    /// Wraps a compiler, built-in or the host's.
    pub const fn new(compiler: &'static dyn SchemaCompiler) -> Self {
        SchemaFormat(compiler)
    }

    /// The format's name, label and extensions.
    pub fn info(self) -> SchemaFormatInfo {
        self.0.info()
    }

    /// What hosts select it by, such as `"xsd"`.
    pub fn name(self) -> &'static str {
        self.info().name
    }

    /// What messages call it, such as `"XSD"`.
    pub fn label(self) -> &'static str {
        self.info().label
    }

    /// The built-in format of a schema file, from the longest of the
    /// formats' extensions that its name ends with (`a.schema.yaml` before
    /// `a.yaml`). An explicit call: the editor never guesses a format.
    pub fn from_path(path: &str) -> Option<SchemaFormat> {
        let name = path
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(path)
            .to_ascii_lowercase();
        let mut best: Option<(usize, SchemaFormat)> = None;
        for format in SchemaFormat::ALL {
            for extension in format.info().extensions {
                let matches = name
                    .strip_suffix(extension)
                    .and_then(|stem| stem.strip_suffix('.'))
                    .is_some_and(|stem| !stem.is_empty());
                if matches && best.is_none_or(|(length, _)| extension.len() > length) {
                    best = Some((extension.len(), *format));
                }
            }
        }
        best.map(|(_, format)| format)
    }

    /// The JSON Schema `input` says in this format, with its losses.
    ///
    /// # Errors
    ///
    /// As the compiler's [`SchemaCompiler::compile`].
    pub fn compile(self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        self.0.compile(input)
    }

    /// The locations `text` refers to, for the host to fetch (see
    /// [`SchemaCompiler::imports`]).
    pub fn imports(self, text: &str) -> Vec<String> {
        self.0.imports(text)
    }
}

impl Default for SchemaFormat {
    fn default() -> Self {
        SchemaFormat::JSON_SCHEMA
    }
}

impl PartialEq for SchemaFormat {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name()
    }
}

impl Eq for SchemaFormat {}

impl std::hash::Hash for SchemaFormat {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name().hash(state);
    }
}

impl std::fmt::Debug for SchemaFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.name())
    }
}

impl std::fmt::Display for SchemaFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A name that is not a built-in schema format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnknownSchemaFormat(pub String);

impl std::fmt::Display for UnknownSchemaFormat {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let known: Vec<&str> = SchemaFormat::ALL
            .iter()
            .map(|format| format.name())
            .collect();
        write!(
            formatter,
            "unknown schema format \"{}\" (expected {})",
            self.0,
            known.join(", ")
        )
    }
}

impl std::error::Error for UnknownSchemaFormat {}

impl std::str::FromStr for SchemaFormat {
    type Err = UnknownSchemaFormat;

    /// A built-in format's name, ignoring case.
    fn from_str(name: &str) -> Result<Self, Self::Err> {
        let wanted = name.trim();
        SchemaFormat::ALL
            .iter()
            .copied()
            .find(|format| format.name().eq_ignore_ascii_case(wanted))
            .ok_or_else(|| UnknownSchemaFormat(name.to_string()))
    }
}

#[cfg(test)]
mod tests;
