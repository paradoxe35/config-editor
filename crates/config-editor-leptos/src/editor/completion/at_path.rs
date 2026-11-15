//! What a schema says of the value at one path, every subschema that
//! applies there combined: the base `properties` entry, `allOf` (through
//! `$ref`), the `then`/`else` the document's `if` takes, dependent schemas,
//! `patternProperties` and `additionalProperties` all apply, and one of a
//! `oneOf`/`anyOf`'s options does. A field the base `properties` only
//! describes (`workers: {description}`) is typed by the `allOf` beside it.

use super::conform::{listed, scalar_text};
use super::*;

/// The schemas that apply to one value: each of `All`, one of `Any`, and
/// a `Schema` as it is (its own combinators decided with it).
#[derive(Debug)]
pub(crate) enum AtPath<'a> {
    Schema(&'a Value),
    All(Vec<AtPath<'a>>),
    Any(Vec<AtPath<'a>>),
    /// An option of a walked `oneOf`/`anyOf` the object is not in (one of
    /// its properties lists values the object's is none of): one of the
    /// schemas there, which gives the object's children nothing.
    Refuted(Box<AtPath<'a>>),
}

impl<'a> AtPath<'a> {
    /// Whether every schema here allows `value` (`None` when one cannot
    /// tell, see [`decides`]).
    pub(crate) fn decides(&self, value: &Value, root: &Value) -> Option<bool> {
        match self {
            AtPath::Schema(schema) => decides(schema, value, root),
            AtPath::All(parts) => {
                let mut known = true;
                for part in parts {
                    match part.decides(value, root) {
                        Some(false) => return Some(false),
                        None => known = false,
                        Some(true) => {}
                    }
                }
                known.then_some(true)
            }
            AtPath::Any(options) => {
                let mut known = true;
                for option in options {
                    match option.decides(value, root) {
                        Some(true) => return Some(true),
                        None => known = false,
                        Some(false) => {}
                    }
                }
                // No option at all allows anything.
                (known || options.is_empty()).then_some(options.is_empty())
            }
            AtPath::Refuted(option) => option.decides(value, root),
        }
    }

    /// Every schema named here, once, in order.
    pub(crate) fn leaves(&self) -> Vec<&'a Value> {
        let mut leaves = Vec::new();
        self.collect(&mut leaves);
        leaves
    }

    fn collect(&self, leaves: &mut Vec<&'a Value>) {
        match self {
            AtPath::Schema(schema) => {
                if !leaves.iter().any(|known| std::ptr::eq(*known, *schema)) {
                    leaves.push(schema);
                }
            }
            AtPath::All(parts) | AtPath::Any(parts) => {
                parts.iter().for_each(|part| part.collect(leaves));
            }
            AtPath::Refuted(option) => option.collect(leaves),
        }
    }

    /// Every schema applying to `instance` (the value here, when the
    /// document gives it: it decides conditionals, dependent schemas and
    /// the options it is in), each schema's applicators walked once.
    pub(crate) fn applied(&self, instance: Option<&Value>, root: &'a Value) -> Applied<'a> {
        let mut walk = Walk {
            instance,
            root,
            ref_alone: refs_stand_alone(root),
            ancestors: Vec::new(),
            seen: HashSet::new(),
        };
        walk.of(self)
    }

    /// Whether nothing here says anything of the value.
    fn is_empty(&self) -> bool {
        match self {
            AtPath::Schema(_) => false,
            AtPath::All(parts) => parts.iter().all(AtPath::is_empty),
            AtPath::Any(options) => options.iter().all(AtPath::is_empty),
            AtPath::Refuted(option) => option.is_empty(),
        }
    }
}

/// The schemas applying to one value, their applicators walked: each
/// `Schema` is one schema object, whose own keywords (`properties`,
/// `items`) give its children.
#[derive(Debug)]
pub(crate) struct Applied<'a> {
    tree: AtPath<'a>,
}

impl<'a> Applied<'a> {
    /// Every schema applying, once, in order (what `branches_for` gives).
    pub(crate) fn branches(&self) -> Vec<&'a Value> {
        self.tree.leaves()
    }

    /// What applies to the child `segment` of the value.
    pub(crate) fn child(&self, segment: &SchemaPathSegment) -> AtPath<'a> {
        let found = children(&self.tree, segment);
        if !found.leaves().is_empty() {
            return found;
        }
        // What no schema evaluates is the `unevaluatedProperties` (or
        // `unevaluatedItems`) schema's.
        let keyword = match segment {
            SchemaPathSegment::Key(_) => "unevaluatedProperties",
            SchemaPathSegment::ArrayItem(_) => "unevaluatedItems",
        };
        all(self
            .branches()
            .into_iter()
            .filter_map(|branch| branch.get(keyword))
            .filter(|unevaluated| unevaluated.is_object())
            .map(AtPath::Schema)
            .collect())
    }
}

/// The child `segment`'s schemas in each schema of `tree`.
fn children<'a>(tree: &AtPath<'a>, segment: &SchemaPathSegment) -> AtPath<'a> {
    match tree {
        AtPath::Schema(schema) => all(own(schema, segment)
            .into_iter()
            .map(AtPath::Schema)
            .collect()),
        AtPath::All(parts) => all(parts.iter().map(|part| children(part, segment)).collect()),
        AtPath::Any(options) => any(options
            .iter()
            .filter(|option| !matches!(option, AtPath::Refuted(_)))
            .map(|option| children(option, segment))
            .collect()),
        AtPath::Refuted(_) => AtPath::All(Vec::new()),
    }
}

/// The child's schemas `schema` names itself: its property and the
/// patterns matching the key, else `additionalProperties`; an item's
/// tuple position or `items`.
fn own<'a>(schema: &'a Value, segment: &SchemaPathSegment) -> Vec<&'a Value> {
    let named = named_properties(schema, &segment.as_key());
    let item = segment
        .as_index()
        .and_then(|index| item_schema(schema, index));
    match segment {
        SchemaPathSegment::ArrayItem(_) => item.map_or(named, |item| vec![item]),
        SchemaPathSegment::Key(_) if !named.is_empty() => named,
        SchemaPathSegment::Key(_) => item
            .or_else(|| {
                schema
                    .get("additionalProperties")
                    .filter(|additional| additional.is_object())
            })
            .into_iter()
            .collect(),
    }
}

/// Nesting of applicators walked for one value, past which a schema says
/// nothing more (a reference cycle ends sooner, by the ancestors' check).
const MAX_NESTING: usize = 32;

/// The applicators of the schemas at one value walked.
struct Walk<'s, 'a> {
    instance: Option<&'s Value>,
    root: &'a Value,
    ref_alone: bool,
    /// The schemas being walked, so a reference cycle ends.
    ancestors: Vec<*const Value>,
    /// The schemas walked, each walked once: met again (two options naming
    /// one `$ref`) a schema is itself, its applicators not walked again, so
    /// shared references cost no more than once.
    seen: HashSet<*const Value>,
}

impl<'a> Walk<'_, 'a> {
    fn of(&mut self, at: &AtPath<'a>) -> Applied<'a> {
        let tree = match at {
            AtPath::Schema(schema) => {
                // One walk for each schema at the value (one met again is
                // not walked again).
                if !self.seen.contains(&std::ptr::from_ref(*schema)) {
                    crate::editor::work_count::note(crate::editor::work_count::Work::BranchWalk);
                }
                self.schema(schema)
            }
            AtPath::All(parts) => {
                AtPath::All(parts.iter().map(|part| self.of(part).tree).collect())
            }
            AtPath::Any(options) => {
                AtPath::Any(options.iter().map(|option| self.of(option).tree).collect())
            }
            AtPath::Refuted(option) => AtPath::Refuted(Box::new(self.of(option).tree)),
        };
        Applied { tree }
    }

    /// `schema` and every schema its applicators apply to the value.
    fn schema(&mut self, schema: &'a Value) -> AtPath<'a> {
        let pointer = std::ptr::from_ref(schema);
        if self.ancestors.len() > MAX_NESTING || self.ancestors.contains(&pointer) {
            return AtPath::All(Vec::new());
        }
        if !schema.is_object() || !self.seen.insert(pointer) {
            return AtPath::Schema(schema);
        }
        self.ancestors.push(pointer);
        let mut parts = Vec::new();
        if let Some(target) = schema
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|reference| ref_target(reference, self.root))
        {
            parts.push(self.schema(target));
        }
        if !(self.ref_alone && schema.get("$ref").is_some()) {
            parts.push(AtPath::Schema(schema));
            for option in array(schema, "allOf") {
                parts.push(self.schema(option));
            }
            for keyword in ["anyOf", "oneOf"] {
                let options: Vec<AtPath<'a>> = self
                    .options(schema, keyword)
                    .into_iter()
                    .map(|(option, refuted)| {
                        let walked = self.schema(option);
                        if refuted {
                            AtPath::Refuted(Box::new(walked))
                        } else {
                            walked
                        }
                    })
                    .collect();
                if !options.is_empty() {
                    parts.push(AtPath::Any(options));
                }
            }
            parts.extend(self.conditional(schema));
            for dependent in self.dependents(schema) {
                parts.push(self.schema(dependent));
            }
        }
        self.ancestors.pop();
        AtPath::All(parts)
    }

    /// The options of `schema`'s `keyword`, each with whether the object
    /// here is not in it (one of its properties lists values the object's
    /// is none of); in all while every option with properties is refuted.
    fn options(&self, schema: &'a Value, keyword: &str) -> Vec<(&'a Value, bool)> {
        let map = self.instance.and_then(Value::as_object);
        let options: Vec<(&'a Value, bool)> = array(schema, keyword)
            .map(|option| {
                let target = target(option, self.root);
                let refuted = map.is_some_and(|map| refuted(target, map, self.root));
                (option, refuted)
            })
            .collect();
        let all_gone = options.iter().all(|(option, refuted)| {
            *refuted || target(option, self.root).get("properties").is_none()
        });
        if all_gone {
            return options
                .into_iter()
                .map(|(option, _)| (option, false))
                .collect();
        }
        options
    }

    /// The `then`/`else` of `schema`'s `if` the value here takes, or
    /// either while it does not decide.
    fn conditional(&mut self, schema: &'a Value) -> Option<AtPath<'a>> {
        let condition = schema.get("if")?;
        let decided = self
            .instance
            .and_then(|instance| decides(condition, instance, self.root));
        let mut branch = |keyword: &str| {
            schema
                .get(keyword)
                .map_or(AtPath::All(Vec::new()), |branch| self.schema(branch))
        };
        Some(match decided {
            Some(true) => branch("then"),
            Some(false) => branch("else"),
            None => {
                let then = branch("then");
                AtPath::Any(vec![then, branch("else")])
            }
        })
    }

    /// The dependent schemas of `schema` whose key the object here has (all
    /// of them while it is unknown).
    fn dependents(&self, schema: &'a Value) -> Vec<&'a Value> {
        ["dependentSchemas", "dependencies"]
            .into_iter()
            .filter_map(|keyword| schema.get(keyword).and_then(Value::as_object))
            .flatten()
            .filter(|(_, dependent)| dependent.is_object() || dependent.is_boolean())
            .filter(|(key, _)| match self.instance {
                None => true,
                Some(Value::Object(members)) => members.contains_key(key.as_str()),
                Some(_) => false,
            })
            .map(|(_, dependent)| dependent)
            .collect()
    }
}

fn array<'a>(schema: &'a Value, keyword: &str) -> impl Iterator<Item = &'a Value> {
    schema
        .get(keyword)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

/// `parts` all applying, without those that say nothing.
fn all(parts: Vec<AtPath<'_>>) -> AtPath<'_> {
    let mut parts: Vec<AtPath<'_>> = parts.into_iter().filter(|part| !part.is_empty()).collect();
    if parts.len() == 1 {
        return parts.remove(0);
    }
    AtPath::All(parts)
}

/// One of `options` applying, read as those that say something of the
/// value: an option silent on a key (`required: [b]` beside one typing
/// `a`) leaves the value to the options typing it, as validation of the
/// object in one of those would.
fn any(options: Vec<AtPath<'_>>) -> AtPath<'_> {
    let mut options: Vec<AtPath<'_>> = options
        .into_iter()
        .filter(|option| !option.is_empty())
        .collect();
    if options.len() == 1 {
        return options.remove(0);
    }
    AtPath::Any(options)
}

/// The schema `schema`'s `$ref` names, else `schema`.
fn target<'a>(schema: &'a Value, root: &'a Value) -> &'a Value {
    schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|reference| ref_target(reference, root))
        .unwrap_or(schema)
}

/// Whether one of `branch`'s properties lists the values allowed and the
/// object's scalar there is none of them, also as text (XML's `1` is the
/// `"1"` listed).
fn refuted(branch: &Value, map: &serde_json::Map<String, Value>, root: &Value) -> bool {
    let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
        return false;
    };
    properties.iter().any(|(key, property)| {
        let Some(present) = map
            .get(key)
            .filter(|present| !present.is_object() && !present.is_array())
        else {
            return false;
        };
        let mut allowed = listed(target(property, root)).peekable();
        allowed.peek().is_some()
            && !allowed
                .any(|allowed| allowed == present || scalar_text(allowed) == scalar_text(present))
    })
}

/// [`AtPath`] of `path` in `document` from the root `schema`.
pub(crate) fn at_path<'a>(
    schema: &'a Value,
    path: &[SchemaPathSegment],
    document: &Value,
) -> AtPath<'a> {
    let mut at = AtPath::Schema(schema);
    let mut instance = Some(document);
    for segment in path {
        at = at.applied(instance, schema).child(segment);
        instance = instance.and_then(|value| match segment {
            SchemaPathSegment::Key(key) => value.get(key),
            SchemaPathSegment::ArrayItem(index) => value.get(index),
        });
    }
    at
}
