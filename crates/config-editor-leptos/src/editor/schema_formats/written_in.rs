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

    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        self.language
            .parse(input.text)
            .map(CompiledSchema::new)
            .map_err(|error| SchemaError(error.to_string()))
    }
}
