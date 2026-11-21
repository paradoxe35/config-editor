//! What a block inserted for an object writes for its required children,
//! decided once for every language (each only writes it in its syntax):
//! a default the language keeps, a list as an empty list (`[]`) or with
//! the items its `minItems` asks for (each a value to type, an object
//! item's members required children of it), an object's own required
//! children filled the same way, the caret at the
//! first value left to type, and past it what the language keeps of a
//! value. A child whose key or value the language cannot write as text
//! reading back the same is left out, not written wrong.

use super::*;

/// One required child of an object, and what an inserted block writes for
/// it (see [`required_children`]).
#[derive(Clone, Debug, PartialEq)]
pub struct RequiredChild<'a> {
    /// Its key (as the schema names it, without a language's spelling of
    /// it: `Markup::required_property`).
    pub key: &'a str,
    /// Its schema.
    pub schema: &'a Value,
    /// What is written for it.
    pub fill: Fill<'a>,
    /// A schema at it marks it an XML attribute (`"xml": {"attribute":
    /// true}`); other languages ignore it. Such children come first: XML
    /// writes them in the start tag, before the content.
    pub attribute: bool,
}

/// What an inserted block writes for a required child.
#[derive(Clone, Debug, PartialEq)]
pub enum Fill<'a> {
    /// This value (a default, an empty list, a value Tab selects).
    Value(Value),
    /// An object of required children of its own, each filled the same way
    /// (written as a block below its key where the language writes one).
    Children(Vec<RequiredChild<'a>>),
    /// The value the caret goes to, left empty for the user to type; a
    /// string starts quoted, the caret inside, where the language quotes
    /// strings.
    Caret {
        /// Whether the value is text.
        string: bool,
    },
    /// This value with the caret at it (before it, inside an empty list):
    /// the block leaves no value to type, so the caret goes to its first.
    CaretAt(Value),
    /// A list written with the items its schemas ask for (`minItems`,
    /// `contains`), each filled as a required child is: an object item
    /// its members as required children of it (an XML attribute marked
    /// and first), a list item a list of its own, else its value.
    /// [`Fill::value`] is the list as a value, for a language that writes
    /// no attributes.
    List {
        /// Its items, each [`Fill::Value`], [`Fill::Children`] or
        /// [`Fill::List`].
        items: Vec<Fill<'a>>,
        /// Whether the caret goes to it: before it, then moved into its
        /// first item's value.
        caret: bool,
    },
    /// No value to write: an empty value where the language has one (a
    /// YAML `key:`, an XML `<key></key>`), else this one (`""`, `null`,
    /// `{}`), else the child is left out.
    Empty(Option<Value>),
}

impl Fill<'_> {
    /// The value this fill writes, an object's members and a list's items
    /// as values; `None` where it leaves the value to type
    /// ([`Fill::Caret`], [`Fill::Empty`] with none), and such a member of
    /// an object is left out.
    pub fn value(&self) -> Option<Value> {
        match self {
            Fill::Value(value) | Fill::CaretAt(value) | Fill::Empty(Some(value)) => {
                Some(value.clone())
            }
            Fill::Children(children) => Some(Value::Object(
                children
                    .iter()
                    .filter_map(|child| Some((child.key.to_string(), child.fill.value()?)))
                    .collect(),
            )),
            Fill::List { items, .. } => Some(Value::Array(
                items
                    .iter()
                    .map(|item| item.value().unwrap_or(Value::Null))
                    .collect(),
            )),
            Fill::Caret { .. } | Fill::Empty(None) => None,
        }
    }
}

/// How deep required children of required children are filled (a schema
/// requiring itself ends there).
const MAX_DEPTH: usize = 8;

/// The required children of the object `schema` describes (or of its
/// list's items, for a key a list of objects repeats), in the order the
/// schema lists them, each with what a block inserted for it writes.
pub fn required_children<'a>(
    language: &dyn ConfigLanguage,
    schema: &'a Value,
    root: &'a Value,
) -> Vec<RequiredChild<'a>> {
    required_children_at(language, schema, None, root)
}

/// [`required_children`] of a value `at` names every schema of: what the
/// document decides beside it (the `then` of an `if` it takes) asks for
/// children too.
pub(crate) fn required_children_at<'a>(
    language: &dyn ConfigLanguage,
    schema: &'a Value,
    at: Option<&Entry<'a>>,
    root: &'a Value,
) -> Vec<RequiredChild<'a>> {
    // A block writing the key's default or const writes no children.
    if kept_default(language, schema, root)
        .or_else(|| kept_const(language, schema, root))
        .is_some_and(|value| !value.is_object())
    {
        return Vec::new();
    }
    let mut caret = false;
    let mut found = children(language, schema, at, root, &mut caret, 0);
    if !caret {
        caret_at_first_value(&mut found);
    }
    found
}

/// Puts the caret at the first value of `children`, depth first; `false`
/// when there is none.
fn caret_at_first_value(children: &mut [RequiredChild<'_>]) -> bool {
    children.iter_mut().any(|child| match &mut child.fill {
        Fill::Value(value) => {
            child.fill = Fill::CaretAt(value.take());
            true
        }
        Fill::Children(nested) => caret_at_first_value(nested),
        Fill::List { caret, .. } => {
            *caret = true;
            true
        }
        _ => false,
    })
}

fn children<'a>(
    language: &dyn ConfigLanguage,
    schema: &'a Value,
    at: Option<&Entry<'a>>,
    root: &'a Value,
    caret: &mut bool,
    depth: usize,
) -> Vec<RequiredChild<'a>> {
    if depth >= MAX_DEPTH {
        return Vec::new();
    }
    // A key a list of objects repeats is its first item.
    let items = || {
        array_branch(schema, root)
            .and_then(|array| item_schema(array, 0))
            .filter(|items| object_branch(items, root).is_some())
    };
    let (object, at) = if object_branch(schema, root).is_some() {
        (schema, at)
    } else {
        match items() {
            Some(items) => (items, None),
            None => return Vec::new(),
        }
    };
    let caret_before = *caret;
    let (found, caret_written) = deciding(
        language,
        (object, at, root),
        |keys| {
            let mut placed = caret_before;
            let found = written(language, keys, root, &mut placed, depth);
            (found, placed)
        },
        |(found, _)| readings(found, root),
    );
    *caret = caret_written;
    let here = Here::of(object, root);
    let taken = |found: &[RequiredChild<'_>]| here.accepts(&preview(found, root));
    if found.is_empty() || taken(&found) != Some(false) {
        return found;
    }
    let caret_after = *caret;
    // Keys an option requires, where the object they make is refused and
    // the empty one taken (an option beside `{}` in a `oneOf`, which the
    // filled object meets too), are not written.
    if here.accepts(&Value::Object(serde_json::Map::new())) == Some(true) {
        *caret = caret_before;
        return Vec::new();
    }
    if here.judges_children_together() {
        // The schemas judge the object whole (its options): another
        // option's own required children, in order, the first they take.
        let first = object_branch(object, root);
        for branch in here.branches().into_iter().filter(|branch| {
            describes_object(branch) && !first.is_some_and(|first| std::ptr::eq(first, *branch))
        }) {
            *caret = caret_before;
            let other = written(
                language,
                required_keys(language, branch, None, root, None),
                root,
                caret,
                depth,
            );
            if !other.is_empty() && taken(&other) == Some(true) {
                return other;
            }
        }
        // None is taken, and the option the keys were written for takes
        // them: another option takes them too, so no value typed there is
        // ever taken. The object is left to type.
        if first.is_some_and(|first| decides(first, &preview(&found, root), root) == Some(true)) {
            *caret = caret_before;
            return Vec::new();
        }
    }
    *caret = caret_after;
    found
}

/// What a block writes for each of `keys` (a key, its schema, and what
/// applies to its value), left out where nothing is.
fn written<'a>(
    language: &dyn ConfigLanguage,
    keys: Vec<(&'a str, &'a Value, Entry<'a>)>,
    root: &'a Value,
    caret: &mut bool,
    depth: usize,
) -> Vec<RequiredChild<'a>> {
    let mut found = Vec::new();
    let mut keys: Vec<(bool, RequiredKey<'a>)> = keys
        .into_iter()
        .map(|(key, child, at)| {
            let attribute =
                super::insert_plan::marks_attribute(&Here::walk(at.clone(), root, Seen::default()));
            (attribute, (key, child, at))
        })
        .collect();
    // Attributes first (stable): the caret and the Tab stops follow the
    // written order, and XML writes them before the content.
    keys.sort_by_key(|(attribute, _)| !*attribute);
    for (attribute, (key, child, at)) in keys {
        if !writes_key(language, key) {
            continue;
        }
        let Some(fill) = fill(language, child, &at, root, caret, depth) else {
            continue;
        };
        found.push(RequiredChild {
            key,
            schema: child,
            fill,
            attribute,
        });
    }
    found
}

/// The object `children` write, a value left to type as its schema's
/// [`placeholder`]: what the user is to type there.
fn preview(children: &[RequiredChild<'_>], root: &Value) -> Value {
    Value::Object(
        children
            .iter()
            .map(|child| {
                let value = match &child.fill {
                    Fill::Value(value) | Fill::CaretAt(value) => value.clone(),
                    Fill::Children(nested) => preview(nested, root),
                    Fill::Caret { .. } | Fill::Empty(None) => placeholder(child.schema, root),
                    Fill::Empty(Some(value)) => value.clone(),
                    list @ Fill::List { .. } => list.value().unwrap_or_default(),
                };
                (child.key.to_string(), value)
            })
            .collect(),
    )
}

/// One required key: its name, its schema and what applies to its value.
pub(super) type RequiredKey<'a> = (&'a str, &'a Value, Entry<'a>);

/// What `write` writes for the required keys of `object` (with `at` and
/// `root`, see [`required_keys`]), what applies to each key decided by
/// the object the siblings make (each of its `readings`, which must
/// agree): an `if` on one sibling's value (`mode: {const: false}` where
/// `mode` is written as text) picks the `then` or `else` the others are
/// filled by. The keys are written once as the object's keys alone
/// decide them; where the object written decides more, once again for
/// that, and that is kept where the object it makes decides the same
/// (else the first). So an object is written at most twice at each
/// level, at most 2^8 times below the fill's depth bound
/// ([`MAX_DEPTH`]), a fixed factor; where nothing is decided by values
/// (most schemas), once.
pub(super) fn deciding<'a, T>(
    language: &dyn ConfigLanguage,
    (object, at, root): (&'a Value, Option<&Entry<'a>>, &'a Value),
    mut write: impl FnMut(Vec<RequiredKey<'a>>) -> T,
    readings: impl Fn(&T) -> Vec<Value>,
) -> T {
    // The keys as every reading of the object written decides them, where
    // they all agree.
    let decide = |written: &T| {
        let values = readings(written);
        let mut decided = values
            .iter()
            .map(|value| required_keys(language, object, at, root, Some(value)));
        let first = decided.next()?;
        decided
            .all(|other| same_keys(&first, &other))
            .then_some(first)
    };
    let keys = required_keys(language, object, at, root, None);
    let first = write(keys.clone());
    let Some(decided) = decide(&first).filter(|decided| !same_keys(&keys, decided)) else {
        return first;
    };
    let again = write(decided.clone());
    if decide(&again).is_some_and(|check| same_keys(&decided, &check)) {
        again
    } else {
        first
    }
}

/// The readings of the object `children` write that decide what applies
/// to them: a value left to type read as its schema's placeholder and as
/// nothing (`null`, as a language with an empty value reads it), so only
/// what both decide is taken.
fn readings(children: &[RequiredChild<'_>], root: &Value) -> Vec<Value> {
    vec![preview(children, root), written_only(children)]
}

/// The object `children` write, each value left to type `null`.
fn written_only(children: &[RequiredChild<'_>]) -> Value {
    Value::Object(
        children
            .iter()
            .map(|child| {
                let value = match &child.fill {
                    Fill::Value(value) | Fill::CaretAt(value) => value.clone(),
                    Fill::Children(nested) => written_only(nested),
                    list @ Fill::List { .. } => list.value().unwrap_or_default(),
                    Fill::Caret { .. } | Fill::Empty(_) => Value::Null,
                };
                (child.key.to_string(), value)
            })
            .collect(),
    )
}

/// Whether two readings of an object's required keys name the same keys
/// with the same schemas applying to each, and the same of them certain
/// to (a branch of an `if` one reading decides is named by both).
fn same_keys(left: &[RequiredKey<'_>], right: &[RequiredKey<'_>]) -> bool {
    let schemas = |at: &Entry<'_>| {
        let pointers = |schemas: Vec<&Value>| {
            schemas
                .into_iter()
                .map(std::ptr::from_ref)
                .collect::<Vec<_>>()
        };
        (pointers(at.schemas()), pointers(at.certain()))
    };
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|((left, _, at), (right, _, other))| {
                left == right && schemas(at) == schemas(other)
            })
}

/// The schema of a key no schema describes: any value.
static ANY_VALUE: std::sync::LazyLock<Value> =
    std::sync::LazyLock::new(|| Value::Object(serde_json::Map::new()));

/// The required keys of the object `schema` describes (as `language`
/// spells them), in the order the schemas list them, each once with the
/// schema describing it: the `required` of every schema certain to apply
/// to the object and of its object branch (an option it may take), and a
/// key's schema the one those give it (its property, a pattern matching
/// it, `additionalProperties`), else one allowing any value. `value`:
/// the object as it is to be written, where known, which decides more of
/// what applies than its keys alone (see [`deciding`]).
pub(super) fn required_keys<'a>(
    language: &dyn ConfigLanguage,
    schema: &'a Value,
    at: Option<&Entry<'a>>,
    root: &'a Value,
    value: Option<&Value>,
) -> Vec<RequiredKey<'a>> {
    let here = match at {
        Some(at) => Here::walk(at.clone(), root, Seen::default()),
        None => Here::of(schema, root),
    };
    let branch = object_branch(schema, root);
    // The keys `walked` requires, and each key a required one makes
    // required in turn (`dependentRequired`, draft 7's `dependencies`
    // lists, a dependent schema's `required`).
    let required_by = |walked: &Here<'a>| {
        let mut sources = walked.certain();
        if let Some(branch) = branch
            && !sources.iter().any(|known| std::ptr::eq(*known, branch))
        {
            sources.push(branch);
        }
        let mut names: Vec<&'a str> = Vec::new();
        for name in sources
            .iter()
            .filter_map(|source| source.get("required").and_then(Value::as_array))
            .flatten()
            .filter_map(Value::as_str)
        {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let mut at = 0;
        while at < names.len() && names.len() < MAX_REQUIRED {
            for name in dependent_required(&sources, names[at]) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
            at += 1;
        }
        names.truncate(MAX_REQUIRED);
        names
    };
    // A walk not knowing the keys takes every dependent schema and no
    // branch of an `if` asking keys: the keys it requires are a first
    // guess. What applies once the object holds just those keys (the
    // `then` or `else` such an `if` takes, the dependent schemas of the
    // keys written, not of a key it leaves out) decides the keys again,
    // until they agree.
    let mut names = required_by(&here);
    let mut filled;
    let mut rounds = 0;
    loop {
        let keys: HashSet<String> = names.iter().map(|name| (*name).to_string()).collect();
        filled = here.rewalk(Seen {
            value,
            keys: Some(&keys),
        });
        let again = required_by(&filled);
        rounds += 1;
        let agree = again.len() == names.len() && again.iter().all(|name| names.contains(name));
        if agree || rounds == MAX_KEY_ROUNDS {
            break;
        }
        names = again;
    }
    let mut found: Vec<(&'a str, &'a Value, Entry<'a>)> = Vec::new();
    for name in names {
        let key = language.required_property(name);
        if found.iter().any(|(known, _, _)| *known == key) {
            continue;
        }
        let at = filled.child(&SchemaPathSegment::Key(key.to_string()));
        let described = at.schemas();
        let child = described
            .iter()
            .find(|schema| !names_only(schema))
            .or(described.first())
            .copied()
            .unwrap_or(&*ANY_VALUE);
        found.push((key, child, at));
    }
    found
}

/// Most required keys one object's fill writes.
const MAX_REQUIRED: usize = 256;

/// Most times [`required_keys`] decides an object's keys again by what
/// the keys decided before make apply: an `if` turning on a key its own
/// branch requires (or leaves out) never agrees, and keeps the last.
const MAX_KEY_ROUNDS: usize = 4;

/// The keys `sources` (an object's schemas) require once `key` is there.
fn dependent_required<'a>(sources: &[&'a Value], key: &str) -> Vec<&'a str> {
    let listed = |list: &'a Value| {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
    };
    let mut found = Vec::new();
    for source in sources {
        if let Some(list) = source
            .get("dependentRequired")
            .and_then(|deps| deps.get(key))
        {
            found.extend(listed(list));
        }
        for keyword in ["dependencies", "dependentSchemas"] {
            match source.get(keyword).and_then(|deps| deps.get(key)) {
                Some(list @ Value::Array(_)) => found.extend(listed(list)),
                Some(schema) => {
                    found.extend(schema.get("required").map(listed).unwrap_or_default())
                }
                None => {}
            }
        }
    }
    found
}

/// What is written for one required child; `None` leaves it out.
fn fill<'a>(
    language: &dyn ConfigLanguage,
    child: &'a Value,
    at: &Entry<'a>,
    root: &'a Value,
    caret: &mut bool,
    depth: usize,
) -> Option<Fill<'a>> {
    if let Some(default) = kept_default(language, child, root)
        .filter(|default| at.accepts(default, root) != Some(false))
    {
        return Some(Fill::Value(default.clone()));
    }
    // A list or an object only where every schema at the child allows
    // one (a pattern typing the key `null` beside a property's
    // `prefixItems` does not, nor the `else` its object takes typing it a
    // boolean beside an option describing an object).
    let types = Here::walk(at.clone(), root, Seen::default()).types();
    let lists = types.allows_type("array");
    if lists && array_branch(child, root).is_some() && object_branch(child, root).is_none() {
        let empty = Value::Array(Vec::new());
        // A list the language cannot write (XML's empty one) is left out,
        // unless the schema takes a value that is no list there (list
        // keywords with no `type`): an empty value to type then.
        let Some(list) = least_list(language, at, root)
            .into_iter()
            .chain([empty])
            .find(|list| language_keeps(language, list, child, root))
        else {
            let takes_text = decides(child, &Value::String(String::new()), root) == Some(true);
            return takes_text.then_some(Fill::Empty(None));
        };
        // Items to type: the caret starts in the first one when no value
        // before the list took it.
        return Some(match list {
            Value::Array(items) if !items.is_empty() => {
                let here = !*caret;
                *caret = true;
                super::list_fill::list_fill(language, items, at, root, here)
            }
            list => Fill::Value(list),
        });
    }
    if types.allows_type("object") && object_branch(child, root).is_some() {
        let nested = children(language, child, Some(at), root, caret, depth + 1);
        if !nested.is_empty() {
            return Some(Fill::Children(nested));
        }
    }
    if !*caret {
        *caret = true;
        return Some(Fill::Caret {
            string: placeholder(child, root).is_string(),
        });
    }
    Some(match filled_past_caret(language, child, at, root) {
        Some(value) => Fill::Value(value),
        None => Fill::Empty(super::fill::fill_value_at(language, child, at, root)),
    })
}

/// Whether `language` writes `key` (of an object) as text that reads back
/// as that key: as its own writer does, which refuses a key it cannot
/// hold (an XML name with a space). A language that writes no documents
/// at all is trusted with every key.
pub fn writes_key(language: &dyn ConfigLanguage, key: &str) -> bool {
    let document = serde_json::json!({ "r": { key: 0 } });
    let Ok(text) = language.write_document(&document, None) else {
        let empty = serde_json::json!({ "r": {} });
        return language.write_document(&empty, None).is_err();
    };
    language.parse(&text).ok().is_some_and(|read| {
        read.get("r")
            .and_then(Value::as_object)
            .is_some_and(|members| members.contains_key(key))
    })
}

/// Whether the caret of `children` goes to a list written with items
/// (see [`Fill::List`]).
pub(crate) fn caret_at_list(children: &[RequiredChild<'_>]) -> bool {
    children.iter().any(|child| match &child.fill {
        Fill::List { caret, .. } => *caret,
        Fill::Children(nested) => caret_at_list(nested),
        _ => false,
    })
}
