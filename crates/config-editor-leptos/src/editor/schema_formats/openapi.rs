//! OpenAPI (and Swagger 2.0) documents as schema sources: one of their
//! named schemas (`components.schemas`, Swagger's `definitions`) is the
//! document's schema, the others its `$defs`. OpenAPI 3.1 schemas are
//! JSON Schema 2020-12 already; 3.0 and Swagger ones are rewritten where
//! they differ (`nullable`, boolean `exclusiveMinimum`/`exclusiveMaximum`,
//! `example`). The OpenAPI XML object (`xml.attribute`) is kept, so an
//! XML view writes attributes where the API says.

use serde_json::{Map, Value, json};

use super::super::diagnostics::SchemaError;
use super::{
    CompiledSchema, LossEffect, SchemaCompiler, SchemaFormatInfo, SchemaInput, SchemaLoss,
};

/// Reads a YAML document, or says why not.
pub(crate) type ReadYaml = fn(&str) -> Result<Value, String>;

/// The OpenAPI format; `yaml` reads a document written in YAML (none
/// without the YAML language).
pub(crate) struct OpenApi {
    pub(crate) yaml: Option<ReadYaml>,
}

/// The options the format takes.
const OPTIONS: [&str; 1] = ["schema"];

impl SchemaCompiler for OpenApi {
    fn info(&self) -> SchemaFormatInfo {
        SchemaFormatInfo {
            name: "openapi",
            label: "OpenAPI",
            extensions: &[
                "openapi.json",
                "openapi.yaml",
                "openapi.yml",
                "swagger.json",
                "swagger.yaml",
            ],
        }
    }

    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        if let Some(unknown) = input
            .options
            .keys()
            .find(|key| !OPTIONS.contains(&key.as_str()))
        {
            return Err(SchemaError(format!(
                "unknown option \"{unknown}\" (expected schema)"
            )));
        }
        let document = self.read(input.text)?;
        let version = document
            .get("openapi")
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                document
                    .get("swagger")
                    .and_then(Value::as_str)
                    .map(|_| "2.0".to_string())
            })
            .ok_or_else(|| {
                SchemaError(
                    "the document is no OpenAPI (it has no \"openapi\" or \"swagger\" version)"
                        .to_string(),
                )
            })?;
        let (schemas, prefix) = if version.starts_with('2') {
            (document.get("definitions"), "#/definitions/")
        } else {
            (
                document.pointer("/components/schemas"),
                "#/components/schemas/",
            )
        };
        let Some(Value::Object(schemas)) = schemas else {
            return Err(SchemaError(format!(
                "the document has no named schemas ({})",
                prefix
                    .trim_start_matches("#/")
                    .trim_end_matches('/')
                    .replace('/', ".")
            )));
        };
        let name = match input.options.get("schema") {
            Some(Value::String(name)) => name.clone(),
            Some(_) => {
                return Err(SchemaError(
                    "the option \"schema\" must be a schema's name".to_string(),
                ));
            }
            None if schemas.len() == 1 => schemas.keys().next().cloned().unwrap_or_default(),
            None => {
                let names: Vec<&str> = schemas.keys().map(String::as_str).collect();
                return Err(SchemaError(format!(
                    "say which schema with the option \"schema\" (one of {})",
                    names.join(", ")
                )));
            }
        };
        if !schemas.contains_key(&name) {
            return Err(SchemaError(format!(
                "the document has no schema \"{name}\""
            )));
        }
        let legacy = !version.starts_with("3.1");
        let mut losses = Vec::new();
        let mut defs = Map::new();
        for (key, schema) in schemas {
            let mut schema = schema.clone();
            rewrite(
                &mut schema,
                prefix,
                legacy,
                &format!("{prefix}{key}"),
                &mut losses,
            );
            defs.insert(key.clone(), schema);
        }
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$ref": format!("#/$defs/{}", name.replace('~', "~0").replace('/', "~1")),
            "$defs": defs,
        });
        Ok(CompiledSchema::new(schema).with_losses(losses))
    }
}

impl OpenApi {
    /// The document, as JSON or (with the YAML language) YAML.
    fn read(&self, text: &str) -> Result<Value, SchemaError> {
        match (serde_json::from_str::<Value>(text), self.yaml) {
            (Ok(document), _) => Ok(document),
            (Err(_), Some(yaml)) => yaml(text).map_err(SchemaError),
            (Err(error), None) => Err(SchemaError(format!(
                "{error} (a YAML document needs the yaml feature)"
            ))),
        }
    }
}

/// `schema` (at `pointer` in the document) as JSON Schema 2020-12: refs to
/// named schemas point into `$defs`; with `legacy` (3.0, Swagger) the
/// keywords that differ are rewritten.
fn rewrite(
    schema: &mut Value,
    prefix: &str,
    legacy: bool,
    pointer: &str,
    losses: &mut Vec<SchemaLoss>,
) {
    match schema {
        Value::Object(map) => {
            if let Some(Value::String(reference)) = map.get_mut("$ref")
                && let Some(name) = reference.strip_prefix(prefix)
            {
                *reference = format!("#/$defs/{name}");
            }
            if legacy {
                legacy_keywords(map);
            }
            if map.contains_key("discriminator") {
                losses.push(SchemaLoss::new(
                    "discriminator",
                    LossEffect::Approximated,
                    format!("{pointer}: the discriminator is not checked; the options are matched by their schemas"),
                ));
            }
            for (key, member) in map.iter_mut() {
                let at = format!("{pointer}/{key}");
                match key.as_str() {
                    // Data, not schemas.
                    "enum" | "const" | "default" | "example" | "examples" | "xml"
                    | "discriminator" | "externalDocs" | "required" => {}
                    // Schemas by name.
                    "properties" | "patternProperties" | "$defs" | "definitions"
                    | "dependentSchemas" => {
                        if let Value::Object(named) = member {
                            for (name, schema) in named.iter_mut() {
                                rewrite(schema, prefix, legacy, &format!("{at}/{name}"), losses);
                            }
                        }
                    }
                    _ => rewrite(member, prefix, legacy, &at, losses),
                }
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter_mut().enumerate() {
                rewrite(item, prefix, legacy, &format!("{pointer}/{index}"), losses);
            }
        }
        _ => {}
    }
}

/// OpenAPI 3.0 / Swagger keywords as 2020-12 says them.
fn legacy_keywords(map: &mut Map<String, Value>) {
    if map.remove("nullable") == Some(Value::Bool(true)) {
        match map.get_mut("type") {
            Some(Value::String(name)) => {
                let name = name.clone();
                map.insert("type".to_string(), json!([name, "null"]));
            }
            Some(Value::Array(names)) => names.push(json!("null")),
            _ => {
                let rest = Value::Object(std::mem::take(map));
                map.insert("anyOf".to_string(), json!([rest, {"type": "null"}]));
            }
        }
    }
    for (flag, bound) in [
        ("exclusiveMinimum", "minimum"),
        ("exclusiveMaximum", "maximum"),
    ] {
        match map.get(flag) {
            Some(Value::Bool(true)) => {
                if let Some(value) = map.remove(bound) {
                    map.insert(flag.to_string(), value);
                } else {
                    map.remove(flag);
                }
            }
            Some(Value::Bool(false)) => {
                map.remove(flag);
            }
            _ => {}
        }
    }
    if let Some(example) = map.remove("example") {
        map.entry("examples").or_insert(json!([example]));
    }
}
