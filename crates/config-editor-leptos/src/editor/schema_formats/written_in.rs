//! JSON Schema written in one of the editor's languages: the document that
//! language reads, taken as the schema.

use super::{CompiledSchema, SchemaCompiler, SchemaFormatInfo, SchemaInput};
use crate::editor::Language;
use crate::editor::diagnostics::SchemaError;

/// JSON Schema written in `language`, named by `info`.
pub(crate) struct SchemaWrittenIn {
    pub(crate) language: Language,
    pub(crate) info: SchemaFormatInfo,
}

impl SchemaCompiler for SchemaWrittenIn {
    fn info(&self) -> SchemaFormatInfo {
        self.info
    }

    /// The schema, the files its `$ref`s name (in the same language)
    /// bundled from the resources.
    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        let parse = |text: &str| {
            self.language
                .parse(text)
                .map_err(|error| SchemaError(error.to_string()))
        };
        let schema = parse(input.text)?;
        super::bundle::bundle(schema, input, &parse).map(CompiledSchema::new)
    }

    /// The files its `$ref`s name.
    fn imports(&self, text: &str) -> Vec<String> {
        self.language
            .parse(text)
            .map(|schema| super::bundle::external_references(&schema))
            .unwrap_or_default()
    }
}
