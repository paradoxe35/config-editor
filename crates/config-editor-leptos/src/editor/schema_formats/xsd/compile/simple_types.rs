//! Simple types: built-in, named, and derived by restriction, list or
//! union.

use std::collections::HashSet;

use serde_json::{Map, Value, json};

use super::super::super::super::diagnostics::SchemaError;
use super::super::super::LossEffect;
use super::super::simple::{Kind, Simple, built_in};
use super::super::tree::{Node, XSD_NS};
use super::{Compiler, error};

impl<'a> Compiler<'a> {
    pub(super) fn built_in(&mut self, at: &Node, local: &str) -> Result<Simple, SchemaError> {
        let Some((simple, loss)) = built_in(local) else {
            return Err(error(at, format!("xs:{local} is no built-in type")));
        };
        if let Some(message) = loss {
            let construct: &'static str = if matches!(local, "float" | "double") {
                "xs:float INF/NaN"
            } else {
                "xs:list"
            };
            self.lose_once(
                at,
                construct,
                LossEffect::Approximated,
                format!("xs:{local}: {message}"),
            );
        }
        Ok(simple)
    }

    /// Named simple type `name`, compiled once.
    pub(super) fn named_simple(&mut self, name: &str, at: &Node) -> Result<Simple, SchemaError> {
        if let Some(simple) = self.simple_cache.get(name) {
            return Ok(simple.clone());
        }
        let Some(node) = self.simple_types.get(name).copied() else {
            return Err(error(
                at,
                format!("the simple type \"{name}\" is not defined"),
            ));
        };
        if !self.started.insert(format!("simple.{name}")) {
            return Err(error(
                node,
                format!("the simple type \"{name}\" is derived from itself"),
            ));
        }
        let simple = self.simple_type(node)?;
        self.simple_cache.insert(name.to_string(), simple.clone());
        Ok(simple)
    }

    /// A simple type by its base name (`base="…"`, `memberTypes`).
    pub(super) fn simple_named(
        &mut self,
        at: &'a Node,
        qualified: &str,
    ) -> Result<Simple, SchemaError> {
        let (namespace, local) = at.resolve(qualified);
        if namespace.as_deref() == Some(XSD_NS) {
            return self.built_in(at, &local);
        }
        self.named_simple(&local, at)
    }

    /// An `xs:simpleType`.
    pub(super) fn simple_type(&mut self, node: &'a Node) -> Result<Simple, SchemaError> {
        let mut simple = if let Some(restriction) = node.child("restriction") {
            self.restriction(restriction)?
        } else if let Some(list) = node.child("list") {
            self.lose(
                list,
                "xs:list",
                LossEffect::Loosened,
                "xs:list items are not checked".to_string(),
            );
            Simple {
                kind: Kind::String,
                schema: json!({"type": "string", "x-xml-whitespace": "collapse"})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            }
        } else if let Some(union) = node.child("union") {
            let mut members = Vec::new();
            for member in union
                .attribute("memberTypes")
                .unwrap_or_default()
                .split_whitespace()
            {
                members.push(self.simple_named(union, member)?);
            }
            for inline in union
                .xsd_children()
                .filter(|child| child.name == "simpleType")
            {
                members.push(self.simple_type(inline)?);
            }
            let kinds: HashSet<_> = members.iter().map(|member| member.kind).collect();
            let kind = if kinds.len() == 1 {
                members[0].kind
            } else {
                Kind::Any
            };
            let mut schema = Map::new();
            schema.insert(
                "anyOf".to_string(),
                Value::Array(members.iter().map(Simple::value).collect()),
            );
            Simple { kind, schema }
        } else {
            Simple {
                kind: Kind::Any,
                schema: Map::new(),
            }
        };
        if let Some(description) = node.documentation() {
            simple
                .schema
                .entry("description")
                .or_insert(Value::String(description));
        }
        Ok(simple)
    }

    /// An `xs:restriction` of a simple type: its base with the facets.
    pub(super) fn restriction(&mut self, node: &'a Node) -> Result<Simple, SchemaError> {
        let mut simple = match (node.attribute("base"), node.child("simpleType")) {
            (Some(base), _) => self.simple_named(node, base)?,
            (None, Some(inline)) => self.simple_type(inline)?,
            (None, None) => Simple {
                kind: Kind::Any,
                schema: Map::new(),
            },
        };
        simple.schema.remove("description");
        let base = simple.clone();
        let mut values = Vec::new();
        let mut patterns = Vec::new();
        for facet in node.xsd_children() {
            let value = facet.attribute("value").unwrap_or_default();
            let number = || base.typed(value).as_number().cloned().map(Value::Number);
            let count = || value.trim().parse::<u64>().ok().map(Value::from);
            let insert = |simple: &mut Simple, key: &str, found: Option<Value>| {
                if let Some(found) = found {
                    simple.schema.insert(key.to_string(), found);
                }
            };
            match facet.name.as_str() {
                "enumeration" => values.push(base.typed(value)),
                "pattern" => patterns.push((facet, value.to_string())),
                "length" => {
                    insert(&mut simple, "minLength", count());
                    insert(&mut simple, "maxLength", count());
                }
                "minLength" => insert(&mut simple, "minLength", count()),
                "maxLength" => insert(&mut simple, "maxLength", count()),
                "minInclusive" | "maxInclusive" | "minExclusive" | "maxExclusive" => {
                    let key = match facet.name.as_str() {
                        "minInclusive" => "minimum",
                        "maxInclusive" => "maximum",
                        "minExclusive" => "exclusiveMinimum",
                        _ => "exclusiveMaximum",
                    };
                    match number() {
                        Some(bound) if matches!(base.kind, Kind::Integer | Kind::Number) => {
                            simple.schema.insert(key.to_string(), bound);
                        }
                        _ => self.lose(
                            facet,
                            "xs:minInclusive",
                            LossEffect::NotChecked,
                            format!(
                                "xs:{} \"{value}\" is not checked on a value that is no number",
                                facet.name
                            ),
                        ),
                    }
                }
                "whiteSpace" => {
                    if simple.kind == Kind::String {
                        simple.schema.insert(
                            "x-xml-whitespace".to_string(),
                            Value::String(value.trim().to_string()),
                        );
                    }
                }
                "totalDigits" | "fractionDigits" => {
                    self.lose(
                        facet,
                        "xs:totalDigits",
                        LossEffect::NotChecked,
                        format!("xs:{} \"{value}\" is not checked", facet.name),
                    );
                }
                "assertion" | "explicitTimezone" => {
                    self.lose(
                        facet,
                        "xs:assertion",
                        LossEffect::NotChecked,
                        format!("xs:{} (XSD 1.1) is not checked", facet.name),
                    );
                }
                _ => {}
            }
        }
        if !values.is_empty() {
            simple
                .schema
                .insert("enum".to_string(), Value::Array(values));
        }
        if !patterns.is_empty() {
            let alternatives: Vec<String> = patterns
                .iter()
                .map(|(_, pattern)| format!("(?:{pattern})"))
                .collect();
            let joined = alternatives.join("|");
            match super::super::regex::translate(&joined) {
                Ok(regex) => {
                    if let Some(known) = simple.schema.remove("pattern") {
                        // Each step of a derivation narrows the one before.
                        let mut all = simple
                            .schema
                            .remove("allOf")
                            .and_then(|all| all.as_array().cloned())
                            .unwrap_or_default();
                        all.push(json!({"pattern": known}));
                        simple.schema.insert("allOf".to_string(), Value::Array(all));
                    }
                    simple
                        .schema
                        .insert("pattern".to_string(), Value::String(regex));
                }
                Err(reason) => {
                    let (facet, pattern) = &patterns[0];
                    self.lose(
                        facet,
                        "xs:pattern",
                        LossEffect::NotChecked,
                        format!("xs:pattern \"{pattern}\" is not checked: {reason}"),
                    );
                }
            }
        }
        Ok(simple)
    }
}
