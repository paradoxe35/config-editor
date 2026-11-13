//! A schema's format, resources and options (what JavaScript passes beside
//! its text, read in `schema_formats`), and what identifies the schema they
//! compile to. No JavaScript here: the native tests include this file.

use std::cell::Cell;

use config_editor_leptos::{Schema, SchemaError, SchemaFormat, SchemaInput, SchemaResources};
use serde_json::{Map, Value};

/// A schema's format, resources and options.
pub(crate) struct SchemaSpec {
    pub(crate) format: SchemaFormat,
    pub(crate) resources: SchemaResources,
    pub(crate) options: Map<String, Value>,
}

impl SchemaSpec {
    /// JSON Schema, with no resources or options.
    pub(crate) fn json_schema() -> Self {
        SchemaSpec {
            format: SchemaFormat::JSON_SCHEMA,
            resources: SchemaResources::new(),
            options: Map::new(),
        }
    }

    /// `text` in this format, compiled.
    pub(crate) fn compile(&self, text: &str) -> Result<Schema, SchemaError> {
        let input = SchemaInput::new(text)
            .resources(&self.resources)
            .options(&self.options);
        Schema::compile_as(self.format, &input)
    }

    /// What identifies the schema `text` compiles to in this format.
    pub(crate) fn key(&self, text: &str) -> SpecKey {
        (
            self.format.name(),
            text.to_string(),
            self.resources.clone(),
            Value::Object(self.options.clone()).to_string(),
            registrations(),
        )
    }
}

/// A schema's format name, text, resources and options (as JSON text), and
/// the [`registrations`] made when it was read: a format registered again
/// compiles the same text anew.
pub(crate) type SpecKey = (&'static str, String, SchemaResources, String, u64);

thread_local! {
    static REGISTRATIONS: Cell<u64> = const { Cell::new(0) };
}

/// How many times `registerSchemaFormat` was called: a schema compiled
/// before the last call may have been compiled by a definition since
/// replaced.
pub(crate) fn registrations() -> u64 {
    REGISTRATIONS.with(Cell::get)
}

/// A format was registered (again).
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
pub(crate) fn note_registration() {
    REGISTRATIONS.with(|count| count.set(count.get() + 1));
}
