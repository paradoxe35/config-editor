//! Schema docs for the key or documented value under the pointer.

use super::super::paths::{FieldPath, line_start, split_line, units_at};
use super::super::work_count::{Work, note};
use super::*;
use crate::editor::language::cursor_at;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct KeyHover {
    pub(crate) key: String,
    pub(crate) start_column: usize,
    pub(crate) type_label: String,
    /// The property's `title`, shown above its description.
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) default: Option<String>,
    pub(crate) examples: Vec<String>,
    pub(crate) required: bool,
    pub(crate) deprecated: bool,
    /// What to use instead, when the schema says (`deprecationMessage`).
    pub(crate) deprecation: Option<String>,
    /// The property's `x-docs-url`, a web address.
    pub(crate) docs_url: Option<String>,
    /// The `oneOf`/`anyOf` option the field's object is in, or all of
    /// them while none matches (see `choices`).
    pub(crate) choice: Option<ChoiceHover>,
    /// Set when the pointer is over one of the key's documented values
    /// rather than the key.
    pub(crate) value: Option<HoverValue>,
    /// An item of the list the key names (a repeated block, a YAML `- `, a
    /// JSON item's `{`): the item's own title and description, under the
    /// list's (those it does not share with it).
    pub(crate) item: Option<ValueDoc>,
}

impl KeyHover {
    /// Whether `other` shows what this shows, wherever on the line it was
    /// found (an XML element's end tag, a key's value): one card is for
    /// both.
    pub(crate) fn same_docs(&self, other: &KeyHover) -> bool {
        let Self {
            key,
            start_column: _,
            type_label,
            title,
            description,
            default,
            examples,
            required,
            deprecated,
            deprecation,
            docs_url,
            choice,
            value,
            item,
        } = self;
        *key == other.key
            && *type_label == other.type_label
            && *title == other.title
            && *description == other.description
            && *default == other.default
            && *examples == other.examples
            && *required == other.required
            && *deprecated == other.deprecated
            && *deprecation == other.deprecation
            && *docs_url == other.docs_url
            && *choice == other.choice
            && *value == other.value
            && *item == other.item
    }
}

/// A documented value under the pointer: an `enum` entry with its
/// `x-enum-descriptions` line, or a `const` branch with its title.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HoverValue {
    pub(crate) label: String,
    pub(crate) doc: ValueDoc,
}

/// The key under character `column` of zero-based `line_index`, with its
/// schema documentation; or, over a value the schema documents, that value
/// with its key's.
pub(crate) fn key_hover(
    source: &str,
    line_index: usize,
    column: usize,
    schema: Option<&SchemaBundle>,
    behavior: &dyn ConfigLanguage,
) -> Option<KeyHover> {
    let bundle = schema?;
    let field = field_at(source, line_index, column, behavior)?;
    field_docs(source, &field, bundle, behavior, Values::Documented)
}

/// What of a field is under a place of the text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Part {
    /// Its key (or the markup naming it: an XML tag, a TOML header).
    Key,
    /// The start of one of its items (a YAML `- `, a repeated block).
    ItemStart,
    /// Its value, as the parsed document has it (an item's, for a list).
    Value,
}

/// The field under a place of the text (see [`field_at`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FieldHit {
    /// Its path in the document: an item's, for an item or an item's value.
    pub(crate) path: Vec<SchemaPathSegment>,
    /// The key naming it (the list's, for an item).
    pub(crate) key: String,
    /// The character column its card is anchored at: where the key, the
    /// item or the value starts.
    pub(crate) start_column: usize,
    pub(crate) part: Part,
}

/// The field under character `column` of zero-based `line_index`: a key
/// as the language reads the line, an item the language says starts
/// there, or a value as the parsed document places it (its span in the
/// text): a value with spaces or punctuation in it is one value, and a
/// text that does not parse has none. Hover, `Ctrl+K Ctrl+I` and the
/// pointer's pause all ask it; only the docs read the schema.
pub(crate) fn field_at(
    source: &str,
    line_index: usize,
    column: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<FieldHit> {
    if let Some((path, key, start_column)) = hovered_key(source, line_index, column, behavior) {
        return Some(FieldHit {
            path,
            key,
            start_column,
            part: Part::Key,
        });
    }
    let line_start = line_start(source, line_index);
    let line = split_line(source, line_index)?;
    if behavior.hover_path(source, line_index, column).is_some()
        || behavior.hover_key(line, column).is_some()
    {
        return None;
    }
    if let Some(path) = item_at(source, line_start, line, column, behavior) {
        let key = list_key(&path)?;
        return Some(FieldHit {
            path,
            key,
            start_column: column,
            part: Part::ItemStart,
        });
    }
    value_at(source, line_start, line_index, line, column, behavior)
}

/// The value whose span holds character `column` of `line` (zero-based
/// `line_index`, starting at byte `line_start`): the field the context
/// there names, when the parsed document places its value over `column`.
fn value_at(
    source: &str,
    line_start: usize,
    line_index: usize,
    line: &str,
    column: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<FieldHit> {
    let at = line.char_indices().nth(column)?.0;
    if line[at..].starts_with(char::is_whitespace) {
        return None;
    }
    // Just past the character, as if it had been typed: the context names
    // the field whose value that is.
    let after = at + line[at..].chars().next().map_or(0, char::len_utf8);
    let context = behavior.completion_context(source, units_at(source, line_start + after));
    let (path, key) = match context.value_key {
        Some(key) => (path_with_key(&context.path, &key), key),
        None if context.is_array_item => (context.path.clone(), list_key(&context.path)?),
        None => return None,
    };
    if crate::editor::parse_cache::parsed(behavior, source).is_err() {
        return None;
    }
    let pointer = crate::editor::paths::FieldPath::from(path.clone()).to_pointer();
    let span = behavior.value_span(source, &FieldPath::from(&pointer))?;
    let on_it = span.line == line_index + 1
        && span.start_column <= column + 1
        && column + 1 < span.end_column;
    on_it.then(|| FieldHit {
        path,
        key,
        start_column: span.start_column - 1,
        part: Part::Value,
    })
}

/// Which values [`field_docs`] gives docs for.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Values {
    /// Only a value the schema documents on its own (its key's docs
    /// with it); none for another value.
    Documented,
    /// Any value: its field's docs, with the value's own where the schema
    /// documents it (the card's, under the pointer and at the caret).
    Any,
}

/// The docs of `field` in `source`: its property's, and over a value the
/// schema documents, that value's with them.
pub(crate) fn field_docs(
    source: &str,
    field: &FieldHit,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
    values: Values,
) -> Option<KeyHover> {
    note(Work::HoverLookup);
    let FieldHit {
        path,
        key,
        start_column,
        part,
    } = field;
    if *part != Part::Value {
        return property_hover(source, path, key.clone(), *start_column, bundle, behavior);
    }
    let documented = value_doc_at(source, path, bundle, behavior);
    if documented.is_none() && values == Values::Documented {
        return None;
    }
    // The docs of the property the value belongs to (an array's, for an item).
    let mut hover = property_hover(
        source,
        property_path(path),
        key.clone(),
        *start_column,
        bundle,
        behavior,
    )?;
    hover.value = documented;
    Some(hover)
}

/// The document's value at `path`, with the docs the schema gives it on
/// its own (an `enum` entry's `x-enum-descriptions` line, a `const`
/// branch's title); none for a value it does not document.
fn value_doc_at(
    source: &str,
    path: &[SchemaPathSegment],
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Option<HoverValue> {
    let root = &bundle.schema;
    let repeats = behavior.repetition() != Repetition::None;
    let document = Document::new(source, behavior);
    let read = || document.get();
    let applies = Applies::new(root, repeats, &read);
    let written = applies.value_at(path)?;
    if written.is_object() || written.is_array() {
        return None;
    }
    // Compared as text: XML writes every value as text.
    let text = crate::editor::applies::scalar_text(written);
    applies.at(path).branches().into_iter().find_map(|branch| {
        schema_value_candidates(branch, false)
            .into_iter()
            .find(|candidate| crate::editor::applies::scalar_text(candidate) == text)
            .and_then(|candidate| {
                Some(HoverValue {
                    label: value_label(&candidate),
                    doc: value_doc(branch, &candidate)?,
                })
            })
    })
}

/// The path of the list item that character `column` of `line` (starting
/// at byte `line_start` of `source`) opens, by what the language says opens
/// one there ([`Hover::item_start`]); None elsewhere.
fn item_at(
    source: &str,
    line_start: usize,
    line: &str,
    column: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<Vec<SchemaPathSegment>> {
    let inside = behavior.item_start(source, line_start, line, column)?;
    let path = behavior
        .completion_context(source, cursor_at(source, line_start, inside))
        .path;
    matches!(path.last(), Some(SchemaPathSegment::ArrayItem(_))).then_some(path)
}

/// The key naming the list an item `path` is in.
fn list_key(path: &[SchemaPathSegment]) -> Option<String> {
    path.iter().rev().find_map(|segment| match segment {
        SchemaPathSegment::Key(key) => Some(key.clone()),
        SchemaPathSegment::ArrayItem(_) => None,
    })
}

/// The key under character `column` of zero-based `line_index`: its path in
/// the document, the key, and the column it starts at. Hover docs and the
/// caret's path (see `paths`) both read it.
pub(crate) fn hovered_key(
    source: &str,
    line_index: usize,
    column: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<(Vec<SchemaPathSegment>, String, usize)> {
    let line = split_line(source, line_index)?;
    let line_start = line_start(source, line_index);
    let resolved = behavior.hover_path(source, line_index, column);
    let (key, start_column) = match &resolved {
        Some((_, key, start)) => (key.clone(), *start),
        None => behavior.hover_key(line, column)?,
    };
    let cursor = units_at(source, line_start + behavior.hover_cursor(line, column));
    let context = behavior.completion_context(source, cursor);
    // The hovered key is either the key whose value the context is in, or the
    // container the context is inside (a TOML `[table]` header, an HCL block
    // name, an XML element with attributes).
    let path = if let Some((path, _, _)) = resolved {
        path
    } else if context.value_key.as_deref() == Some(key.as_str()) {
        path_with_key(&context.path, &key)
    } else {
        let mut container = context.path.clone();
        while matches!(container.last(), Some(SchemaPathSegment::ArrayItem(_))) {
            container.pop();
        }
        if context.value_key.is_some()
            || container.last() != Some(&SchemaPathSegment::Key(key.clone()))
        {
            return None;
        }
        container
    };
    (!path.is_empty()).then_some((path, key, start_column))
}

/// The path of the property `path` is in: itself, or the array an item
/// (a repeated block) belongs to.
fn property_path(path: &[SchemaPathSegment]) -> &[SchemaPathSegment] {
    let kept = path
        .iter()
        .rposition(|segment| !matches!(segment, SchemaPathSegment::ArrayItem(_)))
        .map_or(0, |last| last + 1);
    &path[..kept]
}

/// The docs of the property at `path` in `source`, named `key`, shown at
/// `start_column`.
fn property_hover(
    source: &str,
    path: &[SchemaPathSegment],
    key: String,
    start_column: usize,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Option<KeyHover> {
    // A repeated block (TOML `[[svc]]`, HCL `svc {`, XML `<svc>`) is an
    // item of the array its key names: the array's own docs come first,
    // the item's after.
    let property = property_path(path);
    let parent = &property[..property.len().checked_sub(1)?];
    let root = &bundle.schema;
    let repeats = behavior.repetition() != Repetition::None;
    let document = Document::new(source, behavior);
    let read = || document.get();
    let applies = Applies::new(root, repeats, &read);
    // The object holding the key, with its `oneOf`/`anyOf` options (the
    // one it is in, or those offering the key): what is said of the key
    // is what its suggestion says (see `key_info`). A key of an option the
    // object is not in is not that option's (the step leaves it out).
    let parent_object = ObjectAt::of(&applies.at_object(parent, &HashSet::new()));
    let owners = if property.len() < path.len() {
        applies.at(property).schemas()
    } else {
        Vec::new()
    };
    let here = applies.at(path);
    let schemas = here.schemas();
    if schemas.is_empty() {
        return None;
    }
    let documented: Vec<&Value> = owners.iter().chain(&schemas).copied().collect();
    // What documents the field: the list's schemas (for an item) and its
    // own, walked for its value. An option of its object other than the
    // matched one documents nothing here, and the matched one's title and
    // description are its own row (`choice`), not the field's.
    let instance = Seen::value(applies.value_at(path));
    let field = Here::walk(Entry::any_of(&documented), root, instance);
    let branches = field.documented();
    // The schemas one object schema gives the key when it names it more
    // than once (its property and each pattern matching it), as validation
    // applies them all.
    let named: Vec<&Value> = match path.last() {
        Some(SchemaPathSegment::Key(name)) if owners.is_empty() => parent_object
            .branches
            .iter()
            .map(|branch| named_properties(branch, name))
            .find(|named| named.len() > 1)
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    // An item of a list: the list's docs, the item's own under them.
    let (title, description, item) = if named.len() > 1 {
        let each = |name: &str| {
            let mut texts: Vec<String> = Vec::new();
            for schema in &named {
                let own = Here::walk(Entry::schema(schema), root, instance).own();
                if let Some(text) = text_of(&own, name).filter(|text| !texts.contains(text)) {
                    texts.push(text);
                }
            }
            (!texts.is_empty()).then(|| texts.join(if name == "title" { ", " } else { " " }))
        };
        (each("title"), each("description"), None)
    } else if owners.is_empty() {
        let own = field.own();
        (text_of(&own, "title"), text_of(&own, "description"), None)
    } else {
        // (A repeating language finds the item at the list's path too.)
        let lists: Vec<&Value> = owners
            .iter()
            .filter(|&&owner| !schemas.iter().any(|&item| std::ptr::eq(owner, item)))
            .copied()
            .collect();
        let list = Here::walk(
            Entry::any_of(&lists),
            root,
            Seen::value(applies.value_at(property)),
        )
        .branches();
        let items = here.own();
        let (title, description) = (text_of(&list, "title"), text_of(&list, "description"));
        let own = |name: &str, list: &Option<String>| {
            text_of(&items, name).filter(|text| Some(text) != list.as_ref())
        };
        let item = ValueDoc {
            title: own("title", &title),
            description: own("description", &description),
        };
        (
            title,
            description,
            (item != ValueDoc::default()).then_some(item),
        )
    };
    let default = here
        .documented()
        .into_iter()
        .find_map(|branch| branch.get("default"))
        .map(value_label);
    let mut examples: Vec<String> = Vec::new();
    for branch in &branches {
        let values = match branch.get("examples") {
            Some(Value::Array(examples)) => examples.clone(),
            Some(example) => vec![example.clone()],
            None => Vec::new(),
        };
        for value in values.iter().chain(branch.get("example")) {
            let label = value_label(value);
            if !examples.contains(&label) {
                examples.push(label);
            }
        }
    }
    examples.truncate(5);
    let info = key_info(&key, &parent_object);
    let required = info.as_ref().is_some_and(|info| info.required);
    let type_label = if let Some(list) = owners.first() {
        schema_type_label(list, root, "property")
    } else if named.len() > 1 {
        type_label_of_all(&named, root, "property")
    } else if let Some(info) = info {
        info.type_label
    } else {
        schema_type_label(schemas[0], root, "property")
    };
    let choice = here
        .choices()
        .hover()
        .or_else(|| parent_object.choices.key_hover(&key))
        // The option's description when it is not the property's own.
        .and_then(|choice| match choice {
            ChoiceHover::Matched(mut doc) => {
                if doc.description == description {
                    doc.description = None;
                }
                (doc != ValueDoc::default()).then_some(ChoiceHover::Matched(doc))
            }
            other => Some(other),
        });
    Some(KeyHover {
        key,
        start_column,
        type_label,
        title,
        description,
        default,
        examples,
        required,
        deprecated: documented.iter().any(|schema| is_deprecated(schema, root)),
        deprecation: documented
            .iter()
            .find_map(|schema| deprecation_message(schema, root)),
        docs_url: documented.iter().find_map(|schema| docs_url(schema, root)),
        choice,
        value: None,
        item,
    })
}

/// The parsed document, parsed when first asked: `applies` asks only
/// where a step of a path is decided by it.
struct Document<'a> {
    parsed: std::cell::OnceCell<std::rc::Rc<Result<Value, super::super::language::ParseError>>>,
    source: &'a str,
    behavior: &'a dyn ConfigLanguage,
}

impl<'a> Document<'a> {
    fn new(source: &'a str, behavior: &'a dyn ConfigLanguage) -> Self {
        Self {
            parsed: std::cell::OnceCell::new(),
            source,
            behavior,
        }
    }

    fn get(&self) -> Option<&Value> {
        self.parsed
            .get_or_init(|| crate::editor::parse_cache::parsed(self.behavior, self.source))
            .as_ref()
            .as_ref()
            .ok()
    }
}

/// The first non-blank `name` text (`title`, `description`) of `branches`;
/// a value option (a `const` branch) documents that value, not the key.
fn text_of(branches: &[&Value], name: &str) -> Option<String> {
    branches
        .iter()
        .filter(|branch| branch.get("const").is_none())
        .find_map(|branch| branch.get(name).and_then(Value::as_str))
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod value_tests;
