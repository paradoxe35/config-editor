//! OpenAPI (and Swagger 2.0) documents as schema sources: one of their
//! named schemas (`components.schemas`, Swagger's `definitions`) is the
//! document's schema, the ones it refers to its `$defs` (the others are
//! left out). OpenAPI 3.1 schemas are JSON Schema 2020-12 already; 3.0 and
//! Swagger ones are rewritten where they differ (`nullable` and Swagger's
//! `x-nullable`, boolean `exclusiveMinimum`/`exclusiveMaximum`,
//! `example`, Swagger's `type: file` as a string); a JSON Schema keyword
//! they do not define (`contains`, `const`, `if`, Swagger's missing
//! `oneOf`) is kept as JSON Schema means it, as the document says it. A
//! `discriminator` picks the option a value is checked by (see
//! `discriminator`). The OpenAPI XML object (`xml.attribute`) is kept, so
//! an XML view writes attributes where the API says.

use std::collections::HashSet;

use serde_json::{Map, Value, json};

use super::super::language::{pointer_key, unescaped_key};
use super::super::schema::{SchemaError, each_schema, each_schema_mut};
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
                    "the document is not an OpenAPI or Swagger document (it has no \"openapi\" or \"swagger\" version)"
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
        let mut losses: Vec<(String, SchemaLoss)> = Vec::new();
        let mut defs = Map::new();
        for (key, schema) in schemas {
            let mut schema = schema.clone();
            if legacy {
                let at = format!("{prefix}{}", pointer_key(key));
                losses.extend(
                    file_losses(&schema, &at)
                        .into_iter()
                        .map(|loss| (key.clone(), loss)),
                );
            }
            rewrite(&mut schema, prefix, legacy);
            defs.insert(key.clone(), schema);
        }
        losses.extend(super::discriminator::compile(&mut defs, prefix));
        // What the named schema never reaches is not its schema: neither
        // checked nor told (a validator may refuse it, a loss in it is
        // none of this schema's).
        if let Some(reached) = reached(&defs, &name) {
            defs.retain(|key, _| reached.contains(key));
            losses.retain(|(key, _)| reached.contains(key));
        }
        let schema = json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "$ref": format!("#/$defs/{}", pointer_key(&name)),
            "$defs": defs,
        });
        Ok(CompiledSchema::new(schema)
            .with_losses(losses.into_iter().map(|(_, loss)| loss).collect()))
    }
}

/// The named schemas `name` reaches through `$ref`s (itself included);
/// `None` when a reference names something else in the schema (an
/// anchor, which may be in any of them), so that all are kept.
fn reached(defs: &Map<String, Value>, name: &str) -> Option<HashSet<String>> {
    let mut reached = HashSet::from([name.to_string()]);
    let mut queue = vec![name.to_string()];
    while let Some(next) = queue.pop() {
        let Some(def) = defs.get(&next) else {
            continue;
        };
        let mut elsewhere = false;
        each_schema(def, &(), &mut |_, members, ()| {
            for keyword in ["$ref", "$dynamicRef"] {
                let Some(reference) = members.get(keyword).and_then(Value::as_str) else {
                    continue;
                };
                match reference.strip_prefix("#/$defs/") {
                    Some(rest) => {
                        let token = rest.split('/').next().unwrap_or_default();
                        let target = unescaped_key(token);
                        if reached.insert(target.clone()) {
                            queue.push(target);
                        }
                    }
                    None if reference.starts_with('#') || keyword == "$dynamicRef" => {
                        elsewhere = true;
                    }
                    None => {}
                }
            }
            Some(())
        });
        if elsewhere {
            return None;
        }
    }
    Some(reached)
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

/// `schema` as JSON Schema 2020-12: refs to named schemas point into
/// `$defs`; with `legacy` (3.0, Swagger) the keywords that differ are
/// rewritten. Each schema inside it is reached by the one walk over a
/// schema's schemas, so a property named like a keyword is no keyword.
fn rewrite(schema: &mut Value, prefix: &str, legacy: bool) {
    each_schema_mut(schema, &mut |map| {
        if let Some(Value::String(reference)) = map.get_mut("$ref")
            && let Some(name) = reference.strip_prefix(prefix)
        {
            *reference = format!("#/$defs/{name}");
        }
        if legacy {
            legacy_keywords(map);
        }
    });
}

/// The losses of `schema` (at `pointer` in the document): each Swagger
/// `type: file`, which JSON Schema has not (checked as a string).
fn file_losses(schema: &Value, pointer: &str) -> Vec<SchemaLoss> {
    let mut losses = Vec::new();
    each_schema(schema, &(), &mut |at, members, ()| {
        if members.get("type").and_then(Value::as_str) == Some(FILE) {
            losses.push(SchemaLoss::new(
                "type: file",
                LossEffect::Loosened,
                format!("{pointer}{at}: type: file is checked as a string"),
            ));
        }
        Some(())
    });
    losses
}

/// Swagger's type of an uploaded or downloaded file.
const FILE: &str = "file";

/// OpenAPI 3.0 / Swagger keywords as 2020-12 says them (Swagger's
/// `x-nullable` extension as 3.0's `nullable`).
fn legacy_keywords(map: &mut Map<String, Value>) {
    if map.get("type").and_then(Value::as_str) == Some(FILE) {
        map.insert("type".to_string(), json!("string"));
    }
    let nullable = map.remove("nullable") == Some(Value::Bool(true));
    let extension = map.remove("x-nullable") == Some(Value::Bool(true));
    if nullable || extension {
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
