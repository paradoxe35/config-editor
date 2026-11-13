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

    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        parse(input.text).map(CompiledSchema::new)
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
