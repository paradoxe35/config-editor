//! Reading a document the way the schema expects, for languages that write
//! arrays by repeating their key (a lone value where only a list is allowed
//! is a one-item list) and XML, whose values are all text: its text is read
//! as the type of an option it fits (`<workers>4</workers>` is the number
//! for "an integer or `auto`", `<v>1.3</v>` the string a `const` lists).
//! Each node read otherwise than parsed is noted in the [`DataReading`].

use super::DataReading;
use super::text::{other_reading, text_literal};
use crate::editor::applies::{Entry, Here, Seen, Types, listed, scalar_text, type_includes};
use crate::editor::completion::describes_array;
use crate::editor::language::{Repetition, SchemaPathSegment, pointer_key};
use serde_json::Value;

/// `value` read the way `schema` expects for a language that writes arrays
/// by repeating their key: each lone value (or, for blocks, lone object)
/// where only an array is allowed becomes a one-item array (a single XML
/// `<dns>` element is `dns: [...]`), and XML text is typed by the schema.
pub(super) fn conform(value: Value, schema: &Value, repetition: Repetition) -> DataReading {
    conform_at(value, &Entry::schema(schema), schema, repetition)
}

/// [`conform`] where `at` is what applies to `value` (every subschema at
/// its path combined).
pub(super) fn conform_at(
    mut value: Value,
    at: &Entry<'_>,
    root: &Value,
    repetition: Repetition,
) -> DataReading {
    let mut reading = DataReading::default();
    let mut pointer = String::new();
    let mut node = NodeAt {
        root,
        repetition,
        pointer: &mut pointer,
        reading: &mut reading,
    };
    node.read(&mut value, at, 0);
    reading.value = value;
    let DataReading { value, retyped, .. } = &mut reading;
    retyped.retain(|pointer, parsed| value.pointer(pointer).is_some_and(|read| read != parsed));
    reading
}

/// One reading pass: the schema, what the language repeats, the pointer of
/// the node read and what is noted so far.
struct NodeAt<'p, 'r> {
    root: &'r Value,
    repetition: Repetition,
    pointer: &'p mut String,
    reading: &'p mut DataReading,
}

impl NodeAt<'_, '_> {
    /// Reads `value` (at `self.pointer`) as `at` allows, then its children.
    fn read(&mut self, value: &mut Value, at: &Entry<'_>, depth: usize) {
        if depth > 64 || at.is_empty() {
            return;
        }
        let (root, repetition) = (self.root, self.repetition);
        // An object decides the conditionals it is under (`if kind is a`).
        let applied = Here::walk(
            at.clone(),
            root,
            Seen::value(value.is_object().then_some(&*value)),
        );
        let branches = applied.branches();
        let types = applied.types();
        if repetition == Repetition::Elements {
            let parsed = (!is_filled(value)).then(|| value.clone());
            read_text(value, at, &branches, types, root);
            if let Some(parsed) = parsed.filter(|parsed| parsed != value) {
                self.note_retyped(self.pointer.clone(), parsed);
            }
        }
        let repeatable = match repetition {
            Repetition::Elements => !value.is_array(),
            Repetition::Blocks => value.is_object(),
            _ => false,
        };
        // A lone value the schemas take as it is (list keywords with no
        // `type` say nothing of a value that is no list) stays itself; one
        // they refuse, where they name the list type, is the one item of a
        // list they take (an option naming it beside one taking anything; an
        // option an object's keys refute, the list it reads as taking it).
        let names_list = |types: Types, branches: &[&Value]| {
            types != Types::ANY || branches.iter().any(|branch| type_includes(branch, "array"))
        };
        let item = repeatable
            && if types.allows_type("array") {
                at.accepts(value, root) != Some(true)
                    && (!types.allows(value)
                        || (names_list(types, &branches)
                            && at.accepts(&Value::Array(vec![value.clone()]), root) == Some(true)))
            } else {
                value.is_object()
                    && applied.refutes()
                    && at.accepts_near(value, root) == Some(false)
                    && at.accepts_near(&Value::Array(vec![value.clone()]), root) == Some(true)
                    && {
                        let blind = Here::walk(at.clone(), root, Seen::default());
                        blind.types().allows_type("array")
                            && names_list(blind.types(), &blind.branches())
                    }
            };
        if item {
            *value = Value::Array(vec![value.take()]);
            let pointer = self.pointer.clone();
            self.reading.list_of_one_at(&pointer);
        } else if repetition == Repetition::Elements
            && value.is_object()
            && applied.refutes()
            && at.accepts_near(value, root) == Some(false)
            && {
                // The options the object refutes may be the list's.
                let blind = Here::walk(at.clone(), root, Seen::default());
                blind.types().allows_type("array") && names_list(blind.types(), &blind.branches())
            }
        {
            self.read_as_item(value, at);
        }
        // Each child read with every schema that applies to it there.
        let segments: Vec<SchemaPathSegment> = match &*value {
            Value::Array(items) => (0..items.len()).map(SchemaPathSegment::ArrayItem).collect(),
            Value::Object(map) => map.keys().cloned().map(SchemaPathSegment::Key).collect(),
            _ => return,
        };
        let children: Vec<Entry<'_>> = segments
            .iter()
            .map(|segment| applied.child_typed(segment))
            .collect();
        let length = self.pointer.len();
        match value {
            Value::Array(items) => {
                for (index, (item, at)) in items.iter_mut().zip(&children).enumerate() {
                    self.pointer.push('/');
                    self.pointer.push_str(&index.to_string());
                    self.read(item, at, depth + 1);
                    self.pointer.truncate(length);
                }
            }
            Value::Object(map) => {
                for ((key, child), at) in map.iter_mut().zip(&children) {
                    self.pointer.push('/');
                    self.pointer.push_str(&pointer_key(key));
                    self.read(child, at, depth + 1);
                    self.pointer.truncate(length);
                }
            }
            _ => {}
        }
        if repetition == Repetition::Elements {
            self.reread_for_object(value, at, &children);
        }
    }

    /// `value` (a lone XML object the schemas at it refuse, whose one-item
    /// list they do not take as parsed) made a list's one item where the
    /// schemas take that list once the item's text is read as the item's
    /// schemas want it (`<c>true</c>` as the text an item's `c` asks), the
    /// text so read noted. Only the item's own scalars are read here (its
    /// children are read as any list's item after): the trial costs the
    /// object's members, not what they hold.
    fn read_as_item(&mut self, value: &mut Value, at: &Entry<'_>) {
        let root = self.root;
        let mut item = value.clone();
        let list = Here::walk(at.clone(), root, Seen::default());
        let item_at = list.child_typed(&SchemaPathSegment::ArrayItem(0));
        let here = Here::walk(item_at, root, Seen::value(Some(&item)));
        let mut read = Vec::new();
        if let Value::Object(map) = &mut item {
            for (key, child) in map.iter_mut() {
                if is_filled(child) {
                    continue;
                }
                let child_at = here.child_typed(&SchemaPathSegment::Key(key.clone()));
                let applied = Here::walk(child_at.clone(), root, Seen::default());
                let parsed = child.clone();
                read_text(child, &child_at, &applied.branches(), applied.types(), root);
                if *child != parsed {
                    read.push((pointer_key(key), parsed));
                }
            }
        }
        let list = Value::Array(vec![item]);
        if read.is_empty() || at.accepts_near(&list, root) != Some(true) {
            return;
        }
        *value = list;
        let pointer = self.pointer.clone();
        self.reading.list_of_one_at(&pointer);
        for (key, parsed) in read {
            self.note_retyped(format!("{pointer}/0/{key}"), parsed);
        }
    }

    /// Notes that the scalar at `pointer` was parsed as `parsed` (the
    /// first reading noted is the parse's).
    fn note_retyped(&mut self, pointer: String, parsed: Value) {
        self.reading.retyped.entry(pointer).or_insert(parsed);
    }

    /// An object or list the schemas at it refuse as its values were read
    /// (`0` read as the string a `oneOf` option's `port` takes, so that two
    /// options match; `8` where a list's `contains` asks a text; a lone
    /// element where an option asks a list): each value read another way
    /// ([`other_reading`], or as a list's one item), one at a time, kept
    /// where that makes the schemas take it.
    fn reread_for_object(&mut self, value: &mut Value, at: &Entry<'_>, children: &[Entry<'_>]) {
        let root = self.root;
        if !(value.is_object() || value.is_array()) || at.accepts_near(value, root) != Some(false) {
            return;
        }
        // Each slot, and whether it may be a list's one item: a lone member
        // its schemas name a list for.
        let listed = |child: &Entry<'_>| {
            let here = Here::walk(child.clone(), root, Seen::default());
            here.types().allows_type("array") && here.branches().into_iter().any(describes_array)
        };
        let slots: Vec<(String, bool)> = match &*value {
            Value::Object(map) => map
                .iter()
                .zip(children)
                .map(|((key, held), child)| {
                    let slot = format!("/{}", pointer_key(key));
                    (slot, !held.is_array() && listed(child))
                })
                .filter(|(slot, item)| {
                    *item || value.pointer(slot).and_then(other_reading).is_some()
                })
                .take(MAX_REREADS)
                .collect(),
            Value::Array(items) => items
                .iter()
                .enumerate()
                .filter(|(_, item)| other_reading(item).is_some())
                .map(|(index, _)| (format!("/{index}"), false))
                .take(MAX_REREADS)
                .collect(),
            _ => Vec::new(),
        };
        for (slot, item) in slots {
            if let Some(other) = value.pointer(&slot).and_then(other_reading)
                && self.read_kept(value, &slot, at, Reading::Other(other))
            {
                return;
            }
            if !item {
                continue;
            }
            if self.read_kept(value, &slot, at, Reading::Item(None)) {
                return;
            }
            // The one item read the other way (`<x>-3</x>` as `["-3"]` where
            // the list asks a text and the number alone is refused).
            if let Some(other) = value.pointer(&slot).and_then(other_reading)
                && self.read_kept(value, &slot, at, Reading::Item(Some(other)))
            {
                return;
            }
        }
        // A scalar one level further down, which an option of this object
        // (not of the child holding it) refuses as read.
        let inner: Vec<String> = match &*value {
            Value::Object(map) => map
                .iter()
                .map(|(key, held)| (format!("/{}", pointer_key(key)), held))
                .collect(),
            Value::Array(items) => items
                .iter()
                .enumerate()
                .map(|(index, held)| (format!("/{index}"), held))
                .collect(),
            _ => Vec::new(),
        }
        .into_iter()
        .flat_map(|(slot, held)| -> Vec<String> {
            match held {
                Value::Object(map) => map
                    .iter()
                    .filter(|(_, inner)| other_reading(inner).is_some())
                    .map(|(key, _)| format!("{slot}/{}", pointer_key(key)))
                    .collect(),
                _ => Vec::new(),
            }
        })
        .take(MAX_REREADS)
        .collect();
        for slot in inner {
            if let Some(other) = value.pointer(&slot).and_then(other_reading)
                && self.read_kept(value, &slot, at, Reading::Other(other))
            {
                return;
            }
        }
    }

    /// The value at `slot` of `value` read as `other`, kept where the
    /// schemas at `value` then take it (else put back), and noted when
    /// kept.
    fn read_kept(&mut self, value: &mut Value, slot: &str, at: &Entry<'_>, other: Reading) -> bool {
        let Some(held) = value.pointer_mut(slot) else {
            return false;
        };
        let parsed = held.clone();
        let item = matches!(other, Reading::Item(_));
        let before = match other {
            Reading::Other(other) => Some(std::mem::replace(held, other)),
            Reading::Item(Some(other)) => Some(std::mem::replace(held, Value::Array(vec![other]))),
            Reading::Item(None) => {
                *held = Value::Array(vec![held.take()]);
                None
            }
        };
        if at.accepts_near(value, self.root) == Some(true) {
            let pointer = format!("{}{slot}", self.pointer);
            let scalar = if item {
                self.reading.list_of_one_at(&pointer);
                format!("{pointer}/0")
            } else {
                pointer
            };
            if !is_filled(&parsed) {
                self.note_retyped(scalar, parsed);
            }
            return true;
        }
        if let Some(held) = value.pointer_mut(slot) {
            *held = match (before, held.take()) {
                (Some(before), _) => before,
                (None, Value::Array(mut items)) => items.remove(0),
                (None, other) => other,
            };
        }
        false
    }
}

/// Values of an XML object or list [`NodeAt::reread_for_object`] tries
/// reading another way at most: a refused value is tried a bounded number
/// of times.
const MAX_REREADS: usize = 8;

/// Another reading of an XML value.
enum Reading {
    /// This value instead.
    Other(Value),
    /// A list's one item: the value itself, or this other reading of it.
    Item(Option<Value>),
}

/// Whether `value` holds something (a map or list that is not empty): no
/// scalar reading of it.
fn is_filled(value: &Value) -> bool {
    match value {
        Value::Object(map) => !map.is_empty(),
        Value::Array(_) => true,
        _ => false,
    }
}

/// XML text (`value`) read as the schema wants it: a number, boolean or
/// `null` the parser read is the text it is written as where the schema
/// wants that ([`read_as_text`]); text that is one in another spelling
/// (`1.50`, `1e3`, ` 4 `) is that value where the schema wants one and not
/// the text; and empty text (an empty element, read as `{}`, or an empty
/// attribute) is the empty string where the schema wants text, else
/// `null` where it allows that.
fn read_text(value: &mut Value, at: &Entry<'_>, branches: &[&Value], types: Types, root: &Value) {
    let allowed = |candidate: &Value| {
        types.allows(candidate)
            || branches
                .iter()
                .any(|branch| listed(branch).any(|listed| listed == candidate))
            || whole(candidate).is_some_and(|whole| types.allows(&whole))
    };
    let empty = match value {
        Value::Number(_) | Value::Bool(_) | Value::Null => {
            let text = scalar_text(value);
            if read_as_text(value, &text, at, branches, types, root) {
                *value = Value::String(text);
            }
            return;
        }
        Value::String(text) => {
            // Text the schema takes as it is stays text (`007` for "an
            // integer or a string").
            let fits = |candidate: &Value| at.accepts(candidate, root) == Some(true);
            if let Some(typed) = text_literal(text)
                && allowed(&typed)
                && !fits(&Value::String(text.clone()))
                && !read_as_text(&typed, text, at, branches, types, root)
            {
                *value = typed;
                return;
            }
            text.is_empty()
        }
        Value::Object(map) => map.is_empty(),
        Value::Array(_) => false,
    };
    if !empty || allowed(value) {
        return;
    }
    if types.allows_type("string") {
        *value = Value::String(String::new());
    } else if allowed(&Value::Null) {
        *value = Value::Null;
    }
}

/// A float with no fraction (`1e3`) as the integer it is, which an
/// `integer` type allows.
fn whole(value: &Value) -> Option<Value> {
    let number = value.as_number().filter(|number| number.is_f64())?;
    let float = number.as_f64()?;
    (float.fract() == 0.0 && float.abs() < 9.0e15).then(|| Value::from(float as i64))
}

/// Whether XML text read as a number, boolean or `null` (`value`) is the
/// string it is written as (`text`): where a string the schema lists is
/// that text (`const: "1.3"`), else where no option `value` fits but its
/// text does (`oneOf [integer ≥ 10, string]` for `4`), else where no option
/// allows its kind and text is allowed (`type: string`, or only strings
/// listed).
fn read_as_text(
    value: &Value,
    text: &str,
    at: &Entry<'_>,
    branches: &[&Value],
    types: Types,
    root: &Value,
) -> bool {
    let text = Value::String(text.to_string());
    let listed: Vec<&Value> = branches.iter().flat_map(|branch| listed(branch)).collect();
    if listed.contains(&&text) && !listed.contains(&value) {
        return true;
    }
    let fits = |candidate: &Value| at.accepts(candidate, root) == Some(true);
    if fits(value) {
        return false;
    }
    if fits(&text) {
        return true;
    }
    let allowed = listed.contains(&value) || types.allows(value);
    !allowed
        && (types.allows_type("string")
            || (!listed.is_empty() && listed.iter().all(|allowed| allowed.is_string())))
}

#[cfg(test)]
#[path = "conform_tests.rs"]
mod tests;
