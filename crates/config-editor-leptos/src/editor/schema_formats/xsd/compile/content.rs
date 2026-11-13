//! Complex types' content models: derivation, particles, choices and
//! attributes.

use serde_json::{Map, Value, json};

use super::super::super::super::diagnostics::SchemaError;
use super::super::super::LossEffect;
use super::super::simple::{Kind, Simple};
use super::super::tree::{Node, XSD_NS};
use super::{Choice, Compiler, Model, Prop, error, multiply, occurs};

impl<'a> Compiler<'a> {
    /// An `xs:complexType`.
    pub(super) fn complex_type(&mut self, node: &'a Node) -> Result<Value, SchemaError> {
        let model = self.model(node, 0)?;
        let mut schema = self.model_schema(model);
        if let (Some(description), Value::Object(map)) = (node.documentation(), &mut schema) {
            map.entry("description")
                .or_insert(Value::String(description));
        }
        Ok(schema)
    }

    /// The content model of complex type `node` (named or anonymous).
    pub(super) fn model(&mut self, node: &'a Node, depth: usize) -> Result<Model, SchemaError> {
        if node.attribute("name").is_some() {
            self.models_read += 1;
        }
        if depth > 64 {
            return Err(error(node, "a type derives from itself".to_string()));
        }
        if node.attribute("abstract") == Some("true") {
            self.lose_once(node, "abstract type", LossEffect::Loosened, "abstract types and xsi:type are not checked: a document may use the abstract type itself".to_string());
        }
        let mut model = Model {
            mixed: node.attribute("mixed") == Some("true"),
            ..Model::default()
        };
        if let Some(content) = node.child("complexContent") {
            if content.attribute("mixed") == Some("true") {
                model.mixed = true;
            }
            if let Some(derivation) = content
                .child("extension")
                .or_else(|| content.child("restriction"))
            {
                let base = derivation.attribute("base").unwrap_or_default();
                let (namespace, local) = derivation.resolve(base);
                let base_model = if namespace.as_deref() == Some(XSD_NS) && local == "anyType" {
                    Model::default()
                } else {
                    self.named_model(&local, derivation, depth)?
                };
                if derivation.name == "extension" {
                    model.props = base_model.props;
                    model.choices = base_model.choices;
                    model.open = base_model.open;
                    model.other_attributes = base_model.other_attributes;
                    model.mixed |= base_model.mixed;
                } else {
                    // A restriction restates the content; the attributes
                    // carry over unless it changes or prohibits them.
                    model.props = base_model
                        .props
                        .into_iter()
                        .filter(|(_, prop)| prop.attribute)
                        .collect();
                }
                self.content(derivation, &mut model)?;
            }
        } else if let Some(content) = node.child("simpleContent") {
            if let Some(derivation) = content
                .child("extension")
                .or_else(|| content.child("restriction"))
            {
                let base = derivation.attribute("base").unwrap_or_default();
                let (namespace, local) = derivation.resolve(base);
                if namespace.as_deref() != Some(XSD_NS) && !self.simple_types.contains_key(&local) {
                    // A complex type with simple content as the base.
                    let base_model = self.named_model(&local, derivation, depth)?;
                    model.props = base_model.props;
                    model.text = base_model.text;
                    if derivation.name == "restriction" {
                        let restricted = self.restriction(derivation)?;
                        if !restricted.schema.is_empty() || model.text.is_none() {
                            model.text = Some(merge(model.text.take(), restricted));
                        }
                    }
                } else if derivation.name == "restriction" {
                    model.text = Some(self.restriction(derivation)?);
                } else {
                    model.text = Some(self.simple_named(derivation, base)?);
                }
                self.attributes_of(derivation, &mut model)?;
            }
        } else {
            self.content(node, &mut model)?;
        }
        Ok(model)
    }

    /// Named complex type `name`'s model, read once.
    pub(super) fn named_model(
        &mut self,
        name: &str,
        at: &'a Node,
        depth: usize,
    ) -> Result<Model, SchemaError> {
        if let Some(model) = self.model_cache.get(name) {
            return Ok(model.clone());
        }
        let Some(node) = self.complex_types.get(name).copied() else {
            return Err(error(at, format!("the type \"{name}\" is not defined")));
        };
        if !self.modeling.insert(name.to_string()) {
            return Err(error(
                node,
                format!("the type \"{name}\" derives from itself"),
            ));
        }
        let model = self.model(node, depth + 1);
        self.modeling.remove(name);
        let model = model?;
        self.model_cache.insert(name.to_string(), model.clone());
        Ok(model)
    }

    /// Writes named complex type `name` into `$defs`.
    pub(super) fn write_type(&mut self, name: &str, at: &'a Node) -> Result<(), SchemaError> {
        let model = self.named_model(name, at, 0)?;
        let mut schema = self.model_schema(model);
        if let (Some(description), Value::Object(map)) =
            (self.complex_types[name].documentation(), &mut schema)
        {
            map.entry("description")
                .or_insert(Value::String(description));
        }
        self.defs.insert(format!("type.{name}"), schema);
        Ok(())
    }

    /// Writes the types referred to while their own model was read.
    pub(in super::super) fn finish(&mut self, at: &'a Node) -> Result<(), SchemaError> {
        loop {
            if let Some(name) = self.pending_elements.iter().next().cloned() {
                self.pending_elements.remove(&name);
                let key = format!("element.{name}");
                if self.started.insert(key.clone()) {
                    let node = self.elements[&name];
                    let schema = self.element(node)?;
                    self.defs.insert(key, schema);
                }
                continue;
            }
            let Some(name) = self
                .pending
                .iter()
                .find(|name| !self.defs.contains_key(&format!("type.{name}")))
                .cloned()
            else {
                return Ok(());
            };
            self.pending.remove(&name);
            self.started.insert(format!("type.{name}"));
            self.write_type(&name, at)?;
        }
    }

    /// The particles and attributes `node` (a complex type or a
    /// derivation) adds to `model`.
    pub(super) fn content(&mut self, node: &'a Node, model: &mut Model) -> Result<(), SchemaError> {
        for child in node.xsd_children() {
            match child.name.as_str() {
                "sequence" | "choice" | "all" | "group" => {
                    self.particle(child, (1, Some(1)), model, false)?
                }
                _ => {}
            }
        }
        self.attributes_of(node, model)
    }

    /// Adds particle `node` (an element, a model group, a group reference,
    /// a wildcard), occurring `outer` times, to `model`. `in_choice`: an
    /// option of a choice (none of its elements is required by itself).
    pub(super) fn particle(
        &mut self,
        node: &'a Node,
        outer: (u64, Option<u64>),
        model: &mut Model,
        in_choice: bool,
    ) -> Result<(), SchemaError> {
        let (min, max) = occurs(node).map_err(|message| error(node, message))?;
        let min = if in_choice {
            0
        } else {
            outer.0.saturating_mul(min)
        };
        let max = multiply(outer.1, max);
        match node.name.as_str() {
            "element" => self.element_particle(node, (min, max), model)?,
            "sequence" | "all" => {
                if node.name == "sequence"
                    && node
                        .xsd_children()
                        .filter(|child| child.name == "element")
                        .count()
                        > 1
                {
                    self.lose_once(
                        node,
                        "xs:sequence order",
                        LossEffect::NotChecked,
                        "the order of the elements of an xs:sequence is not checked".to_string(),
                    );
                }
                for child in node.xsd_children() {
                    self.particle(child, (min, max), model, false)?;
                }
            }
            "choice" => self.choice(node, (min, max), model)?,
            "group" => {
                let Some(name) = node.attribute("ref") else {
                    return Ok(());
                };
                let (_, local) = node.resolve(name);
                let Some(group) = self.groups.get(&local).copied() else {
                    return Err(error(node, format!("the group \"{name}\" is not defined")));
                };
                for child in group.xsd_children() {
                    if matches!(child.name.as_str(), "sequence" | "choice" | "all") {
                        self.particle(child, (min, max), model, false)?;
                    }
                }
            }
            "any" => {
                model.open = true;
                self.lose(node, "xs:any", LossEffect::Loosened, "xs:any allows any other element here; its namespace and processContents are not checked".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    /// An `xs:choice`: each option's keys, optional; one taken is checked
    /// where every option requires a key of its own and the choice is not
    /// repeated.
    pub(super) fn choice(
        &mut self,
        node: &'a Node,
        (min, max): (u64, Option<u64>),
        model: &mut Model,
    ) -> Result<(), SchemaError> {
        let mut options = Vec::new();
        for option in node.xsd_children().filter(|child| {
            matches!(
                child.name.as_str(),
                "element" | "sequence" | "choice" | "all" | "group" | "any"
            )
        }) {
            let mut inner = Model::default();
            self.particle(option, (1, Some(1)), &mut inner, false)?;
            let required: Vec<String> = inner
                .props
                .iter()
                .filter(|(_, prop)| prop.min > 0)
                .map(|(key, _)| key.clone())
                .collect();
            options.push(required);
            model.open |= inner.open;
            for (key, mut prop) in inner.props {
                prop.min = 0;
                prop.max = multiply(prop.max, max);
                model.add(key, prop);
            }
        }
        let exact =
            max == Some(1) && options.len() > 1 && options.iter().all(|keys| !keys.is_empty());
        if exact {
            model.choices.push(Choice {
                options,
                required: min > 0,
            });
        } else if options.len() > 1 {
            self.lose(node, "xs:choice", LossEffect::Loosened, "this xs:choice is checked as each option allowed: taking several of its options is not refused".to_string());
        }
        Ok(())
    }

    /// An element particle: a local declaration or a reference to a global
    /// one (with its substitution group).
    pub(super) fn element_particle(
        &mut self,
        node: &'a Node,
        (min, max): (u64, Option<u64>),
        model: &mut Model,
    ) -> Result<(), SchemaError> {
        if let Some(name) = node.attribute("ref") {
            let (_, local) = node.resolve(name);
            let members: Vec<&'a Node> =
                self.substitutions.get(&local).cloned().unwrap_or_default();
            let head = self.elements.get(&local).copied();
            let head_usable = head.is_some_and(|head| head.attribute("abstract") != Some("true"));
            let alternatives = members.len() + usize::from(head_usable);
            if !members.is_empty() {
                self.lose(node, "substitution group", LossEffect::Loosened, format!("the substitution group of \"{local}\" is checked as each member allowed: taking several is not refused"));
            }
            let min = if alternatives > 1 { 0 } else { min };
            if head_usable || members.is_empty() {
                let schema = self.global_element(&local, node)?;
                model.add(
                    local.clone(),
                    Prop {
                        schema,
                        min,
                        max,
                        attribute: false,
                    },
                );
            }
            for member in members {
                let Some(name) = member.attribute("name") else {
                    continue;
                };
                if member.attribute("abstract") == Some("true") {
                    continue;
                }
                let schema = self.global_element(name, member)?;
                model.add(
                    name.to_string(),
                    Prop {
                        schema,
                        min,
                        max,
                        attribute: false,
                    },
                );
            }
            return Ok(());
        }
        let Some(name) = node.attribute("name") else {
            return Err(error(node, "an element needs a name or a ref".to_string()));
        };
        let schema = self.element(node)?;
        model.add(
            name.to_string(),
            Prop {
                schema,
                min,
                max,
                attribute: false,
            },
        );
        Ok(())
    }

    /// The attributes (and attribute groups, and attribute wildcard)
    /// declared in `node`, into `model`.
    pub(super) fn attributes_of(
        &mut self,
        node: &'a Node,
        model: &mut Model,
    ) -> Result<(), SchemaError> {
        for child in node.xsd_children() {
            match child.name.as_str() {
                "attribute" => self.attribute(child, model)?,
                "attributeGroup" => {
                    let Some(name) = child.attribute("ref") else {
                        continue;
                    };
                    let (_, local) = child.resolve(name);
                    let Some(group) = self.attribute_groups.get(&local).copied() else {
                        return Err(error(
                            child,
                            format!("the attribute group \"{name}\" is not defined"),
                        ));
                    };
                    self.attributes_of(group, model)?;
                }
                "anyAttribute" => {
                    let namespace = child.attribute("namespace").unwrap_or("##any").trim();
                    if namespace == "##other" {
                        // Exact: an attribute of another namespace is
                        // written with a prefix.
                        model.other_attributes = true;
                    } else {
                        model.open = true;
                        self.lose(child, "xs:anyAttribute", LossEffect::Loosened, "xs:anyAttribute allows any other attribute here; its namespace is not checked".to_string());
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// One `xs:attribute` (a declaration or a reference).
    pub(super) fn attribute(
        &mut self,
        node: &'a Node,
        model: &mut Model,
    ) -> Result<(), SchemaError> {
        let declaration = match node.attribute("ref") {
            Some(name) => {
                let (namespace, local) = node.resolve(name);
                if namespace.as_deref() == Some("http://www.w3.org/XML/1998/namespace") {
                    // `xml:lang`, `xml:space`: keyed as written.
                    let key = format!("xml:{local}");
                    model.props.retain(|(known, _)| *known != key);
                    model.props.push((
                        key,
                        Prop {
                            schema: json!({"type": "string", "xml": {"attribute": true}}),
                            min: 0,
                            max: Some(1),
                            attribute: true,
                        },
                    ));
                    return Ok(());
                }
                match self.attributes.get(&local).copied() {
                    Some(declaration) => declaration,
                    None => {
                        return Err(error(
                            node,
                            format!("the attribute \"{name}\" is not declared"),
                        ));
                    }
                }
            }
            None => node,
        };
        let Some(name) = declaration.attribute("name") else {
            return Ok(());
        };
        let usage = node.attribute("use").unwrap_or("optional");
        if usage == "prohibited" {
            model
                .props
                .retain(|(key, prop)| !(prop.attribute && key == name));
            return Ok(());
        }
        let simple = if let Some(type_name) = declaration.attribute("type") {
            let (namespace, local) = declaration.resolve(type_name);
            if namespace.as_deref() == Some(XSD_NS) {
                self.built_in(declaration, &local)?
            } else {
                self.named_simple(&local, declaration)?
            }
        } else if let Some(inline) = declaration.child("simpleType") {
            self.simple_type(inline)?
        } else {
            Simple {
                kind: Kind::Any,
                schema: Map::new(),
            }
        };
        let mut schema = simple.schema.clone();
        let default = node
            .attribute("default")
            .or_else(|| declaration.attribute("default"));
        let fixed = node
            .attribute("fixed")
            .or_else(|| declaration.attribute("fixed"));
        if let Some(default) = default {
            schema.insert("default".to_string(), simple.typed(default));
        }
        if let Some(fixed) = fixed {
            schema.insert("const".to_string(), simple.typed(fixed));
        }
        if let Some(description) = node.documentation().or_else(|| declaration.documentation()) {
            schema.insert("description".to_string(), Value::String(description));
        }
        schema.insert("xml".to_string(), json!({"attribute": true}));
        let min = u64::from(usage == "required");
        model
            .props
            .retain(|(key, prop)| !(prop.attribute && key == name));
        model.props.push((
            name.to_string(),
            Prop {
                schema: Value::Object(schema),
                min,
                max: Some(1),
                attribute: true,
            },
        ));
        Ok(())
    }
}

/// `base` narrowed by `restricted` (a restriction's facets).
fn merge(base: Option<Simple>, restricted: Simple) -> Simple {
    match base {
        Some(mut base) => {
            base.schema.extend(restricted.schema);
            base
        }
        None => restricted,
    }
}
