//! The XSD components as JSON Schema in the editor's XML mapping: elements
//! keyed by local name, attributes as properties marked
//! `"xml": {"attribute": true}` (`@name` beside a child element of that
//! name), repeated elements as lists, text beside attributes as `#text`,
//! named types and global elements under `$defs`.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde_json::{Map, Value, json};

use super::super::super::diagnostics::SchemaError;
use super::super::{LossEffect, SchemaLoss};
use super::checks::IDENTITY;
use super::checks::automaton::Namespaces;
use super::checks::identity::{Constraint, supported};
use super::simple::Simple;
use super::tree::{Node, XSD_NS};

mod content;
mod content_model;
mod emit;
mod simple_types;

/// What compiling the schema decides: whether a closed content model
/// rejects keys it does not name (not with `lax`).
pub(super) struct Settings {
    pub(super) lax: bool,
}

/// The schema's top-level components by name, the `$defs` written so far
/// and the losses found.
pub(super) struct Compiler<'a> {
    settings: Settings,
    elements: HashMap<String, &'a Node>,
    complex_types: HashMap<String, &'a Node>,
    simple_types: HashMap<String, &'a Node>,
    groups: HashMap<String, &'a Node>,
    attribute_groups: HashMap<String, &'a Node>,
    attributes: HashMap<String, &'a Node>,
    /// The members of each substitution group, by its head's name.
    substitutions: HashMap<String, Vec<&'a Node>>,
    pub(super) defs: Map<String, Value>,
    /// `$defs` entries being written (a reference to one is enough).
    started: HashSet<String>,
    /// Simple types already compiled, by name.
    simple_cache: HashMap<String, Simple>,
    /// Named complex types' content models, each read once (a base of
    /// several derived types is not read again for each).
    model_cache: HashMap<String, Model>,
    /// Named complex types whose model is being read.
    modeling: HashSet<String>,
    /// Named complex types referred to while their model was being read.
    pending: BTreeSet<String>,
    /// Global elements referred to while a model was being read.
    pending_elements: BTreeSet<String>,
    /// How many times a named complex type's model was read (the count
    /// test checks each is read once).
    pub(super) models_read: usize,
    pub(super) losses: Vec<SchemaLoss>,
    /// Losses reported once for the whole schema, by construct.
    once: HashSet<&'static str>,
    /// The target namespace each component (by kind and name) was read
    /// from.
    origins: HashMap<(String, String), Option<String>>,
    /// The definitions an `xs:redefine` replaced, by kind and name.
    originals: HashMap<(String, String), &'a Node>,
    /// The groups being read, innermost last.
    grouping: Vec<String>,
    /// The complex types whose content model (for the content check) is
    /// being read, innermost last.
    modeling_content: Vec<String>,
    /// The target namespace of each file read (by location; the schema's
    /// own text under `None`).
    target_namespaces: HashMap<Option<String>, Option<String>>,
    /// Whether a content model was written (the content check runs).
    pub(super) content_checked: bool,
    /// Whether an identity constraint was written (its check runs).
    pub(super) identity_checked: bool,
}

/// One child element or attribute of a content model.
#[derive(Clone, Debug)]
struct Prop {
    schema: Value,
    min: u64,
    /// `None`: unbounded.
    max: Option<u64>,
    attribute: bool,
}

/// An `xs:choice` of the model: the keys each option requires, and whether
/// one must be taken.
#[derive(Clone, Debug)]
struct Choice {
    options: Vec<Vec<String>>,
    required: bool,
}

/// A complex type's content as properties.
#[derive(Clone, Debug, Default)]
struct Model {
    props: Vec<(String, Prop)>,
    choices: Vec<Choice>,
    /// `xs:any` or `xs:anyAttribute`: other keys are allowed.
    open: bool,
    /// `xs:anyAttribute namespace="##other"`: attributes of other
    /// namespaces (prefixed, as the reader keys them) are allowed.
    other_attributes: bool,
    /// The namespaces an `xs:anyAttribute` allows.
    any_attribute: Option<Namespaces>,
    mixed: bool,
    /// Simple content: the text's type.
    text: Option<Simple>,
}

impl Model {
    fn prop_mut(&mut self, key: &str) -> Option<&mut Prop> {
        self.props
            .iter_mut()
            .find(|(name, _)| name == key)
            .map(|(_, prop)| prop)
    }

    /// Adds `prop` as `key`; an element named twice in the model is one
    /// list of both.
    fn add(&mut self, key: String, prop: Prop) {
        match self.prop_mut(&key) {
            Some(known) if !known.attribute && !prop.attribute => {
                known.min += prop.min;
                known.max = known.max.zip(prop.max).map(|(left, right)| left + right);
            }
            Some(_) => {}
            None => self.props.push((key, prop)),
        }
    }
}

fn multiply(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    left.zip(right)
        .map(|(left, right)| left.saturating_mul(right))
}

/// `minOccurs` and `maxOccurs` of a particle.
fn occurs(node: &Node) -> Result<(u64, Option<u64>), String> {
    let min = match node.attribute("minOccurs") {
        Some(text) => text
            .trim()
            .parse()
            .map_err(|_| format!("minOccurs=\"{text}\" is no count"))?,
        None => 1,
    };
    let max = match node.attribute("maxOccurs").map(str::trim) {
        Some("unbounded") => None,
        Some(text) => Some(
            text.parse()
                .map_err(|_| format!("maxOccurs=\"{text}\" is no count"))?,
        ),
        None => Some(1),
    };
    Ok((min, max))
}

/// The pointer of `$defs` entry `name`.
fn reference(name: &str) -> Value {
    json!({ "$ref": format!("#/$defs/{}", name.replace('~', "~0").replace('/', "~1")) })
}

impl<'a> Compiler<'a> {
    pub(super) fn new(schema: &'a Node, settings: Settings) -> Self {
        let mut compiler = Compiler {
            settings,
            elements: HashMap::new(),
            complex_types: HashMap::new(),
            simple_types: HashMap::new(),
            groups: HashMap::new(),
            attribute_groups: HashMap::new(),
            attributes: HashMap::new(),
            substitutions: HashMap::new(),
            defs: Map::new(),
            started: HashSet::new(),
            simple_cache: HashMap::new(),
            model_cache: HashMap::new(),
            modeling: HashSet::new(),
            pending: BTreeSet::new(),
            pending_elements: BTreeSet::new(),
            models_read: 0,
            losses: Vec::new(),
            once: HashSet::new(),
            origins: HashMap::new(),
            originals: HashMap::new(),
            grouping: Vec::new(),
            modeling_content: Vec::new(),
            target_namespaces: HashMap::new(),
            content_checked: false,
            identity_checked: false,
        };
        compiler.index(schema);
        compiler
    }

    /// Indexes the top-level components of `schema` (the schema's own text
    /// first, then each file it includes or imports). A name two
    /// namespaces define is a loss: the data model keys by local name, so
    /// the first one read is used.
    pub(super) fn index(&mut self, schema: &'a Node) {
        let namespace = schema.attribute("targetNamespace").map(str::to_string);
        self.target_namespaces
            .entry(schema.location.as_deref().map(str::to_string))
            .or_insert_with(|| namespace.clone());
        for child in schema.xsd_children() {
            let Some(name) = child.attribute("name").map(str::to_string) else {
                continue;
            };
            let kind = child.name.clone();
            match self.origins.get(&(kind.clone(), name.clone())) {
                Some(known) if *known != namespace => {
                    let message = format!(
                        "two namespaces define the {kind} \"{name}\": the one read first is used (the data model keys by local name)"
                    );
                    self.lose(child, "namespace clash", LossEffect::Approximated, message);
                    continue;
                }
                Some(_) => {}
                None => {
                    self.origins.insert((kind, name.clone()), namespace.clone());
                }
            }
            let table = match child.name.as_str() {
                "element" => {
                    if let Some(head) = child.attribute("substitutionGroup") {
                        for head in head.split_whitespace() {
                            let (_, head) = child.resolve(head);
                            self.substitutions.entry(head).or_default().push(child);
                        }
                    }
                    &mut self.elements
                }
                "complexType" => &mut self.complex_types,
                "simpleType" => &mut self.simple_types,
                "group" => &mut self.groups,
                "attributeGroup" => &mut self.attribute_groups,
                "attribute" => &mut self.attributes,
                _ => continue,
            };
            table.entry(name).or_insert(child);
        }
    }

    /// Applies `redefine` (an `xs:redefine`): each definition in it
    /// replaces the one of its name, which stays the base it derives from.
    pub(super) fn redefine(&mut self, redefine: &'a Node) {
        for child in redefine.xsd_children() {
            let Some(name) = child.attribute("name").map(str::to_string) else {
                continue;
            };
            let table = match child.name.as_str() {
                "complexType" => &mut self.complex_types,
                "simpleType" => &mut self.simple_types,
                "group" => &mut self.groups,
                "attributeGroup" => &mut self.attribute_groups,
                _ => continue,
            };
            if let Some(original) = table.insert(name.clone(), child) {
                self.originals.insert((child.name.clone(), name), original);
            }
        }
    }

    /// The definition `redefinition` replaced (its kind and name), when
    /// `redefinition` is being read and refers to its own name.
    pub(super) fn original(&self, kind: &str, name: &str) -> Option<&'a Node> {
        self.originals
            .get(&(kind.to_string(), name.to_string()))
            .copied()
    }

    /// A loss of `construct` at `node`.
    pub(super) fn lose(
        &mut self,
        node: &Node,
        construct: &'static str,
        effect: LossEffect,
        message: String,
    ) {
        let mut loss = SchemaLoss::new(construct, effect, message).at(node.line, node.column);
        if let Some(location) = &node.location {
            loss = loss.in_file(location.to_string());
        }
        self.losses.push(loss);
    }

    /// [`lose`](Self::lose), reported once for the whole schema.
    fn lose_once(
        &mut self,
        node: &Node,
        construct: &'static str,
        effect: LossEffect,
        message: String,
    ) {
        if self.once.insert(construct) {
            self.lose(node, construct, effect, message);
        }
    }

    /// The target namespace of the file `node` is in (an included file
    /// without one takes the schema's).
    pub(super) fn target_namespace(&self, node: &Node) -> Option<String> {
        self.target_namespaces
            .get(&node.location.as_deref().map(str::to_string))
            .cloned()
            .flatten()
            .or_else(|| self.target_namespaces.get(&None).cloned().flatten())
    }

    /// The global elements a document may start with: every one of
    /// `schemas` (the schema's own text first) that is not abstract, each
    /// name once.
    pub(super) fn global_elements(&self, schemas: &[&'a Node]) -> Vec<&'a Node> {
        let mut seen = HashSet::new();
        schemas
            .iter()
            .flat_map(|schema| schema.xsd_children())
            .filter(|child| child.name == "element" && child.attribute("abstract") != Some("true"))
            .filter(|child| {
                child
                    .attribute("name")
                    .is_some_and(|name| seen.insert(name))
            })
            .collect()
    }

    /// The `$ref` to global element `name`'s schema, written once.
    pub(super) fn global_element(&mut self, name: &str, at: &Node) -> Result<Value, SchemaError> {
        let key = format!("element.{name}");
        if !self.elements.contains_key(name) {
            return Err(error(at, format!("the element \"{name}\" is not declared")));
        }
        if !self.modeling.is_empty() {
            // Read while a type's model is: its own type may derive from
            // that type (Spring's `<list>` extends the type holding it),
            // so it is written once the model is read.
            if !self.started.contains(&key) {
                self.pending_elements.insert(name.to_string());
            }
        } else if self.started.insert(key.clone()) {
            let node = self.elements[name];
            let schema = self.element(node)?;
            self.defs.insert(key.clone(), schema);
        }
        Ok(reference(&key))
    }

    /// The schema of element declaration `node` (global or local; not a
    /// reference).
    pub(super) fn element(&mut self, node: &'a Node) -> Result<Value, SchemaError> {
        let constraints = self.identity_constraints(node);
        if node.attribute("nillable") == Some("true") {
            self.lose_once(
                node,
                "nillable",
                LossEffect::NotChecked,
                "nillable elements (xsi:nil) are not checked".to_string(),
            );
        }
        if node.child("alternative").is_some() {
            self.lose(
                node,
                "xs:alternative",
                LossEffect::NotChecked,
                "xs:alternative (XSD 1.1) is not checked".to_string(),
            );
        }
        let (mut schema, simple) = if let Some(type_name) = node.attribute("type") {
            self.type_reference(node, type_name)?
        } else if let Some(simple) = node.child("simpleType") {
            let simple = self.simple_type(simple)?;
            (simple.value(), Some(simple))
        } else if let Some(complex) = node.child("complexType") {
            (self.complex_type(complex)?, None)
        } else {
            (json!({}), None)
        };
        let typed = |text: &str| match &simple {
            Some(simple) => simple.typed(text),
            None => Value::String(text.to_string()),
        };
        if let Value::Object(map) = &mut schema {
            if let Some(default) = node.attribute("default") {
                map.insert("default".to_string(), typed(default));
            }
            if let Some(fixed) = node.attribute("fixed") {
                map.insert("const".to_string(), typed(fixed));
            }
            if let Some(description) = node.documentation() {
                map.entry("description")
                    .or_insert(Value::String(description));
            }
            if !constraints.is_empty() {
                map.insert(IDENTITY.to_string(), Value::Array(constraints));
            }
        }
        Ok(schema)
    }

    /// The `xs:key`, `xs:keyref` and `xs:unique` of element declaration
    /// `node`, for the identity check; one whose XPath is beyond the
    /// subset it reads is a loss.
    fn identity_constraints(&mut self, node: &Node) -> Vec<Value> {
        let mut found = Vec::new();
        for child in node.xsd_children() {
            let construct = match child.name.as_str() {
                "key" => "xs:key",
                "keyref" => "xs:keyref",
                "unique" => "xs:unique",
                _ => continue,
            };
            let name = child.attribute("name").unwrap_or_default().to_string();
            let selector = child
                .child("selector")
                .and_then(|selector| selector.attribute("xpath"))
                .unwrap_or_default()
                .to_string();
            let fields: Vec<String> = child
                .xsd_children()
                .filter(|field| field.name == "field")
                .filter_map(|field| field.attribute("xpath"))
                .map(str::to_string)
                .collect();
            if fields.is_empty() || !supported(&selector, &fields) {
                self.lose(
                    child,
                    construct,
                    LossEffect::NotChecked,
                    format!("{construct} \"{name}\" is not checked: its XPath is beyond the subset read (child steps, *, .//, |, a field's final @attribute)"),
                );
                continue;
            }
            let refer = child
                .attribute("refer")
                .map(|refer| refer.rsplit(':').next().unwrap_or_default().to_string());
            self.identity_checked = true;
            found.push(
                Constraint {
                    kind: child.name.clone(),
                    name,
                    selector,
                    fields,
                    refer,
                }
                .to_json(),
            );
        }
        found
    }

    /// The schema a `type="…"` names, and the simple type when it is one.
    fn type_reference(
        &mut self,
        at: &'a Node,
        type_name: &str,
    ) -> Result<(Value, Option<Simple>), SchemaError> {
        let (namespace, local) = at.resolve(type_name);
        if namespace.as_deref() == Some(XSD_NS) {
            if local == "anyType" {
                return Ok((json!({}), None));
            }
            let simple = self.built_in(at, &local)?;
            return Ok((simple.value(), Some(simple)));
        }
        if self.simple_types.contains_key(&local) {
            let simple = self.named_simple(&local, at)?;
            let key = format!("type.{local}");
            if !self.defs.contains_key(&key) {
                self.defs.insert(key.clone(), simple.value());
            }
            return Ok((reference(&key), Some(simple)));
        }
        if self.complex_types.contains_key(&local) {
            let key = format!("type.{local}");
            if self.modeling.contains(&local) {
                // Its own content (or a type derived from it) holds an
                // element of it: written once its model is read.
                self.pending.insert(local);
            } else if self.started.insert(key.clone()) {
                self.write_type(&local, at)?;
            }
            return Ok((reference(&key), None));
        }
        // A built-in type written without the XSD prefix in a schema whose
        // default namespace is XSD's is resolved above; anything else is
        // missing.
        Err(error(
            at,
            format!("the type \"{type_name}\" is not defined"),
        ))
    }
}

/// A compile error placed at `node`.
pub(super) fn error(node: &Node, message: String) -> SchemaError {
    let file = node
        .location
        .as_deref()
        .map_or_else(String::new, |location| format!("{location}, "));
    SchemaError(format!(
        "{message} ({file}line {}, column {})",
        node.line, node.column
    ))
}
