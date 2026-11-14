//! Schema docs for the key or documented value under the pointer.

use super::super::paths::{line_start, split_line, units_at};
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
    note(Work::HoverLookup);
    let bundle = schema?;
    if let Some((path, key, start_column)) = hovered_key(source, line_index, column, behavior) {
        return property_hover(source, &path, key, start_column, bundle, behavior);
    }
    // No key under the pointer: maybe a value the schema documents.
    let line_start = line_start(source, line_index);
    let line = split_line(source, line_index)?;
    if behavior.hover_path(source, line_index, column).is_some()
        || behavior.hover_key(line, column).is_some()
    {
        return None;
    }
    if let Some(path) = item_at(source, line_start, line, column, behavior) {
        let key = list_key(&path)?;
        return property_hover(source, &path, key, column, bundle, behavior);
    }
    value_hover(source, line_start, line, column, bundle, behavior)
}

/// The path of the list item that character `column` of `line` (starting
/// at byte `line_start` of `source`) opens, by what the language says opens
/// one there ([`ConfigLanguage::item_start`]); None elsewhere.
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

/// Over a value (`fast` of `mode: fast`) that the schema documents on its
/// own: the value's title and description, with its key's docs.
fn value_hover(
    source: &str,
    line_start: usize,
    line: &str,
    column: usize,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Option<KeyHover> {
    let (start, end) = value_token(line, column)?;
    let token = &line[start..end];
    // Where completion would offer this value: its context names the key.
    let cursor = source[..line_start + end].encode_utf16().count();
    let context = behavior.completion_context(source, cursor);
    let (path, key) = match context.value_key {
        Some(key) => (path_with_key(&context.path, &key), key),
        None if context.is_array_item => (context.path.clone(), list_key(&context.path)?),
        None => return None,
    };
    let root = &bundle.schema;
    let repeats = behavior.repetition() != Repetition::None;
    let parsed = conditional_document(source, bundle, behavior);
    let document = parsed.as_deref().and_then(|parsed| parsed.as_ref().ok());
    let doc = schema_at_path_in(root, &path, repeats, document)
        .into_iter()
        .flat_map(|schema| resolve_branches(schema, root))
        .find_map(|branch| {
            schema_value_candidates(branch, false)
                .into_iter()
                .find(|candidate| value_label(candidate) == token)
                .and_then(|candidate| value_doc(branch, &candidate))
        })?;
    // The docs of the property the value belongs to (an array's, for an item).
    let mut hover = property_hover(
        source,
        property_path(&path),
        key,
        line[..start].chars().count(),
        bundle,
        behavior,
    )?;
    hover.value = Some(HoverValue {
        label: token.to_string(),
        doc,
    });
    Some(hover)
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

/// The byte range in `line` of the bare word at character `column`: a
/// value as written, without its quotes or the punctuation around it.
fn value_token(line: &str, column: usize) -> Option<(usize, usize)> {
    let stop = |character: char| character.is_whitespace() || "\"'`,:=[]{}<>#;".contains(character);
    let at = line.char_indices().nth(column)?.0;
    if line[at..].chars().next().is_none_or(stop) {
        return None;
    }
    let start = line[..at]
        .char_indices()
        .rev()
        .find(|(_, character)| stop(*character))
        .map_or(0, |(index, character)| index + character.len_utf8());
    let end = line[at..]
        .find(stop)
        .map_or(line.len(), |offset| at + offset);
    Some((start, end))
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
    let parsed = conditional_document(source, bundle, behavior);
    let document = parsed.as_deref().and_then(|parsed| parsed.as_ref().ok());
    let at = |path: &[SchemaPathSegment]| document.and_then(|document| value_at(document, path));
    // A key of `oneOf`/`anyOf` options of the object holding it: the
    // option it is in (or those offering it), and its schema there. What
    // is said of it is what its suggestion says (see `key_info`).
    let parents = schema_at_path_in(root, parent, repeats, document);
    let parent_choices =
        ChoicesAt::in_object(&parents, root, || object_at(source, parent, behavior));
    let parent_branches: Vec<&Value> = parents
        .iter()
        .flat_map(|schema| branches_for(schema, root, at(parent)))
        .collect();
    let in_matched = |schemas| match &property[property.len() - 1] {
        SchemaPathSegment::Key(name) => {
            in_matched_option(schemas, name, &parent_branches, &parent_choices)
        }
        SchemaPathSegment::ArrayItem(_) => schemas,
    };
    let owners = if property.len() < path.len() {
        in_matched(schema_at_path_in(root, property, repeats, document))
    } else {
        Vec::new()
    };
    let schemas = schema_at_path_in(root, path, repeats, document);
    let schemas = if owners.is_empty() {
        in_matched(schemas)
    } else {
        schemas
    };
    if schemas.is_empty() {
        return None;
    }
    let documented: Vec<&Value> = owners.iter().chain(&schemas).copied().collect();
    // The `oneOf`/`anyOf` options of the field's object: an option other
    // than the matched one documents nothing here, and the matched one's
    // title and description are its own row (`choice`), not the field's.
    let options = ChoicesAt::in_object(&schemas, root, || object_at(source, path, behavior));
    let not_other_option = |branch: &&Value| !matches!(options.of(branch), Some((_, false)));
    let branches: Vec<&Value> = branches_of(&documented, root, at(path))
        .into_iter()
        .filter(not_other_option)
        .collect();
    // The schemas one object schema gives the key when it names it more
    // than once (its property and each pattern matching it), as validation
    // applies them all.
    let named: Vec<&Value> = match path.last() {
        Some(SchemaPathSegment::Key(name)) if owners.is_empty() => parents
            .iter()
            .flat_map(|schema| branches_for(schema, root, at(parent)))
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
                let own = field_own(&options, &branches_of(&[*schema], root, at(path)));
                if let Some(text) = text_of(&own, name).filter(|text| !texts.contains(text)) {
                    texts.push(text);
                }
            }
            (!texts.is_empty()).then(|| texts.join(if name == "title" { ", " } else { " " }))
        };
        (each("title"), each("description"), None)
    } else if owners.is_empty() {
        let own = field_own(&options, &branches);
        (text_of(&own, "title"), text_of(&own, "description"), None)
    } else {
        // (A repeating language finds the item at the list's path too.)
        let lists: Vec<&Value> = owners
            .iter()
            .filter(|&&owner| !schemas.iter().any(|&item| std::ptr::eq(owner, item)))
            .copied()
            .collect();
        let list = branches_of(&lists, root, at(property));
        let items = field_own(&options, &branches_of(&schemas, root, at(path)));
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
    let default = branches_of(&schemas, root, at(path))
        .into_iter()
        .filter(not_other_option)
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
    let info = key_info(&key, &parent_branches, &parent_choices, root);
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
    let choice = options
        .hover()
        .or_else(|| parent_choices.key_hover(&key))
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

/// Of the schemas of a key `name` more than one `oneOf`/`anyOf` option
/// of its object (`branches`, `choices`) names, the matched option's (all
/// of them when that leaves none): it documents the key, as its
/// suggestion does.
fn in_matched_option<'a>(
    schemas: Vec<&'a Value>,
    name: &str,
    branches: &[&Value],
    choices: &ChoicesAt<'_>,
) -> Vec<&'a Value> {
    let others: Vec<&Value> = branches
        .iter()
        .filter(|branch| matches!(choices.of(branch), Some((_, false))))
        .flat_map(|branch| named_properties(branch, name))
        .collect();
    let kept: Vec<&'a Value> = schemas
        .iter()
        .filter(|schema| !others.iter().any(|other| std::ptr::eq(*other, **schema)))
        .copied()
        .collect();
    if kept.is_empty() { schemas } else { kept }
}

/// The branches outside every option of `options`: the field's own.
fn field_own<'a>(options: &ChoicesAt<'_>, branches: &[&'a Value]) -> Vec<&'a Value> {
    branches
        .iter()
        .filter(|branch| options.of(branch).is_none())
        .copied()
        .collect()
}

/// The branches of `schemas` for `instance` (see [`branches_for`]).
fn branches_of<'a>(
    schemas: &[&'a Value],
    root: &'a Value,
    instance: Option<&Value>,
) -> Vec<&'a Value> {
    schemas
        .iter()
        .flat_map(|schema| branches_for(schema, root, instance))
        .collect()
}

/// The parsed document, when the schema has conditionals its values
/// decide (see [`branches_for`]); not read otherwise.
fn conditional_document(
    source: &str,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Option<std::rc::Rc<Result<Value, super::super::language::ParseError>>> {
    bundle
        .has_conditionals
        .then(|| crate::editor::parse_cache::parsed(behavior, source))
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
mod tests {
    use super::value_token;

    #[test]
    fn value_tokens_leave_out_quotes_and_markup() {
        let word = |line: &str, column: usize| {
            value_token(line, column).map(|(start, end)| line[start..end].to_string())
        };
        assert_eq!(word("mode: fast", 7).as_deref(), Some("fast"));
        assert_eq!(word("mode = \"fast\"", 9).as_deref(), Some("fast"));
        assert_eq!(word("<mode>fast</mode>", 7).as_deref(), Some("fast"));
        assert_eq!(word("  \"mode\": \"fast\",", 12).as_deref(), Some("fast"));
        assert_eq!(word("mode: fast", 4), None, "the separator is no value");
        assert_eq!(word("mode: fast", 40), None);
    }
}
