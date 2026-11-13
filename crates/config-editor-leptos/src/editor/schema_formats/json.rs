//! JSON Schema written as JSON: the schema as it is.

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
            extensions: &["json", "schema.json"],
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

/// `text` as JSON, or why not: a document nested deeper than the reader
/// goes says so in words.
pub(crate) fn parse(text: &str) -> Result<Value, SchemaError> {
    serde_json::from_str(text).map_err(|error| {
        SchemaError(
            if error.to_string().starts_with("recursion limit exceeded") {
                "it nests objects and lists more than 128 levels deep, deeper than the editor reads"
                    .to_string()
            } else {
                error.to_string()
            },
        )
    })
}
