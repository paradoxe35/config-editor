//! Compiling a schema text: read it, rewrite the references the editor
//! reads differently from the text, build the validator, and read once
//! what the editor needs to know of the whole schema.

use serde_json::Value;

use super::SchemaFailure;
use super::subschemas::each_schema;
use crate::editor::schema_formats::{SchemaCheck, SchemaFormat, SchemaInput, SchemaLoss};

/// A schema compiled once, shared by every editor using it.
pub(crate) struct SchemaBundle {
    /// The schema completion, hover, reading and locks walk: references by
    /// `$id`/`$anchor` rewritten as pointers (see `ui_refs`).
    pub(crate) schema: Value,
    /// The JSON Schema as compiled, where the editor rewrote some of its
    /// references in `schema` (see [`super::Schema::json`]).
    pub(crate) emitted: Option<Value>,
    /// What the editor cannot use in the schema (see [`super::Schema::warnings`]).
    pub(crate) warnings: Vec<String>,
    /// What the schema's format said that the JSON Schema does not check.
    pub(crate) losses: Vec<SchemaLoss>,
    /// The format's checks of what the JSON Schema cannot hold.
    pub(crate) checks: Vec<std::sync::Arc<dyn SchemaCheck>>,
    /// Whether the warnings went to the console already: the editors that
    /// share this compiled schema (a documents view's, of one schema text)
    /// say them there once.
    pub(crate) warned_console: std::sync::atomic::AtomicBool,
    /// Some schema is `deprecated`: documents are checked for its key.
    pub(crate) has_deprecated: bool,
    /// Some schema is `readOnly`, `writeOnly` or `x-secret`: documents are
    /// walked for them.
    pub(crate) has_marks: bool,
    /// Some schema is `writeOnly` or `x-secret`: a syntax message hides
    /// what it quotes of the text (see `diagnostics`).
    pub(crate) has_secrets: bool,
    /// The validator and its explaining twins.
    #[cfg(feature = "schema-validation")]
    pub(crate) validation: crate::editor::diagnostics::Validation,
}

/// The compiled schema, or why `schema_text` (JSON, or YAML with the
/// `yaml` feature) is no JSON Schema (worded by the `invalidSchema` text
/// where it is shown, see [`super::schema_message`]): empty for a text that
/// is empty or only white space.
pub(crate) fn build_schema(schema_text: &str) -> Result<SchemaBundle, String> {
    // A byte order mark (a file saved by an editor that writes one) is no
    // part of the schema.
    let schema_text = schema_text.strip_prefix('\u{feff}').unwrap_or(schema_text);
    if schema_text.trim().is_empty() {
        return Err(String::new());
    }
    let schema = crate::editor::languages::read_schema_text(schema_text)?;
    build_compiled(schema, Vec::new(), Vec::new())
}

/// The bundle of `input` in `format`.
pub(crate) fn build_formatted(
    format: SchemaFormat,
    input: &SchemaInput<'_>,
) -> Result<SchemaBundle, SchemaFailure> {
    // A byte order mark is no part of the schema, in any format.
    let input = &SchemaInput {
        text: input.text.strip_prefix('\u{feff}').unwrap_or(input.text),
        ..*input
    };
    let compiled = format
        .compile(input)
        .map_err(|error| SchemaFailure::Source {
            format: format.label(),
            error: error.0,
        })?;
    build_compiled(compiled.schema, compiled.losses, compiled.checks)
        .map_err(SchemaFailure::JsonSchema)
}

/// The bundle of a compiled JSON Schema and the losses of its format, or why
/// it is no JSON Schema.
fn build_compiled(
    mut schema: Value,
    losses: Vec<SchemaLoss>,
    checks: Vec<std::sync::Arc<dyn SchemaCheck>>,
) -> Result<SchemaBundle, String> {
    super::empty_refs::to_root(&mut schema);
    #[cfg(feature = "schema-validation")]
    let validation = crate::editor::diagnostics::Validation::build(&schema)?;
    // The validator resolves `$id` and `$anchor` references itself;
    // completion and hover read them as pointers.
    // TODO(merge-test): v0.2.0 cloned the schema only when `to_pointers`
    // had rewrites to make; here it is always cloned.
    let before = schema.clone();
    crate::editor::ui_refs::to_pointers(&mut schema);
    let emitted = (before != schema).then_some(before);
    let mut facts = Facts::of(&schema);
    facts
        .warnings
        .extend(losses.iter().map(SchemaLoss::to_string));
    let has_marks = crate::editor::protect::mentions_marks(&schema);
    let has_secrets = has_marks && crate::editor::protect::mentions_secrets(&schema);
    Ok(SchemaBundle {
        schema,
        emitted,
        warnings: facts.warnings,
        losses,
        checks,
        warned_console: std::sync::atomic::AtomicBool::new(false),
        has_deprecated: facts.has_deprecated,
        has_marks,
        has_secrets,
        #[cfg(feature = "schema-validation")]
        validation,
    })
}

/// What one walk over every schema of a compiled schema finds.
#[derive(Debug, Default, PartialEq, Eq)]
struct Facts {
    warnings: Vec<String>,
    has_deprecated: bool,
}

impl Facts {
    fn of(schema: &Value) -> Self {
        let mut facts = Facts::default();
        each_schema(schema, &(), &mut |pointer, members, ()| {
            let marked = |keyword: &str| members.get(keyword) == Some(&Value::Bool(true));
            facts.has_deprecated |= marked("deprecated");
            if let Some(Value::Object(patterns)) = members.get("patternProperties") {
                let place = format!("{pointer}/patternProperties");
                for pattern in patterns
                    .keys()
                    .filter(|pattern| !crate::editor::completion::compiles(pattern))
                {
                    facts.warnings.push(format!(
                        "{place}: the pattern \"{pattern}\" is not supported by the editor's \
                         completion engine; keys it matches get no completions or hover docs \
                         (validation still checks them)"
                    ));
                }
            }
            Some(())
        });
        facts
    }
}

/// The JSON pointer of `tokens` (each escaped).
#[cfg(test)]
fn pointer_of(tokens: &[&str]) -> String {
    use super::subschemas::escaped;
    tokens
        .iter()
        .map(|token| format!("/{}", escaped(token)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Every keyword name, used as a property, definition or dependency
    /// name, is read as that name's schema by every pass of the compile
    /// step; a schema written inside data is read by none.
    #[test]
    fn every_compile_pass_reads_keyword_names_as_names_and_data_as_data() {
        let names = [
            "const",
            "enum",
            "default",
            "examples",
            "example",
            "properties",
            "patternProperties",
            "$defs",
            "definitions",
            "dependentSchemas",
            "dependencies",
            "oneOf",
            "if",
            "$ref",
        ];
        let holders = [
            "properties",
            "patternProperties",
            "$defs",
            "definitions",
            "dependentSchemas",
            "dependencies",
        ];
        let inner = || {
            json!({
                "$ref": "",
                "additionalProperties": false,
                "deprecated": true,
                        "items": {"patternProperties": {"^(?!x-)": {}}},
            })
        };
        for name in names {
            for holder in holders {
                let mut schema = json!({ holder: { name: inner() } });
                let at = pointer_of(&[holder, name]);
                super::super::empty_refs::to_root(&mut schema);
                assert_eq!(
                    schema.pointer(&format!("{at}/$ref")),
                    Some(&json!("#")),
                    "empty_refs at {at}"
                );
                #[cfg(feature = "schema-validation")]
                {
                    let validated = super::super::false_schema::for_validation(&schema);
                    assert!(
                        validated
                            .as_ref()
                            .and_then(|validated| validated.pointer(&format!("{at}/properties")))
                            .is_some(),
                        "false_schema at {at}"
                    );
                }
                let facts = Facts::of(&schema);
                assert!(facts.has_deprecated, "deprecated at {at}");
                let mut anchored = json!({ holder: { name: {"$anchor": "t"} }, "$ref": "#t" });
                crate::editor::ui_refs::to_pointers(&mut anchored);
                assert_eq!(anchored["$ref"], json!(format!("#{at}")), "ui_refs at {at}");
                assert_eq!(facts.warnings.len(), 1, "patterns at {at}: {facts:?}");
            }
        }
        for data in ["const", "enum", "default", "examples", "example"] {
            let mut schema = json!({ data: inner(), "properties": {"a": {data: [inner()]}} });
            let written = schema.clone();
            super::super::empty_refs::to_root(&mut schema);
            assert_eq!(schema, written, "empty_refs in {data}");
            #[cfg(feature = "schema-validation")]
            assert_eq!(
                super::super::false_schema::for_validation(&schema),
                None,
                "false_schema in {data}"
            );
            assert_eq!(Facts::of(&schema), Facts::default(), "facts in {data}");
        }
    }
}
