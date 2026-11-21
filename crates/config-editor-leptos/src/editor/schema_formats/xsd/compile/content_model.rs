//! A complex type's content model for the content check (`x-xsd-content`,
//! see `checks`): its particles as the automaton's, resolved through group
//! references, element references with their substitution groups, and the
//! bases of extensions; and, where a wildcard leaves the JSON Schema open,
//! its attributes.

use serde_json::{Value, json};

use super::super::super::LossEffect;
use super::super::checks::CONTENT;
use super::super::checks::automaton::{Automaton, Namespaces, Occurs, Particle};
use super::super::tree::{Node, XSD_NS};
use super::{Compiler, Model, occurs};

/// The particles a model group holds.
const PARTICLES: [&str; 6] = ["element", "sequence", "choice", "all", "group", "any"];

impl<'a> Compiler<'a> {
    /// Writes the content model of complex type `node` (whose `model` was
    /// read) into its `schema`.
    pub(super) fn annotate_content(&mut self, node: &'a Node, model: &Model, schema: &mut Value) {
        if model.text.is_some() {
            // Simple content: no child elements.
            return;
        }
        let outer = std::mem::take(&mut self.grouping);
        let content = self.content_model(node, 0);
        self.grouping = outer;
        let content = content.unwrap_or_else(|| Occurs::once(Particle::Sequence(Vec::new())));
        if content.counts_past_unrolled() {
            self.lose_once(
                node,
                "occurrence count",
                LossEffect::Loosened,
                format!(
                    "a minOccurs or maxOccurs above {} in a content model is checked as unbounded",
                    super::super::checks::automaton::MAX_UNROLLED
                ),
            );
        }
        if Automaton::new(&content).is_none() {
            self.lose(
                node,
                "content model",
                LossEffect::Loosened,
                "this content model is too large to check its order and counts".to_string(),
            );
            return;
        }
        let shared = model.props.iter().find(|(name, prop)| {
            prop.attribute
                && model
                    .props
                    .iter()
                    .any(|(other, prop)| !prop.attribute && other == name)
        });
        if let Some((name, _)) = shared {
            let message = format!(
                "an attribute and a child element both named \"{name}\": in another language a value under that name may be either, and is taken as whichever the content model allows"
            );
            self.lose_once(
                node,
                "attribute named as an element",
                LossEffect::Approximated,
                message,
            );
        }
        if content.orders() {
            let at = ordering_sequence(node).unwrap_or(node);
            self.lose_once(
                at,
                "xs:sequence order",
                LossEffect::NotChecked,
                "the order of an xs:sequence's elements is checked in an XML text only: in another language the children are taken in the model's order (TOML and HCL write tables after values)".to_string(),
            );
        }
        let mut annotation = json!({ "model": content.to_json() });
        if let Some(namespace) = self.target_namespace(node) {
            annotation["namespace"] = Value::String(namespace);
        }
        if model.open && !self.settings.lax {
            // Open for a wildcard: the attributes are checked here.
            let attributes: Vec<&str> = model
                .props
                .iter()
                .filter(|(_, prop)| prop.attribute)
                .map(|(name, _)| name.as_str())
                .collect();
            annotation["attributes"] = json!(attributes);
            if let Some(namespaces) = &model.any_attribute {
                annotation["anyAttribute"] = namespaces.to_json();
            }
        }
        if self.settings.lax {
            annotation["lax"] = Value::Bool(true);
        }
        if let Value::Object(map) = schema {
            map.insert(CONTENT.to_string(), annotation);
            self.content_checked = true;
        }
    }

    /// The content model of complex type `node` (`None`: no particles).
    fn content_model(&mut self, node: &'a Node, depth: usize) -> Option<Occurs> {
        if depth > 64 {
            return None;
        }
        if let Some(content) = node.child("complexContent") {
            let derivation = content
                .child("extension")
                .or_else(|| content.child("restriction"))?;
            let mine = self.own_particle(derivation, depth);
            if derivation.name == "restriction" {
                return mine;
            }
            let (namespace, local) =
                derivation.resolve(derivation.attribute("base").unwrap_or_default());
            let base = if namespace.as_deref() == Some(XSD_NS) {
                None
            } else {
                let base_node = if self.modeling_content.contains(&local) {
                    self.original("complexType", &local)
                } else {
                    self.complex_types.get(&local).copied()
                };
                base_node.and_then(|base_node| {
                    self.modeling_content.push(local.clone());
                    let model = self.content_model(base_node, depth + 1);
                    self.modeling_content.pop();
                    model
                })
            };
            return match (base, mine) {
                (Some(base), Some(mine)) => {
                    if [&base, &mine]
                        .iter()
                        .any(|part| matches!(part.particle, Particle::All(_)))
                    {
                        self.lose(
                            derivation,
                            "xs:all",
                            LossEffect::Loosened,
                            "an xs:all extended (or extending) is checked without its counts"
                                .to_string(),
                        );
                    }
                    Some(Occurs::once(Particle::Sequence(vec![base, mine])))
                }
                (base, mine) => base.or(mine),
            };
        }
        self.own_particle(node, depth)
    }

    /// The model group (or group reference) `holder` holds.
    fn own_particle(&mut self, holder: &'a Node, depth: usize) -> Option<Occurs> {
        let particle = holder
            .xsd_children()
            .find(|child| matches!(child.name.as_str(), "sequence" | "choice" | "all" | "group"))?;
        self.particle_model(particle, depth + 1)
    }

    /// The particle `node` (an element, model group, group reference or
    /// wildcard) as the automaton's.
    fn particle_model(&mut self, node: &'a Node, depth: usize) -> Option<Occurs> {
        if depth > 64 {
            return None;
        }
        let (min, max) = occurs(node).ok()?;
        let particle = match node.name.as_str() {
            "element" => match node.attribute("ref") {
                Some(reference) => self.element_reference(reference, node)?,
                None => Particle::Element(node.attribute("name")?.to_string()),
            },
            "any" => Particle::Any(self.wildcard_namespaces(node)),
            "sequence" | "choice" | "all" => {
                let items: Vec<Occurs> = node
                    .xsd_children()
                    .filter(|child| PARTICLES.contains(&child.name.as_str()))
                    .filter_map(|child| self.particle_model(child, depth + 1))
                    .collect();
                match node.name.as_str() {
                    "sequence" => Particle::Sequence(items),
                    "choice" => Particle::Choice(items),
                    _ => Particle::All(items),
                }
            }
            "group" => {
                let (_, local) = node.resolve(node.attribute("ref")?);
                let group = if self.grouping.contains(&local) {
                    self.original("group", &local)?
                } else {
                    self.groups.get(&local).copied()?
                };
                self.grouping.push(local);
                let inner = group
                    .xsd_children()
                    .find(|child| matches!(child.name.as_str(), "sequence" | "choice" | "all"))
                    .and_then(|child| self.particle_model(child, depth + 1));
                self.grouping.pop();
                Particle::Sequence(vec![inner?])
            }
            _ => return None,
        };
        Some(Occurs { particle, min, max })
    }

    /// A reference to global element `reference`: it, or any member of its
    /// substitution group (abstract ones left out).
    fn element_reference(&self, reference: &str, at: &Node) -> Option<Particle> {
        let (_, local) = at.resolve(reference);
        let mut names: Vec<String> = Vec::new();
        if self
            .elements
            .get(&local)
            .is_some_and(|head| head.attribute("abstract") != Some("true"))
        {
            names.push(local.clone());
        }
        let mut heads = vec![local];
        while let Some(head) = heads.pop() {
            for member in self.substitutions.get(&head).into_iter().flatten() {
                let Some(name) = member.attribute("name") else {
                    continue;
                };
                if names.iter().any(|known| known == name)
                    || heads.iter().any(|known| known == name)
                {
                    continue;
                }
                // A member's own members substitute for the head too.
                heads.push(name.to_string());
                if member.attribute("abstract") != Some("true") {
                    names.push(name.to_string());
                }
            }
        }
        match names.len() {
            // Nothing can stand here.
            0 => Some(Particle::Choice(Vec::new())),
            1 => Some(Particle::Element(names.remove(0))),
            _ => Some(Particle::Choice(
                names
                    .into_iter()
                    .map(|name| Occurs::once(Particle::Element(name)))
                    .collect(),
            )),
        }
    }

    /// The namespaces wildcard `node` (`xs:any`, `xs:anyAttribute`)
    /// allows.
    pub(super) fn wildcard_namespaces(&self, node: &Node) -> Namespaces {
        let target = self.target_namespace(node);
        let written = node.attribute("namespace").unwrap_or("##any").trim();
        match written {
            "##any" => Namespaces::Any,
            "##other" => Namespaces::Not(target),
            list => Namespaces::In(
                list.split_whitespace()
                    .map(|namespace| match namespace {
                        "##local" => None,
                        "##targetNamespace" => target.clone(),
                        other => Some(other.to_string()),
                    })
                    .collect(),
            ),
        }
    }
}

/// The first `xs:sequence` under `node` holding more than one particle:
/// where an order is written.
fn ordering_sequence(node: &Node) -> Option<&Node> {
    node.xsd_children().find_map(|child| {
        let particles = child
            .xsd_children()
            .filter(|particle| PARTICLES.contains(&particle.name.as_str()))
            .count();
        if child.name == "sequence" && particles > 1 {
            Some(child)
        } else {
            ordering_sequence(child)
        }
    })
}
