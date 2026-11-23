//! What a schema says applies at one path of one document: every schema
//! that applies there combined. All of them: the base `properties` entry,
//! `allOf` (through `$ref`), the `then`/`else` the document's `if` takes,
//! dependent schemas, `patternProperties`, `additionalProperties`, the
//! item schemas; and one of a `oneOf`/`anyOf`'s options, narrowed by the
//! document where it tells which. Every reader of a schema (suggestions,
//! hover, XML typing, the quick check, fills, locks) asks this module, so
//! they agree on every shape.
//!
//! An [`Entry`] is what applies to a value before the applicators of its
//! schemas are walked; a [`Here`] the same walked for the value. A walk
//! takes each schema once (shared where met again), and an [`Applies`]
//! session keeps each step of a path, so a path costs one walk per step.

mod accepts;
mod choices;
mod document;
mod expand;
mod list_rules;
mod narrow;
mod node;
mod query;
mod refusing;
mod step;
mod types;

pub(crate) use accepts::decides;
pub(crate) use choices::{ChoiceHover, Choices};
pub(crate) use document::{Next, Walking, each_value};
pub(crate) use list_rules::{Contains, ListRules};
pub(crate) use narrow::{listed, scalar_text};
pub(crate) use node::{Entry, Walked};
pub(crate) use types::Types;

use crate::editor::language::SchemaPathSegment;
use expand::Expand;
use node::Tree;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

/// What is known of the value schemas are walked for.
#[derive(Clone, Copy, Default)]
pub(crate) struct Seen<'i> {
    /// The value, where the document gives it.
    pub(crate) value: Option<&'i Value>,
    /// The keys the text has, for an object whose value the document does
    /// not give (a key being typed into a text that does not parse): the
    /// value is an object with them.
    pub(crate) keys: Option<&'i HashSet<String>>,
}

impl<'i> Seen<'i> {
    pub(crate) fn value(value: Option<&'i Value>) -> Self {
        Self { value, keys: None }
    }

    /// Whether the object has `key` (true while that is unknown).
    fn has_key(&self, key: &str) -> bool {
        match (self.value, self.keys) {
            (Some(Value::Object(members)), _) if members.contains_key(key) => true,
            (_, Some(keys)) => keys.contains(key),
            (Some(_), None) => false,
            (None, None) => true,
        }
    }
}

/// What applies to one value, its applicators walked.
#[derive(Clone, Debug)]
pub(crate) struct Here<'s> {
    root: &'s Value,
    entry: Entry<'s>,
    tree: Tree<'s>,
}

impl<'s> Here<'s> {
    /// What applies to a value of `schema` alone, the value unknown.
    pub(crate) fn of(schema: &'s Value, root: &'s Value) -> Self {
        Self::walk(Entry::schema(schema), root, Seen::default())
    }

    /// `entry` walked for the value `seen`.
    pub(crate) fn walk(entry: Entry<'s>, root: &'s Value, seen: Seen<'_>) -> Self {
        let ref_alone = refs_stand_alone(root);
        let tree = Expand::new(root, ref_alone, seen).entry(&entry);
        Self { root, entry, tree }
    }

    /// [`Self::walk`] where the value is read (`value`) only when
    /// something walked is decided by it: most schemas have nothing so.
    fn walk_read<'i>(entry: Entry<'s>, root: &'s Value, value: &dyn Fn() -> Seen<'i>) -> Self {
        let ref_alone = refs_stand_alone(root);
        let mut blind = Expand::new(root, ref_alone, Seen::default());
        let tree = blind.entry(&entry);
        let tree = if blind.decidable {
            let seen = value();
            if seen.value.is_some() || seen.keys.is_some() {
                Expand::new(root, ref_alone, seen).entry(&entry)
            } else {
                tree
            }
        } else {
            tree
        };
        Self { root, entry, tree }
    }

    /// The same schemas walked again for the value `seen`.
    pub(crate) fn rewalk(&self, seen: Seen<'_>) -> Self {
        Self::walk(self.entry.clone(), self.root, seen)
    }

    pub(crate) fn root(&self) -> &'s Value {
        self.root
    }

    /// What applied before the walk: the schemas the path leads to.
    pub(crate) fn entry(&self) -> &Entry<'s> {
        &self.entry
    }

    /// The schemas the path leads to (each standing for all it applies).
    pub(crate) fn schemas(&self) -> Vec<&'s Value> {
        self.entry.schemas()
    }

    /// Every schema applying, once, in order: each option of a
    /// `oneOf`/`anyOf`, both branches of an `if` not decided.
    pub(crate) fn branches(&self) -> Vec<&'s Value> {
        query::leaves(&self.tree, Walked::Any)
    }

    /// The schemas certain to apply: not a branch of an `if` the value
    /// does not decide, nor an option it is not known to be in.
    pub(crate) fn certain(&self) -> Vec<&'s Value> {
        query::leaves(&self.tree, Walked::Certain)
    }

    /// The schemas the value may still be read by: no option it is not
    /// in, only the matched one of options where one is.
    pub(crate) fn offered(&self) -> Vec<&'s Value> {
        query::leaves(&self.tree, Walked::Offered)
    }

    /// The kinds of value everything here allows, combined as the schemas
    /// combine (see [`Types`]).
    pub(crate) fn types(&self) -> Types {
        types::types(&self.tree)
    }

    /// Whether some value of the kind `name` (`object` or `array`) may be
    /// taken here: the kinds allow it, and its empty value is not refused,
    /// or is refused only as too few entries (a `required` key, a
    /// `minProperties`; a `minItems`, a `contains`) where entries may be
    /// written: for a list, where the counts its rules ask fit together
    /// and an item at least is allowed ([`Self::list_rules`]). A list two
    /// `oneOf` options both take meets nothing.
    pub(crate) fn may_hold(&self, name: &str) -> bool {
        if !self.types().allows_type(name) {
            return false;
        }
        let empty = match name {
            "object" => Value::Object(serde_json::Map::new()),
            "array" => Value::Array(Vec::new()),
            _ => return true,
        };
        if self.accepts(&empty) != Some(false) {
            return true;
        }
        let asked = |leaf: &&Value| match name {
            "object" => {
                leaf.get("required")
                    .and_then(Value::as_array)
                    .is_some_and(|keys| !keys.is_empty())
                    || leaf.get("minProperties").and_then(Value::as_u64) > Some(0)
            }
            _ => {
                leaf.get("minItems").and_then(Value::as_u64) > Some(0)
                    || leaf.get("contains").is_some()
            }
        };
        if !self.branches().iter().any(asked) {
            return false;
        }
        name == "object" || {
            let rules = self.list_rules();
            rules.counts_fit() && rules.most() != Some(0)
        }
    }

    /// The list rules of every schema certain to apply here, merged
    /// ([`ListRules`]); where none of them says anything of a list, those
    /// of the first schema that does (the option a list is written for).
    pub(crate) fn list_rules(&self) -> ListRules<'s> {
        let mut schemas = self.certain();
        let lists = |schema: &&'s Value| !ListRules::of(&[*schema]).is_empty();
        if !schemas.iter().any(lists) {
            schemas.extend(self.branches().into_iter().find(lists));
        }
        ListRules::of(&schemas)
    }

    /// Whether the schemas here judge a child together with the value
    /// holding it, so that only a check of the whole value tells which
    /// reading of the child they take: an option of a `oneOf`/`anyOf` is
    /// taken by the whole value, a `not` or an `if` reads it whole, and a
    /// `contains` or `uniqueItems` reads every item, an `enum` or `const`
    /// the whole value. Every other keyword says of a child only what the
    /// schemas the path leads to there say.
    pub(crate) fn judges_children_together(&self) -> bool {
        const TOGETHER: [&str; 8] = [
            "oneOf",
            "anyOf",
            "not",
            "if",
            "contains",
            "uniqueItems",
            "enum",
            "const",
        ];
        self.branches().iter().any(|schema| {
            schema.as_object().is_some_and(|members| {
                TOGETHER
                    .iter()
                    .any(|keyword| members.contains_key(*keyword))
            })
        })
    }

    /// Whether the value refutes an option of a `oneOf`/`anyOf` here.
    pub(crate) fn refutes(&self) -> bool {
        query::refutes(&self.tree)
    }

    /// The `oneOf`/`anyOf` options of the object here.
    pub(crate) fn choices(&self) -> Choices<'s> {
        Choices::of(&self.tree)
    }

    /// The schemas documenting the value here: every one applying, but
    /// those of an option of its object other than the one it is in.
    pub(crate) fn documented(&self) -> Vec<&'s Value> {
        let choices = self.choices();
        self.branches()
            .into_iter()
            .filter(|branch| !matches!(choices.of_branch(branch), Some((_, false))))
            .collect()
    }

    /// The schemas documenting the value itself: every one applying
    /// outside its object's options (an option's title and description
    /// are the option's, see [`Choices::hover`]).
    pub(crate) fn own(&self) -> Vec<&'s Value> {
        let choices = self.choices();
        self.branches()
            .into_iter()
            .filter(|branch| choices.of_branch(branch).is_none())
            .collect()
    }

    /// Whether every schema here allows `value` (`None` when one cannot
    /// tell, see [`decides`]).
    pub(crate) fn accepts(&self, value: &Value) -> Option<bool> {
        self.entry.accepts(value, self.root)
    }

    /// What applies to the child `segment`, before its walk.
    pub(crate) fn child(&self, segment: &SchemaPathSegment) -> Entry<'s> {
        step::child(&self.entry, &self.tree, segment)
    }

    /// [`Self::child`] for reading the child's type: by every option the
    /// value may be in, not one it only seems to fit; an item where the
    /// schemas name none is nothing (a value read as its own one item
    /// would be read again inside itself).
    pub(crate) fn child_typed(&self, segment: &SchemaPathSegment) -> Entry<'s> {
        step::named(&self.tree, segment, false)
    }

    /// What applies to the children no `properties` entry names: each
    /// item and each entry (see [`step::unnamed`]).
    #[cfg(feature = "protect")]
    pub(crate) fn unnamed_children(&self) -> Entry<'s> {
        step::unnamed(&self.tree)
    }

    /// [`Self::child`] as the schemas name it: an item where they name
    /// none is nothing (not the value read as its own one item).
    pub(crate) fn child_named(&self, segment: &SchemaPathSegment) -> Entry<'s> {
        step::child_only(&self.tree, segment)
    }
}

/// One reading of a schema against one document: what applies at each
/// path asked, each step of a path walked once for the session.
pub(crate) struct Applies<'s, 'd> {
    root: &'s Value,
    /// With it a key naming an array is one of its items as well (each XML
    /// `<dns>` element is one entry of `dns`): the language writes lists by
    /// repeating keys (`data_reading::lone_items`).
    repeats_arrays: bool,
    /// The document, read only at a step it decides (a parse of the
    /// whole text, which most steps of most paths need none of).
    document: &'d dyn Fn() -> Option<&'d Value>,
    /// Whether an option a value only fits is taken as its own (not when
    /// reading values' types, see [`Here::child_typed`]).
    trust_fit: bool,
    /// The paths walked, as a tree of their steps: a path is found one
    /// step at a time, so a deep path costs its depth, not its depth
    /// squared (as keying each prefix by its whole path would).
    steps: RefCell<Vec<Step<'s>>>,
}

/// One path walked: what applies there, and the steps walked below it
/// (indexes into [`Applies::steps`]).
struct Step<'s> {
    here: Rc<Here<'s>>,
    children: HashMap<SchemaPathSegment, usize>,
}

fn no_document<'d>() -> Option<&'d Value> {
    None
}

impl<'s, 'd> Applies<'s, 'd> {
    pub(crate) fn new(
        root: &'s Value,
        repeats_arrays: bool,
        document: &'d dyn Fn() -> Option<&'d Value>,
    ) -> Self {
        Self {
            root,
            repeats_arrays,
            document,
            trust_fit: true,
            steps: RefCell::new(Vec::new()),
        }
    }

    /// [`Self::new`] for reading the types of the document's values: each
    /// by every option its object may be in, not one it only seems to fit.
    pub(crate) fn for_types(
        root: &'s Value,
        repeats_arrays: bool,
        document: &'d dyn Fn() -> Option<&'d Value>,
    ) -> Self {
        Self {
            trust_fit: false,
            ..Self::new(root, repeats_arrays, document)
        }
    }

    /// A session without a document: what the schema says whatever is
    /// written.
    pub(crate) fn blind(root: &'s Value, repeats_arrays: bool) -> Self {
        Self::new(root, repeats_arrays, &no_document)
    }

    pub(crate) fn root(&self) -> &'s Value {
        self.root
    }

    /// The document's value at `path`.
    pub(crate) fn value_at(&self, path: &[SchemaPathSegment]) -> Option<&'d Value> {
        (self.document)().and_then(|document| value_at(document, path))
    }

    /// What applies at `path`, walked for the document's value there.
    pub(crate) fn at(&self, path: &[SchemaPathSegment]) -> Rc<Here<'s>> {
        if self.steps.borrow().is_empty() {
            let here = Here::walk_read(Entry::schema(self.root), self.root, &|| {
                Seen::value(self.value_at(&[]))
            });
            self.steps.borrow_mut().push(Step {
                here: Rc::new(here),
                children: HashMap::new(),
            });
        }
        let mut node = 0;
        for (index, segment) in path.iter().enumerate() {
            let known = self.steps.borrow()[node].children.get(segment).copied();
            node = match known {
                Some(child) => child,
                None => {
                    let parent_here = self.steps.borrow()[node].here.clone();
                    let at = &path[..=index];
                    let entry = self.step(&parent_here, segment, at);
                    let here =
                        Here::walk_read(entry, self.root, &|| Seen::value(self.value_at(at)));
                    let mut steps = self.steps.borrow_mut();
                    let child = steps.len();
                    steps.push(Step {
                        here: Rc::new(here),
                        children: HashMap::new(),
                    });
                    steps[node].children.insert(segment.clone(), child);
                    child
                }
            };
        }
        self.steps.borrow()[node].here.clone()
    }

    /// [`Self::at`] for an object with `keys` (those its text has): its
    /// options narrowed by them where the document does not give it.
    pub(crate) fn at_object(&self, path: &[SchemaPathSegment], keys: &HashSet<String>) -> Here<'s> {
        let here = self.at(path);
        let empty = Value::Object(serde_json::Map::new());
        let value = self
            .value_at(path)
            .map(|value| if value.is_null() { &empty } else { value });
        here.rewalk(Seen {
            value,
            keys: Some(keys),
        })
    }

    /// The child `segment` of `parent`, at `path`: with `repeats_arrays`
    /// a key naming an array is one of its items too.
    fn step(
        &self,
        parent: &Here<'s>,
        segment: &SchemaPathSegment,
        path: &[SchemaPathSegment],
    ) -> Entry<'s> {
        let next = if self.trust_fit {
            parent.child(segment)
        } else {
            parent.child_typed(segment)
        };
        if !self.repeats_arrays || !matches!(segment, SchemaPathSegment::Key(_)) || next.is_empty()
        {
            return next;
        }
        let walked = Here::walk_read(next.clone(), self.root, &|| {
            Seen::value(self.value_at(path))
        });
        with_items(next, &walked)
    }
}

/// `next` (what applies under a key, `walked` for its value) or, where it
/// is an array, one of its items: a language that writes a list by
/// repeating its key writes a one-item list as the item alone.
fn with_items<'s>(next: Entry<'s>, walked: &Here<'s>) -> Entry<'s> {
    let items = step::child_only(&walked.tree, &SchemaPathSegment::ArrayItem(0));
    if items.is_empty() {
        return next;
    }
    Entry(node::any(vec![next.0, items.0]))
}

/// The value at `path` in a parsed document: a step written as an index
/// names an item of an array and the key of its digits in an object. An
/// item of something the document has as one value (an XML element written
/// once) is that value.
pub(crate) fn value_at<'v>(document: &'v Value, path: &[SchemaPathSegment]) -> Option<&'v Value> {
    path.iter()
        .try_fold(document, |value, segment| match value {
            Value::Array(items) => items.get(segment.as_index()?),
            Value::Object(members) => members
                .get(&*segment.as_key())
                .or_else(|| (*segment == SchemaPathSegment::ArrayItem(0)).then_some(value)),
            _ => (*segment == SchemaPathSegment::ArrayItem(0)).then_some(value),
        })
}

/// The schema a `$ref` names in `root`: a `#` JSON pointer, its tokens
/// percent-decoded. (References by `$id` or `$anchor` are rewritten to
/// pointers when the schema is built, see `ui_refs`.)
pub(crate) fn ref_target<'a>(reference: &str, root: &'a Value) -> Option<&'a Value> {
    let pointer = reference.strip_prefix('#')?;
    root.pointer(pointer)
        .or_else(|| root.pointer(&crate::editor::ui_refs::percent_decoded(pointer)))
}

/// Whether the keywords beside a `$ref` are ignored: up to draft-07, which
/// the root's `$schema` names.
pub(crate) fn refs_stand_alone(root: &Value) -> bool {
    root.get("$schema")
        .and_then(Value::as_str)
        .is_some_and(|dialect| {
            ["draft-03", "draft-04", "draft-06", "draft-07"]
                .iter()
                .any(|draft| dialect.contains(draft))
        })
}

pub(crate) fn type_includes(schema: &Value, expected: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(schema_type)) => schema_type == expected,
        Some(Value::Array(types)) => types.iter().any(|schema_type| schema_type == expected),
        _ => false,
    }
}

/// The schema of item `index` of an array of `schema`: its tuple position
/// (`prefixItems`, or up to draft-07 an `items` list), else the schema of
/// the other items (`items`, or after a draft-07 tuple `additionalItems`).
pub(crate) fn item_schema(schema: &Value, index: usize) -> Option<&Value> {
    let tuple = |keyword: &str| schema.get(keyword).and_then(Value::as_array);
    if let Some(positions) = tuple("prefixItems") {
        return positions
            .get(index)
            .or_else(|| schema.get("items").filter(|items| items.is_object()));
    }
    if let Some(positions) = tuple("items") {
        return positions.get(index).or_else(|| {
            schema
                .get("additionalItems")
                .filter(|items| items.is_object())
        });
    }
    schema.get("items").filter(|items| items.is_object())
}

/// The schemas `schema` gives the key `key` by name: its property and the
/// patterns matching it.
pub(crate) fn named_properties<'a>(schema: &'a Value, key: &str) -> Vec<&'a Value> {
    let property = schema
        .get("properties")
        .and_then(|properties| properties.get(key));
    let patterns = schema
        .get("patternProperties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(pattern, _)| crate::editor::completion::pattern_matches(pattern, key))
        .map(|(_, property)| property);
    property.into_iter().chain(patterns).collect()
}

#[cfg(all(test, feature = "yaml"))]
mod decided_tests;
#[cfg(test)]
mod tests;
