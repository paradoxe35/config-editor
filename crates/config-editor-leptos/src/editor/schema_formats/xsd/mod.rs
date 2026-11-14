//! XML Schema (XSD 1.0) as JSON Schema in the editor's XML mapping (see
//! the README's "XML mapping" and "Schema formats"). Feature `xsd`.
//!
//! Every global element is a property of the document (a file may hold
//! any of them at the top), unless the option `root` names one; the option
//! `configRoot` compiles one element's content as the document, for files
//! whose top element is `config-root="true"`. Content models are closed
//! (`additionalProperties: false`) unless the option `lax` is `true`. What
//! JSON Schema cannot check (identity constraints, element order, …) is a
//! [`SchemaLoss`](super::SchemaLoss).

mod checks;
mod compile;
mod files;
mod regex;
mod simple;
mod tree;

use serde_json::{Map, Value, json};

use super::super::diagnostics::SchemaError;
use super::{
    CompiledSchema, LossEffect, SchemaCheck, SchemaCompiler, SchemaFormatInfo, SchemaInput,
};
use compile::{Compiler, Settings, error};
use std::sync::Arc;
use tree::Node;

/// The XSD format.
pub(super) struct Xsd;

/// The options the format takes.
const OPTIONS: [&str; 3] = ["root", "configRoot", "lax"];

/// Constructs XSD 1.1 adds, or that no JSON Schema keyword holds: each is a
/// loss where it is written.
const NOT_CHECKED: [(&str, &str); 3] = [
    ("assert", "xs:assert (XSD 1.1) is not checked"),
    ("override", "xs:override (XSD 1.1) is not applied"),
    ("notation", "xs:notation is not checked"),
];

impl SchemaCompiler for Xsd {
    fn info(&self) -> SchemaFormatInfo {
        SchemaFormatInfo {
            name: "xsd",
            label: "XSD",
            extensions: &["xsd"],
        }
    }

    fn compile(&self, input: &SchemaInput<'_>) -> Result<CompiledSchema, SchemaError> {
        let root = tree::read(input.text)?;
        if !root.is("schema") {
            return Err(error(
                &root,
                "the document is no XML Schema (its root is not xs:schema)".to_string(),
            ));
        }
        if let Some(unknown) = input
            .options
            .keys()
            .find(|key| !OPTIONS.contains(&key.as_str()))
        {
            return Err(SchemaError(format!(
                "unknown option \"{unknown}\" (expected {})",
                OPTIONS.join(", ")
            )));
        }
        let option = |name: &str| input.options.get(name);
        let text_option = |name: &str| -> Result<Option<&str>, SchemaError> {
            match option(name) {
                None | Some(Value::Null) => Ok(None),
                Some(Value::String(text)) => Ok(Some(text.as_str())),
                Some(_) => Err(SchemaError(format!(
                    "the option \"{name}\" must be an element name"
                ))),
            }
        };
        let lax = match option("lax") {
            None | Some(Value::Null) => false,
            Some(Value::Bool(lax)) => *lax,
            Some(_) => {
                return Err(SchemaError(
                    "the option \"lax\" must be true or false".to_string(),
                ));
            }
        };
        let only = text_option("root")?;
        let config_root = text_option("configRoot")?;
        let others = files::load(&root, input.location, input.resources)?;
        let schemas: Vec<&Node> = std::iter::once(&root).chain(others.iter()).collect();
        let mut compiler = Compiler::new(&root, Settings { lax });
        for other in &others {
            compiler.index(other);
        }
        for schema in &schemas {
            for redefine in schema.xsd_children().filter(|node| node.name == "redefine") {
                compiler.redefine(redefine);
            }
        }
        for schema in &schemas {
            for (name, message) in NOT_CHECKED {
                for found in descendants(schema).filter(|node| node.is(name)) {
                    let construct = match name {
                        "assert" => "xs:assert",
                        "override" => "xs:override",
                        _ => "xs:notation",
                    };
                    compiler.lose(
                        found,
                        construct,
                        LossEffect::NotChecked,
                        message.to_string(),
                    );
                }
            }
            for attribute in ["blockDefault", "finalDefault"] {
                if schema.attribute(attribute).is_some() {
                    compiler.lose(
                        schema,
                        "derivation control",
                        LossEffect::NotChecked,
                        format!("{attribute} (derivation control) is not checked"),
                    );
                }
            }
        }
        let schema = if let Some(name) = config_root {
            let Some(element) = compiler
                .global_elements(&schemas)
                .into_iter()
                .find(|node| node.attribute("name") == Some(name))
            else {
                return Err(SchemaError(format!(
                    "configRoot: the schema declares no global element \"{name}\""
                )));
            };
            compiler.element(element)?
        } else {
            let mut properties = Map::new();
            for element in compiler.global_elements(&schemas) {
                let Some(name) = element.attribute("name") else {
                    continue;
                };
                if only.is_some_and(|only| only != name) {
                    continue;
                }
                let schema = compiler.global_element(name, element)?;
                properties.insert(name.to_string(), schema);
            }
            if let Some(only) = only.filter(|_| properties.is_empty()) {
                return Err(SchemaError(format!(
                    "root: the schema declares no global element \"{only}\""
                )));
            }
            let mut document = json!({"type": "object", "properties": properties});
            if !lax {
                document["additionalProperties"] = Value::Bool(false);
            }
            if let Some(only) = only {
                document["required"] = json!([only]);
            }
            document
        };
        compiler.finish(&root)?;
        let mut document = match schema {
            Value::Object(map) => map,
            other => {
                let mut map = Map::new();
                map.insert("allOf".to_string(), json!([other]));
                map
            }
        };
        document.insert(
            "$schema".to_string(),
            json!("https://json-schema.org/draft/2020-12/schema"),
        );
        if let Some(description) = root.documentation() {
            document
                .entry("description")
                .or_insert(Value::String(description));
        }
        if !compiler.defs.is_empty() {
            document.insert(
                "$defs".to_string(),
                Value::Object(std::mem::take(&mut compiler.defs)),
            );
        }
        let mut checks: Vec<Arc<dyn SchemaCheck>> = Vec::new();
        if compiler.content_checked {
            checks.push(Arc::new(checks::ContentCheck));
        }
        if compiler.identity_checked {
            checks.push(Arc::new(checks::IdentityCheck));
        }
        Ok(CompiledSchema::new(Value::Object(document))
            .with_losses(compiler.losses)
            .with_checks(checks))
    }

    /// The `schemaLocation`s of its `xs:include`, `xs:import` and
    /// `xs:redefine`.
    fn imports(&self, text: &str) -> Vec<String> {
        let Ok(root) = tree::read(text) else {
            return Vec::new();
        };
        root.xsd_children()
            .filter(|node| {
                matches!(
                    node.name.as_str(),
                    "include" | "import" | "redefine" | "override"
                )
            })
            .filter_map(|node| node.attribute("schemaLocation"))
            .map(str::to_string)
            .collect()
    }
}

/// `node` and every element under it.
fn descendants(node: &Node) -> Box<dyn Iterator<Item = &Node> + '_> {
    Box::new(std::iter::once(node).chain(node.children.iter().flat_map(descendants)))
}

#[cfg(all(test, feature = "xml", feature = "json", feature = "schema-validation"))]
mod corpus_tests;
#[cfg(test)]
mod files_tests;
#[cfg(test)]
mod tests;
