//! JSON Schema, written as JSON or YAML (see `read_schema_text`): the
//! schema as it is.

use serde_json::Value;

use super::{CompiledSchema, SchemaCompiler, SchemaFormatInfo, SchemaInput};
use crate::editor::diagnostics::SchemaError;

/// The identity format.
pub(super) struct JsonSchema;

impl SchemaCompiler for JsonSchema {
    fn info(&self) -> SchemaFormatInfo {
        SchemaFormatInfo {
            name: "json-schema",
            label: "JSON Schema",
            extensions: EXTENSIONS,
        }
    }

    /// The schema, the files its `$ref`s name bundled from the resources.
    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        let schema = parse(input.text)?;
        super::bundle::bundle(schema, input, &parse).map(CompiledSchema::new)
    }

    /// The files its `$ref`s name.
    fn imports(&self, text: &str) -> Vec<String> {
        parse(text)
            .map(|schema| super::bundle::external_references(&schema))
            .unwrap_or_default()
    }
}

/// The file extensions: YAML's too where YAML is read.
#[cfg(feature = "yaml")]
const EXTENSIONS: &[&str] = &[
    "json",
    "schema.json",
    "yaml",
    "yml",
    "schema.yaml",
    "schema.yml",
];
#[cfg(not(feature = "yaml"))]
const EXTENSIONS: &[&str] = &["json", "schema.json"];

/// `text` as JSON or YAML, or why not.
pub(crate) fn parse(text: &str) -> Result<Value, SchemaError> {
    crate::editor::languages::read_schema_text(text).map_err(SchemaError)
}
