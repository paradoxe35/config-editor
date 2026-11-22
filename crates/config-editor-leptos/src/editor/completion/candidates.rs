//! The suggestions for a caret position: keys and values the schema
//! allows there, framed for the language.

use std::ops::Range;

use super::elsewhere::OtherPlaces;
use super::named_keys::{KeyEntry, named_keys};
use super::placed::Placed;
use super::*;
use crate::editor::key_names::{KeptKeys, KeyNameSource, NameLookup};
use crate::editor::text::text_size;

/// Schemas often give a port a string example (`"7835"`) next to an integer
/// branch; when numbers are allowed, insert the bare number.
pub(super) fn numeric_example_as_number(value: Value, numeric_allowed: bool) -> Value {
    if numeric_allowed
        && let Value::String(text) = &value
        && let Ok(number) = text.parse::<i64>()
    {
        return Value::from(number);
    }
    value
}

/// What finds the names a value may take (`x-config-editor-keys`): the
/// schema's identity and the names beyond the document, the document
/// around the caret (read only for a field with the keyword), and the
/// items its list holds already, which are not offered again.
pub(super) struct ValueNames<'a> {
    pub(super) lookup: NameLookup<'a>,
    pub(super) document: &'a dyn Fn() -> Option<&'a Value>,
    pub(super) taken: HashSet<String>,
}

#[expect(
    clippy::too_many_arguments,
    reason = "one suggestion run's place, schema and language, passed as they are read"
)]
pub(super) fn value_completions(
    here: &Here<'_>,
    typed: &str,
    replace: TextRange,
    root: &Value,
    leading: &str,
    open: bool,
    behavior: &dyn ConfigLanguage,
    names: &ValueNames<'_>,
) -> Vec<Completion> {
    let query = unquoted(behavior, typed);
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    let branches = here.branches();
    // A language without null (TOML) is not offered one.
    let null_writable = behavior
        .write_document(&serde_json::json!({ "value": null }), None)
        .is_ok();
    let numeric_allowed = branches
        .iter()
        .any(|branch| type_includes(branch, "integer") || type_includes(branch, "number"));
    // The names first, before the values the schema lists.
    let named = branches
        .iter()
        .find(|branch| branch.get(crate::editor::key_names::KEYWORD).is_some());
    if let Some(&named) = named {
        let type_label = schema_type_label(named, root, "value");
        for offered in names.lookup.offered(&branches, || (names.document)()) {
            let value = Value::String(offered.name);
            let label = value_label(&value);
            if (names.taken.contains(&label) && label != query)
                || !seen.insert(label.clone())
                || here.accepts(&value) == Some(false)
            {
                continue;
            }
            let Some(score) = match_score(&label, query) else {
                continue;
            };
            let insert = format!("{leading}{}", behavior.write_value(&value));
            candidates.push((
                score,
                Completion {
                    matched: match_ranges(&label, query),
                    label,
                    detail: offered.detail.unwrap_or_default(),
                    type_label: type_label.clone(),
                    cursor_offset: TextSize::of(insert.as_str()),
                    insert,
                    start: replace.start(),
                    end: replace.end(),
                    selected: TextSize::default(),
                    is_value: true,
                    renames: false,
                    deprecated: false,
                    fills: Vec::new(),
                    closes_tag: false,
                    key_origin: Some(offered.origin),
                    read_only: false,
                },
            ));
        }
    }
    {
        for &branch in &branches {
            let description = branch
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let type_label = schema_type_label(branch, root, "value");
            let docs = ValueDocs::of(branch);
            for value in schema_value_candidates(branch, open) {
                let value = numeric_example_as_number(value, numeric_allowed);
                let label = value_label(&value);
                // A value every schema here refuses is none to offer (one
                // a pattern property or an `else` beside it rules out).
                if !seen.insert(label.clone())
                    || (value.is_null() && !null_writable)
                    || here.accepts(&value) == Some(false)
                {
                    continue;
                }
                if let Some(score) = match_score(&label, query) {
                    let insert = format!("{leading}{}", behavior.write_value(&value));
                    // The value's own words, else its branch's.
                    let detail = docs
                        .of_value(&value)
                        .and_then(|doc| doc.detail())
                        .unwrap_or_else(|| description.to_string());
                    candidates.push((
                        score,
                        Completion {
                            label: label.clone(),
                            detail,
                            type_label: type_label.clone(),
                            cursor_offset: TextSize::of(insert.as_str()),
                            insert,
                            start: replace.start(),
                            end: replace.end(),
                            selected: TextSize::default(),
                            is_value: true,
                            renames: false,
                            matched: match_ranges(&label, query),
                            deprecated: false,
                            fills: Vec::new(),
                            closes_tag: false,
                            key_origin: None,
                            read_only: false,
                        },
                    ));
                }
            }
        }
    }
    sorted_completions(candidates)
}

/// The keys a schema certain to apply (`certain`, see [`Here::certain`])
/// declares `false` (`properties: {b: false}`, the `else` an `if` takes
/// saying `url: false`) or otherwise admitting nothing ([`admits_nothing`]:
/// `{not: {}}`): no value of theirs is valid, so they are not offered.
fn forbidden_keys<'a>(certain: &[&'a Value]) -> HashSet<&'a str> {
    certain
        .iter()
        .filter_map(|branch| branch.get("properties").and_then(Value::as_object))
        .flat_map(|properties| {
            properties
                .iter()
                .filter(|(_, schema)| admits_nothing(schema))
                .map(|(key, _)| key.as_str())
        })
        .collect()
}

/// Whether a list key written by repeating its element (`property` its
/// schema, `written` what the object holds for it) takes one more: not
/// one whose items have places (the new item, at the caret, would move
/// the others to places that type them otherwise), nor one at its
/// `maxItems`, nor one whose lone value written is not taken as a list's
/// item (list keywords with no `type` take it as itself), nor one the
/// item the insert writes (its default or const) makes refused (a
/// `uniqueItems` it repeats).
fn repeats_as_elements(
    property: &Value,
    written: Option<&Value>,
    root: &Value,
    behavior: &dyn ConfigLanguage,
) -> bool {
    let Some(array) = array_branch(property, root) else {
        return false;
    };
    if array.get("prefixItems").is_some() || array.get("items").is_some_and(Value::is_array) {
        return false;
    }
    let Some(written) = written else {
        return true;
    };
    // The items as the schema reads them (an XSD boolean written `1` is
    // `true`), as validation reads them: a list validation takes takes
    // one more.
    let read = crate::editor::data_reading::value_under(
        behavior,
        written.clone(),
        &crate::editor::applies::Entry::schema(property),
        root,
    );
    let items = match read {
        Value::Array(items) => items,
        lone => vec![lone],
    };
    let most = array.get("maxItems").and_then(Value::as_u64);
    let next = item_schema(array, items.len()).and_then(|item| {
        kept_default(behavior, item, root).or_else(|| kept_const(behavior, item, root))
    });
    let takes_next = next.is_none_or(|next| {
        let mut more = items.clone();
        more.push(next.clone());
        decides(property, &Value::Array(more), root) != Some(false)
    });
    most.is_none_or(|most| (items.len() as u64) < most)
        && takes_next
        && (written.is_array() || decides(property, &Value::Array(items), root) != Some(false))
}

/// Of `fills` (the values a block writes for required children), those
/// `insert` holds where [`stop_ranges`] finds them, in order.
fn written_fills(
    insert: &str,
    cursor_offset: TextSize,
    fills: Vec<(String, String)>,
) -> Vec<(String, String)> {
    let mut kept: Vec<(String, String)> = Vec::with_capacity(fills.len());
    for fill in fills {
        kept.push(fill);
        if stop_ranges(insert, cursor_offset, &kept).len() != kept.len() {
            kept.pop();
        }
    }
    kept
}

/// Whether writing `key` (its value `property`'s placeholder) into the
/// object `written` makes the schemas at the object refuse a value it
/// already holds: what applies to each value is walked again for the
/// object with the key (a dependent schema of the key, the `then` of an
/// `if` the key decides, the option the key picks), and a value they took
/// is refused. Keys the object then requires are not counted: the problem
/// then names the key to write next. A language read as data (XML, HCL
/// blocks) has the object read as the schemas read it, before and with the
/// key ([`value_under`]): the reading the key makes the schemas take, if
/// any (`<port>0</port>` the text a dependent schema asks, or the list
/// `port` asks), is the one judged.
///
/// [`value_under`]: crate::editor::data_reading::value_under
fn refused_beside(
    key: &str,
    property: &Value,
    here: &Here<'_>,
    written: Option<&Value>,
    root: &Value,
    language: &dyn ConfigLanguage,
) -> bool {
    let Some(object) = written.and_then(Value::as_object) else {
        return false;
    };
    let mut with_key = object.clone();
    with_key.insert(key.to_string(), placeholder(property, root));
    let with_key = Value::Object(with_key);
    let after = here.rewalk(Seen::value(Some(&with_key)));
    let read = |value: &Value, at: &Here<'_>| {
        crate::editor::data_reading::value_under(language, value.clone(), at.entry(), root)
    };
    let (seen_before, seen_after) = (
        read(&Value::Object(object.clone()), here),
        read(&with_key, &after),
    );
    object
        .keys()
        .filter(|name| *name != key)
        .take(MAX_READ_BESIDE)
        .any(|name| {
            let segment = SchemaPathSegment::Key(name.clone());
            let (before, after) = (here.child(&segment), after.child(&segment));
            let (Some(was), Some(now)) = (seen_before.get(name), seen_after.get(name)) else {
                return false;
            };
            before.accepts(was, root) != Some(false) && after.accepts(now, root) == Some(false)
        })
}

/// Values beside a suggested key [`refused_beside`] judges at most: the
/// check costs the same for any object.
const MAX_READ_BESIDE: usize = 8;

/// `(cursor_offset, selected)` (bytes of `insert`) for an inserted key: a
/// scalar default the insert wrote just before the caret (`max_pending:
/// 512|`, `mode = "fail|"`) is selected, so typing replaces it instead of
/// appending to it.
pub(super) fn default_selection(
    insert: &str,
    cursor_offset: TextSize,
    property: &Value,
    root: &Value,
    behavior: &dyn ConfigLanguage,
) -> (TextSize, TextSize) {
    let unselected = (cursor_offset, TextSize::default());
    let Some(default) = kept_default(behavior, property, root)
        .or_else(|| kept_const(behavior, property, root))
        .filter(|value| !value.is_object() && !value.is_array() && !value.is_null())
    else {
        return unselected;
    };
    let text = match default {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    if text.is_empty() {
        return unselected;
    }
    let Some(before) = slice::before(insert, cursor_offset) else {
        return unselected;
    };
    let length = TextSize::of(text.as_str());
    if before.ends_with(&text) {
        return (cursor_offset, length);
    }
    // The caret is after the closing quote: select inside the quotes.
    let quotes = quote_starts(behavior);
    if let Some(quoted) = before.strip_suffix(quotes.as_slice())
        && quoted.ends_with(&text)
    {
        return (TextSize::of(quoted), length);
    }
    unselected
}

/// The keys offered for the object `here` is what applies to (walked for
/// its keys and value).
#[expect(
    clippy::too_many_arguments,
    reason = "one suggestion run's place, schema and language, passed as they are read"
)]
pub(super) fn property_completions<'s>(
    here: &Here<'s>,
    typed: &str,
    replace: TextRange,
    root: &Value,
    existing: Option<&HashSet<String>>,
    repetition: Repetition,
    insert_context: &InsertContext<'_>,
    behavior: &dyn ConfigLanguage,
    written: Option<&Value>,
    read_rewritable: &dyn Fn() -> HashSet<String>,
    names: &ValueNames<'_>,
) -> Vec<Completion> {
    let rewritable = std::cell::OnceCell::new();
    let rewritable_keys = || rewritable.get_or_init(read_rewritable);
    let mut candidates = Vec::new();
    let mut named = Vec::new();
    let object = ObjectAt::of(here);
    let choices = &object.choices;
    let branches = &object.branches;
    let forbidden = forbidden_keys(&object.certain);
    // Some property of the object is marked an XML attribute.
    let attributes_marked = branches
        .iter()
        .filter_map(|branch| branch.get("properties").and_then(Value::as_object))
        .flat_map(|properties| properties.values())
        .any(|property| super::insert_plan::marks_attribute(&Here::of(property, root)));
    // An object holding its `maxProperties` takes no other key.
    let full = existing.is_some_and(|existing| {
        object
            .certain
            .iter()
            .filter_map(|branch| branch.get("maxProperties")?.as_u64())
            .any(|most| existing.len() as u64 >= most)
    });
    // A key of `oneOf`/`anyOf` options names them (the matched one alone
    // when it has the key); once an option matches, another option's keys
    // are not offered (adding one would leave it).
    let mut entries = Vec::new();
    for branch in &here.offered() {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let choice = choices.of_branch(branch);
        let bonus = if choice.is_some_and(|(_, matched)| matched) {
            MATCHED_CHOICE_BONUS
        } else {
            0
        };
        let required: HashSet<&str> = branch
            .get("required")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for (key, property) in properties {
            // A key this branch only names is described where another
            // declares it: its type, its docs, what accepting it writes.
            let Some(info) = key_info(key, &object) else {
                continue;
            };
            let property = if names_only(property) {
                info.schema
            } else {
                property
            };
            entries.push(KeyEntry {
                key: key.clone(),
                property,
                bonus: bonus
                    + if required.contains(key.as_str()) {
                        REQUIRED_BONUS
                    } else {
                        0
                    },
                type_label: info.type_label,
                detail: ValueDoc {
                    title: choices.titles_holding(key),
                    description: property
                        .get("description")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                }
                .detail()
                .unwrap_or_default(),
                origin: None,
                named: false,
            });
        }
    }
    // Then the keys its `propertyNames` names.
    entries.extend(named_keys(here, &object, names));
    for entry in entries {
        let key = &entry.key;
        let property = entry.property;
        // A list key written by repetition can appear again: any list in
        // XML; a list of objects as HCL blocks (never inside `{ … }`),
        // once it is written so (not as one whole list).
        let present = existing.is_some_and(|existing| existing.contains(key))
            && (repetition != Repetition::None || !rewritable_keys().contains(key));
        let repeatable = match repetition {
            Repetition::Elements => repeats_as_elements(
                property,
                written.and_then(|written| written.get(key)),
                root,
                behavior,
            ),
            Repetition::Blocks => {
                insert_context.position == EntryPosition::Block
                    && array_branch(property, root)
                        .and_then(|array| array.get("items"))
                        .is_some_and(|items| object_branch(items, root).is_some())
                    && (!present || rewritable_keys().contains(key))
            }
            _ => false,
        };
        if !repeatable && present || forbidden.contains(key.as_str()) || full && !present {
            continue;
        }
        // A key a `not` keeps apart from keys written (`git` beside
        // `path`): writing it would break the `not`. Nor a key that
        // makes the schemas refuse a value written beside it.
        if existing.is_some_and(|existing| excluded_by_not(key, branches, existing, root))
            || (!present && refused_beside(key, property, here, written, root, behavior))
        {
            continue;
        }
        let Some(score) = match_score(key, typed) else {
            continue;
        };
        let score = score + entry.bonus;
        let another = InsertContext {
            another_item: present && repeatable,
            attributes_marked,
            ..*insert_context
        };
        // Another item of a list written by repeating its key (an XML
        // element, an HCL block) is one item: written as its items'
        // schema says.
        let written_schema = if another.another_item {
            array_branch(property, root)
                .and_then(|array| item_schema(array, 1))
                .unwrap_or(property)
        } else {
            property
        };
        // A default (or const) another schema at the key refuses (the
        // `else` the object takes typing it otherwise) is not written:
        // the key is written as a schema there whose value is not
        // refused describes it.
        let written_schema = if another.another_item {
            written_schema
        } else {
            let at_key = here.child(&SchemaPathSegment::Key(key.to_string()));
            let refused = |schema: &Value| {
                kept_default(behavior, schema, root)
                    .or_else(|| kept_const(behavior, schema, root))
                    .is_some_and(|value| at_key.accepts(value, root) == Some(false))
            };
            if refused(written_schema) {
                at_key
                    .schemas()
                    .into_iter()
                    .find(|schema| !refused(schema))
                    .unwrap_or(written_schema)
            } else {
                written_schema
            }
        };
        // The children every schema at the key asks for (a `then` the
        // document takes beside the property): not for one more item.
        let at_key =
            (!another.another_item).then(|| here.child(&SchemaPathSegment::Key(key.to_string())));
        let plan = InsertPlan::at(behavior, written_schema, at_key.as_ref(), root);
        let Some((insert, cursor_offset)) = behavior.write_insert(key, &plan, &another) else {
            continue;
        };
        // The language API places the caret in UTF-16 units.
        let cursor_offset = byte_of(&insert, cursor_offset);
        let children = plan.children;
        // Only the values the insert writes are Tab stops (a list
        // written `[]` writes none of its items' children).
        let fills = written_fills(&insert, cursor_offset, fills_of(&children));
        let (cursor_offset, selected) =
            default_selection(&insert, cursor_offset, written_schema, root, behavior);
        // A list written with items to type: the caret in the first.
        let (cursor_offset, selected) =
            if selected == TextSize::default() && caret_at_list(&children) {
                caret_in_list(&insert, cursor_offset, &fills, behavior)
                    .unwrap_or((cursor_offset, selected))
            } else {
                (cursor_offset, selected)
            };
        let completion = (
            score,
            Completion {
                label: key.clone(),
                detail: entry.detail,
                type_label: entry.type_label,
                insert,
                start: replace.start(),
                end: replace.end(),
                cursor_offset,
                selected,
                // A name alone (a table header segment) completes the
                // entry, so the list does not reopen on it.
                is_value: insert_context.position == EntryPosition::Name,
                renames: false,
                matched: match_ranges(key, typed),
                deprecated: is_deprecated(property, root),
                fills,
                closes_tag: false,
                key_origin: entry.origin,
                read_only: false,
            },
        );
        if entry.named {
            named.push(completion);
        } else {
            candidates.push(completion);
        }
    }
    // The named keys follow the declared ones, best match first, else in
    // the order found (as a value's names are).
    let mut options = sorted_completions(candidates);
    named.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    for (_, option) in named {
        if !options.iter().any(|offered| offered.label == option.label) {
            options.push(option);
        }
    }
    options
}

#[cfg(test)]
pub(crate) fn completions(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    completions_for(source, cursor, force, schema, enabled, behavior, false)
}

/// [`completions`] for the word before `cursor` as a whole to be replaced,
/// as Ctrl+Space over a selected key does: every key is offered, not only
/// those matching the word, and accepting one renames it.
#[cfg(test)]
pub(crate) fn completions_for(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
    replace_word: bool,
) -> Vec<Completion> {
    if !enabled || schema.is_none() {
        return Vec::new();
    }
    let bodies = behavior.string_body_lines(source);
    // The tests write the caret in UTF-16 units, as the language API does.
    completions_in(
        source,
        byte_of(source, cursor),
        force,
        schema,
        enabled,
        behavior,
        replace_word,
        &bodies,
        (&crate::editor::key_names::NoKeyNames, None),
    )
}

/// The suggestions at byte `at` of `source`, with the text's multi-line
/// string bodies already found (`Text::string_body_lines`): the live editor
/// keeps them in a memo, so a keystroke does not parse the whole document
/// for them.
#[expect(
    clippy::too_many_arguments,
    reason = "one suggestion run's place, schema and language, passed as they are read"
)]
pub(crate) fn completions_in(
    source: &str,
    at: TextSize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
    replace_word: bool,
    string_bodies: &[Range<usize>],
    names: (&dyn KeyNameSource, Option<&KeptKeys>),
) -> Vec<Completion> {
    if !enabled {
        return Vec::new();
    }
    let Some(bundle) = schema else {
        return Vec::new();
    };
    let at = at.min(TextSize::of(source));
    // The language API takes the caret in UTF-16 units.
    let cursor = utf16_of(source, at);
    // Nothing is suggested inside a comment or a multi-line string.
    let line = slice::before(source, at)
        .unwrap_or(source)
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    if super::super::parse_cache::lexical_state(behavior, source, cursor).in_comment
        || string_bodies.iter().any(|lines| lines.contains(&line))
    {
        return Vec::new();
    }
    let cursor_byte = usize::from(at);
    // In a line's indentation, before its text: as where the text starts,
    // so a new entry lines up with it instead of splitting its indent.
    let line_start = source[..cursor_byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[cursor_byte..]
        .find('\n')
        .map_or(source.len(), |at| cursor_byte + at);
    let indent_end = line_start + source[line_start..line_end].len()
        - source[line_start..line_end].trim_start().len();
    if cursor_byte < indent_end && indent_end < line_end {
        return completions_in(
            source,
            text_size(indent_end),
            force,
            schema,
            enabled,
            behavior,
            replace_word,
            string_bodies,
            names,
        );
    }
    let mut context = super::super::parse_cache::completion_context(behavior, source, cursor);
    let mut placed = Placed::of(source, &context);
    // A closing tag being typed is closed, never renamed.
    if let Some((name, close)) = &context.closing_name {
        let typed = context.typed.to_lowercase();
        if !force && !name.to_lowercase().starts_with(&typed) {
            return Vec::new();
        }
        let insert = format!("{name}{close}");
        return vec![Completion {
            label: name.clone(),
            detail: String::new(),
            type_label: String::new(),
            closes_tag: true,
            key_origin: None,
            read_only: false,
            cursor_offset: TextSize::of(insert.as_str()),
            insert,
            start: placed.replace.start(),
            end: placed.replace.end(),
            selected: TextSize::default(),
            is_value: true,
            renames: true,
            matched: match_ranges(name, &context.typed),
            deprecated: false,
            fills: Vec::new(),
        }];
    }
    line_up_on_indented_line(
        source,
        cursor_byte,
        line_start,
        line_end,
        (&mut context, &mut placed),
        behavior,
    );
    // A dotted head typed in front of an entry of the table it is typed in
    // (`tls.c|path`, `path` a key of that table): the entry is not renamed.
    let head_segments = context.suffix_head_depth;
    let entry_of_table = head_segments > 0
        && context.path.len() >= head_segments
        && context.entry_after.as_deref().is_some_and(|word| {
            known_key(
                bundle,
                &context.path[..context.path.len() - head_segments],
                word,
                behavior,
            ) && !known_key(bundle, &context.path, word, behavior)
        });
    // The entry after the caret keeps a dotted head only when its key is
    // one under it (see `suffix_head`).
    if !context.suffix_head.is_empty()
        && let Some(word) = context.entry_after.clone()
        && known_key(bundle, &context.path, &word, behavior)
    {
        let head = std::mem::take(&mut context.suffix_head);
        context.entry_suffix.push_str(&head);
    }
    if replace_word {
        // The word is replaced, not filtered by: the selected part and the
        // rest of it after the caret (`«pa»th`).
        context.typed = String::new();
    }
    // A space typed after a complete word ends it: nothing is offered unless
    // asked for, so Enter starts a new line.
    if !force && source[..cursor_byte].ends_with([' ', '\t']) && !context.typed.trim().is_empty() {
        return Vec::new();
    }
    if !force
        && context.value_key.is_some()
        && source[..cursor_byte].ends_with([' ', '\t'])
        && line_value_before(source, cursor_byte, behavior)
    {
        return Vec::new();
    }
    let mut options = completions_for_context(
        source,
        cursor_byte,
        (&context, &placed),
        force,
        bundle,
        behavior,
        names,
    );
    let replaces_word =
        replace_word || (!entry_of_table && replaces_word_after_caret(&context, bundle, behavior));
    // Typed in front of a key, that key is a sibling already there.
    if !replaces_word && let Some(word) = context.entry_after.as_deref() {
        options.retain(|option| option.is_value || option.label != word);
    }
    let mut other_places = OtherPlaces::new(source, &context, behavior);
    for option in &mut options {
        let renamed = frame_insert(option, &context, &placed, replaces_word);
        // A whole key replaced: every other place it is written follows.
        let names_a_key = !option.is_value || context.position == EntryPosition::Name;
        let elsewhere = if names_a_key {
            other_places.of(option)
        } else {
            Vec::new()
        };
        let renamed_elsewhere = !elsewhere.is_empty();
        if renamed || renamed_elsewhere {
            // The key replaced whole is renamed: just its name is written.
            if !renamed {
                let lead = option
                    .insert
                    .find(option.label.as_str())
                    .map_or("", |at| &option.insert[..at])
                    .to_string();
                option.insert = format!("{lead}{}", option.label);
                option.cursor_offset = TextSize::of(option.insert.as_str());
                option.selected = TextSize::default();
                option.renames = true;
            }
            let mut paired_names = placed.paired_names.clone();
            for place in elsewhere {
                if !paired_names.contains(&place) {
                    paired_names.push(place);
                }
            }
            // A rename keeps what the insert writes before the name (`<`)
            // only where the document has it there too (not in `</name>`).
            if let Some(at) = option.insert.find(option.label.as_str())
                && at > 0
                && !slice::after(source, option.start)
                    .is_some_and(|rest| rest.starts_with(&option.insert[..at]))
            {
                option.insert.drain(..at);
                option.cursor_offset = option
                    .cursor_offset
                    .checked_sub(text_size(at))
                    .unwrap_or_default();
            }
            // The key's other places get the new name too: the insert runs
            // through them.
            // Places before it (an end tag's start tag) come first.
            let mut before: Vec<TextRange> = paired_names
                .iter()
                .copied()
                .filter(|place| place.end() <= option.start)
                .collect();
            before.sort_unstable_by_key(|place| (place.start(), place.end()));
            for place in before.into_iter().rev() {
                let between = between(source, place.end(), option.start);
                let prefix = format!("{}{between}", option.label);
                option.cursor_offset += TextSize::of(prefix.as_str());
                option.insert.insert_str(0, &prefix);
                option.start = place.start();
            }
            let mut places: Vec<TextRange> = paired_names
                .iter()
                .copied()
                .filter(|place| place.start() >= option.end)
                .collect();
            places.sort_unstable_by_key(|place| (place.start(), place.end()));
            for place in places {
                if place.start() < option.end {
                    continue;
                }
                option
                    .insert
                    .push_str(between(source, option.end, place.start()));
                option.insert.push_str(&option.label);
                option.end = place.end();
            }
        }
        // Typing in front of a key (`"|tcp"`) inserts; it never renames it.
        let in_front = !replaces_word
            && context
                .entry_after
                .as_deref()
                .is_some_and(|word| !word.is_empty());
        // A new list item (`[| { a = 1 }]`) renames nothing: no key is there.
        if !in_front && !renamed_elsewhere && context.entry_wrap.is_none() {
            rename_key_with_value(option, source, at, behavior);
        }
        // The key already written, the caret at its end and the key going on
        // (`tcp|.nodelay`, `tcp|: …`, `tcp| {`): accepting changes nothing.
        let rest = &source[cursor_byte..];
        if !option.is_value && option.label == context.typed && behavior.value_follows_key(rest) {
            option.insert = option.label.clone();
            option.end = at;
            option.start = at - TextSize::of(option.label.as_str()).min(at);
            option.cursor_offset = TextSize::of(option.insert.as_str());
            option.selected = TextSize::default();
            option.renames = true;
        }
        // A key typed after an auto-paired quote (`"pub|"`) whose insert is
        // written without quotes takes the closing quote along.
        let quotes = quote_starts(behavior);
        if !option.is_value
            && option.end == at
            && let Some(quote) = between(source, option.start.min(at), at)
                .chars()
                .next()
                .filter(|first| quotes.contains(first))
            && !option.insert.starts_with(quote)
            && source[cursor_byte..].starts_with(quote)
        {
            option.end += TextSize::of(quote);
        }
        // A value accepted between quotes it is written without (an XML
        // attribute's `mode="|"`) takes the closing quote along, so the caret
        // ends after it, as values written with their quotes do.
        if option.is_value
            && let Some(quote) =
                slice::before(source, option.start).and_then(|head| head.chars().next_back())
            && quotes.contains(&quote)
            && slice::after(source, option.end).is_some_and(|rest| rest.starts_with(quote))
            && !option.insert.contains(quote)
        {
            option.insert.push(quote);
            option.end += TextSize::of(quote);
            option.cursor_offset = TextSize::of(option.insert.as_str());
        }
        // An insert ending in a line break before an existing one would leave
        // a blank line, unless the caret goes on that line (`[pool]\n|`).
        let caret_on_new_line = option.cursor_offset == TextSize::of(option.insert.as_str());
        if option.insert.ends_with('\n')
            && slice::after(source, option.end).is_some_and(|rest| rest.starts_with('\n'))
            && !caret_on_new_line
        {
            option.insert.pop();
            option.cursor_offset = option
                .cursor_offset
                .min(TextSize::of(option.insert.as_str()));
        }
    }
    options
}

/// On a line holding only indentation and the word being typed: an entry
/// indented deeper than one can be there lines up with what Enter after the
/// line above writes, and indentation left after the caret goes, so an
/// accepted key neither breaks the document nor leaves trailing spaces.
fn line_up_on_indented_line(
    source: &str,
    cursor_byte: usize,
    line_start: usize,
    line_end: usize,
    (context, placed): (&mut CompletionContext, &mut Placed),
    behavior: &dyn ConfigLanguage,
) {
    let typed_start = usize::from(placed.replace.start());
    if context.value_key.is_some()
        || context.is_array_item
        || typed_start < line_start
        || typed_start > cursor_byte
        || !source[line_start..typed_start].trim().is_empty()
        || !source[cursor_byte..line_end].trim().is_empty()
    {
        return;
    }
    if placed.replace.end() == text_size(cursor_byte) {
        placed.replace = TextRange::new(placed.replace.start(), text_size(line_end));
    }
    if line_start == 0 {
        return;
    }
    let newline = behavior.newline(source, utf16_of(source, text_size(line_start - 1)));
    let written = &newline.text[..newline.caret.min(newline.text.len())];
    let target = written.rsplit('\n').next().unwrap_or_default();
    // Only after a sibling (the line above at that indentation): a first
    // entry may sit deeper under its opener, as the document's style.
    let above = source[..line_start - 1]
        .rsplit('\n')
        .next()
        .unwrap_or_default();
    let above_indent = &above[..above.len() - above.trim_start_matches([' ', '\t']).len()];
    // A line above that ends at its separator (`server:`) opens what
    // follows: the caret's line is its first entry, however the language's
    // Enter would indent (one without brackets keeps the line's indent).
    let above_code = above[..behavior.comment_start(above).unwrap_or(above.len())].trim_end();
    if behavior
        .config()
        .separators
        .iter()
        .any(|separator| above_code.ends_with(separator))
    {
        return;
    }
    if !target.trim().is_empty()
        || typed_start - line_start <= target.len()
        || above_indent != target
    {
        return;
    }
    placed.replace = TextRange::new(text_size(line_start), placed.replace.end());
    context.entry_prefix = format!("{target}{}", context.entry_prefix);
    context.base_indent = target.to_string();
}

/// Adds the context's entry prefix and suffix to an accepted insert, and lays
/// out a multi-line insert per `context.block`.
/// Whether the schema has `key` among the properties at `path`.
fn known_key(
    bundle: &SchemaBundle,
    path: &[SchemaPathSegment],
    key: &str,
    behavior: &dyn ConfigLanguage,
) -> bool {
    declares(&blind_branches(bundle, path, behavior), key)
}

/// The schemas at `path`, whatever the document holds.
fn blind_branches<'a>(
    bundle: &'a SchemaBundle,
    path: &[SchemaPathSegment],
    behavior: &dyn ConfigLanguage,
) -> Vec<&'a Value> {
    Applies::blind(
        &bundle.schema,
        crate::editor::data_reading::lone_items(behavior),
    )
    .at(path)
    .branches()
}

/// Whether one of `branches` names `key` as a property.
fn declares(branches: &[&Value], key: &str) -> bool {
    branches.iter().any(|branch| {
        branch
            .get("properties")
            .and_then(Value::as_object)
            .is_some_and(|properties| properties.contains_key(key))
    })
}

/// Whether a schema among `branches` describes an object.
fn has_object(branches: &[&Value]) -> bool {
    branches.iter().any(|branch| describes_object(branch))
}

/// Whether a schema among `branches` describes an array.
fn has_array(branches: &[&Value]) -> bool {
    branches.iter().any(|branch| describes_array(branch))
}

/// Whether the word after the caret (`cp` of `t|cp`) is the rest of the key
/// being edited, so a suggestion replaces the whole key, rather than an
/// existing key the user is typing in front of (`k|nodelay`). Only a word
/// that is a known key on its own, where the whole one is not, is the
/// latter.
fn replaces_word_after_caret(
    context: &CompletionContext,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> bool {
    // At a key's start the user types in front of it.
    let Some(word) = context.entry_after.as_deref().filter(|word| {
        !word.is_empty()
            && !context
                .typed
                .trim_start_matches(quote_starts(behavior).as_slice())
                .is_empty()
    }) else {
        return false;
    };
    let branches = blind_branches(bundle, &context.path, behavior);
    let known = |name: &str| declares(&branches, name);
    // A JSON key's typed part starts with its quote.
    let whole = format!(
        "{}{word}",
        context
            .typed
            .trim_start_matches(quote_starts(behavior).as_slice())
    );
    !(known(word) && !known(&whole))
}

/// `option` written into its context: prefixes and suffixes, a block, or a
/// rename of the key the caret is in (`replaces_word`, see
/// [`replaces_word_after_caret`]).
///
/// Returns whether the option renames that key.
fn frame_insert(
    option: &mut Completion,
    context: &CompletionContext,
    placed: &Placed,
    replaces_word: bool,
) -> bool {
    let expand = !option.is_value && option.insert.contains('\n');
    let mut renamed = false;
    match context.block.as_ref().zip(placed.block).filter(|_| expand) {
        Some((block, range)) => {
            let prefix = format!(
                "{}\n{}",
                context.entry_prefix.trim_end_matches([' ', '\t']),
                block.indent
            );
            option.insert.insert_str(0, &prefix);
            option.insert.push_str(&block.after);
            option.cursor_offset += TextSize::of(prefix.as_str());
            option.start = range.start();
            option.end = range.end();
        }
        None if context.names_only
            || context.entry_after.as_deref().is_some_and(|word| {
                !word.is_empty() && (replaces_word || option.label.ends_with(word))
            }) =>
        {
            // The suggestion completes the key the caret is inside: rename it,
            // keeping the entry's existing separator and value.
            option.end += context
                .entry_after
                .as_deref()
                .map_or(TextSize::default(), TextSize::of);
            // What the insert writes before the name stays (XML's `<`).
            let lead = option
                .insert
                .find(option.label.as_str())
                .map_or("", |at| &option.insert[..at]);
            option.insert = format!("{lead}{}", option.label);
            option.cursor_offset = TextSize::of(option.insert.as_str());
            // A rename writes no value: nothing of it is selected, and the
            // list does not reopen on it.
            option.selected = TextSize::default();
            option.renames = true;
            renamed = true;
        }
        None => {
            option.insert.insert_str(0, &context.entry_prefix);
            // A suffix starting a new line would double an insert's own break.
            if context.entry_suffix.starts_with('\n') && option.insert.ends_with('\n') {
                option.insert.pop();
            }
            if let Some(range) = placed.insert_range {
                option.start = range.start();
                option.end = range.end();
            }
            let before_suffix = TextSize::of(option.insert.as_str());
            option.insert.push_str(&context.entry_suffix);
            option.cursor_offset += TextSize::of(context.entry_prefix.as_str());
            if context.suffix_caret > 0 {
                let into_suffix = text_size(context.suffix_caret.min(context.entry_suffix.len()));
                option.cursor_offset = before_suffix + into_suffix;
                // In the new entry's body: its keys are offered next.
                option.is_value = false;
            }
        }
    }
    renamed
}

pub(super) fn completions_for_context(
    source: &str,
    cursor_byte: usize,
    (context, placed): (&CompletionContext, &Placed),
    force: bool,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
    (names, kept): (&dyn KeyNameSource, Option<&KeptKeys>),
) -> Vec<Completion> {
    let around = Around {
        read: std::cell::OnceCell::new(),
        source,
        cursor_byte,
        placed,
        behavior,
        names: NameLookup {
            identity: &bundle.identity,
            source: names,
            kept,
        },
    };
    // Read only where a step of the path is decided by the document (a
    // conditional, `oneOf`/`anyOf` options), see `applies`.
    let document = || around.document();
    let applies = Applies::new(
        &bundle.schema,
        crate::editor::data_reading::lone_items(behavior),
        &document,
    );
    completions_around(&around, &applies, context, force, bundle)
}

/// The document around the caret ([`document_around`]: a parse of the
/// whole text), read once for a suggestion run and only when something
/// asks for it.
struct Around<'s> {
    read: std::cell::OnceCell<Option<Value>>,
    source: &'s str,
    cursor_byte: usize,
    /// The places the run's context names, as bytes.
    placed: &'s Placed,
    behavior: &'s dyn ConfigLanguage,
    /// Where the names of `x-config-editor-keys` fields come from.
    names: NameLookup<'s>,
}

impl Around<'_> {
    fn document(&self) -> Option<&Value> {
        self.read
            .get_or_init(|| document_around(self.source, self.cursor_byte, self.behavior))
            .as_ref()
    }

    /// The keys the object at `path` has around the caret, the entry
    /// being typed left out; `None` where the document around the caret
    /// does not tell (it does not read, or no object is there).
    fn keys_at(&self, path: &[SchemaPathSegment]) -> Option<HashSet<String>> {
        let object = super::super::applies::value_at(self.document()?, path)?.as_object()?;
        Some(object.keys().cloned().collect())
    }
}

fn completions_around(
    around: &Around<'_>,
    applies: &Applies<'_, '_>,
    context: &CompletionContext,
    force: bool,
    bundle: &SchemaBundle,
) -> Vec<Completion> {
    let (source, cursor_byte, behavior) = (around.source, around.cursor_byte, around.behavior);
    let placed = around.placed;
    let here = applies.at(&context.path);
    let branches = here.branches();
    let read = || around.document();
    let names = |taken: HashSet<String>| ValueNames {
        lookup: around.names,
        document: &read,
        taken,
    };
    // An empty document under a schema whose root is a list: its first
    // entry opens the list's first item.
    if let Some(options) = root_values(
        (context, placed),
        &branches,
        applies,
        behavior,
        &names(HashSet::new()),
    ) {
        return options;
    }
    if let Some((open, close, indent)) = context.root_item
        && context.value_key.is_none()
        && !has_object(&branches)
        && has_array(&branches)
    {
        let item = CompletionContext {
            path: vec![SchemaPathSegment::ArrayItem(0)],
            entry_wrap: Some((open, close)),
            base_indent: format!("{}{indent}", context.base_indent),
            root_item: None,
            ..context.clone()
        };
        return completions_around(around, applies, &item, force, bundle);
    }
    if let Some(key) = context.value_key.as_deref() {
        // A number being typed is the user's own value: suggesting a longer
        // one (`1` → `10`) would make Enter replace it. Ctrl+Space still lists them.
        let typed_value = unquoted(behavior, &context.typed);
        if !force && !typed_value.is_empty() && typed_value.parse::<f64>().is_ok() {
            return Vec::new();
        }
        let value_path = path_with_key(&context.path, key);
        let value_here = applies.at(&value_path);
        let value_branches = value_here.branches();
        // A value written after a key is the key's own in a language that
        // writes only blocks as a list's items (HCL): no scalar there is
        // one of its list's items.
        let scalar_here = if behavior.repetition() == Repetition::Blocks {
            std::rc::Rc::new(Here::walk(
                applies
                    .at(&context.path)
                    .child(&SchemaPathSegment::Key(key.to_string())),
                &bundle.schema,
                Seen::value(applies.value_at(&value_path)),
            ))
        } else {
            value_here.clone()
        };
        let leading = if context.value_needs_space { " " } else { "" };
        let mut options = value_completions(
            &scalar_here,
            &context.typed,
            placed.replace,
            &bundle.schema,
            leading,
            force || typed_value.is_empty(),
            behavior,
            &names(HashSet::new()),
        );
        if !force && unquoted(behavior, &context.typed).is_empty() {
            options.retain(|option| option.label != "null");
        }
        if context.value_or_children && (options.is_empty() || has_object(&value_branches)) {
            let children = CompletionContext {
                path: path_with_key(&context.path, key),
                value_key: None,
                value_or_children: false,
                explicit_entry_start: true,
                ..context.clone()
            };
            return completions_around(around, applies, &children, force, bundle);
        }
        return options;
    }
    // An element of a list written by repeating it (XML's `<members>|`)
    // is an item of the list, as a list's marked item is.
    let repeated_item = behavior.repetition() == Repetition::Elements
        && context.value_key.is_none()
        && matches!(context.path.last(), Some(SchemaPathSegment::ArrayItem(_)));
    if context.is_array_item || repeated_item {
        let has_object_properties = branches.iter().any(|branch| {
            branch.get("properties").is_some()
                || branch.get("patternProperties").is_some()
                || type_includes(branch, "object")
        });
        if !has_object_properties {
            let mut options = value_completions(
                &here,
                &context.typed,
                placed.replace,
                &bundle.schema,
                "",
                force || unquoted(behavior, &context.typed).is_empty(),
                behavior,
                &names(list_items_written(applies, &context.path)),
            );
            separate_from_next_item(
                &mut options,
                source,
                cursor_byte,
                (context, placed),
                behavior,
            );
            return options;
        }
    }
    let query = unquoted(behavior, &context.typed);
    if query.is_empty() && !force && !context.explicit_entry_start {
        return Vec::new();
    }
    // A query holding a separator, a comment or a line break names no key.
    let config = behavior.config();
    if query.contains('\n')
        || config
            .separators
            .iter()
            .chain(config.line_comments)
            .any(|marker| !marker.is_empty() && query.contains(marker))
    {
        return Vec::new();
    }
    if context.no_properties {
        return Vec::new();
    }
    // A new list item (`[|, {…}]`) has no keys yet: the item the document
    // has at its index is the one after it. One its line starts with the
    // language's item marker (`- |` over `  image: x`) is there already.
    let line_start = source[..cursor_byte].rfind('\n').map_or(0, |at| at + 1);
    let marked_item = super::super::flow_syntax::Flow::of(behavior)
        .after_item_marker(source[line_start..].trim_start())
        .is_some();
    let existing_keys = if context.is_array_item && !marked_item {
        None
    } else {
        around.keys_at(&context.path)
    };
    // Keys are typed into an object: its options are narrowed by the keys
    // its text has, and by its value where the document gives it.
    let no_keys = HashSet::new();
    let object = applies.at_object(&context.path, existing_keys.as_ref().unwrap_or(&no_keys));
    let unit = indent_unit(source);
    let base_indent = context
        .block
        .as_ref()
        .map_or(context.base_indent.as_str(), |block| block.indent.as_str());
    let mut options = property_completions(
        &object,
        query,
        placed.replace,
        &bundle.schema,
        existing_keys.as_ref(),
        behavior.repetition(),
        &InsertContext {
            base_indent,
            unit: &unit,
            position: context.position,
            path: &context.path,
            array_item: context.is_array_item,
            another_item: false,
            attributes_marked: false,
        },
        behavior,
        applies.value_at(&context.path),
        &|| behavior.rewritable_keys(source, cursor_byte, &context.path),
        &names(HashSet::new()),
    );
    // A key typed directly in an array starts a new object item, which needs
    // its delimiters: `[|]` becomes `[{ hosts = [] }]`, not `[hosts = []]`.
    if let Some((open, close)) = context.entry_wrap {
        for option in &mut options {
            option.insert = format!("{open}{}{close}", option.insert);
            option.cursor_offset += TextSize::of(open);
        }
        separate_from_next_item(
            &mut options,
            source,
            cursor_byte,
            (context, placed),
            behavior,
        );
    }
    options
}

/// The text items the list holding the item at `path` has written (its
/// last step an index), compared as text: a name already in the list is
/// not offered for another of its items.
fn list_items_written(applies: &Applies<'_, '_>, path: &[SchemaPathSegment]) -> HashSet<String> {
    let Some((SchemaPathSegment::ArrayItem(_), list)) = path.split_last() else {
        return HashSet::new();
    };
    applies
        .value_at(list)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| !item.is_object() && !item.is_array())
                .map(crate::editor::applies::scalar_text)
                .collect()
        })
        .unwrap_or_default()
}

/// Accepting a key keeps the list open so its child keys can be suggested. Accepting
/// a value ends the entry, so the list closes instead of offering the same value again.
/// A key whose default is left selected is a placeholder: typing replaces it,
/// so there is nothing to suggest.
pub(crate) fn accept_reopens_suggestions(completion: &Completion, auto_trigger: bool) -> bool {
    auto_trigger
        && !completion.is_value
        && !completion.renames
        && completion.selected == TextSize::default()
}

/// The first character of each of the language's quotes.
fn quote_starts(behavior: &dyn ConfigLanguage) -> Vec<char> {
    behavior
        .config()
        .quotes
        .iter()
        .filter_map(|quote| quote.chars().next())
        .collect()
}

/// The text of `source` from byte `from` to byte `to`; empty when they do
/// not cut it (in that order, on character boundaries).
fn between(source: &str, from: TextSize, to: TextSize) -> &str {
    (from <= to)
        .then(|| slice::slice(source, TextRange::new(from, to)))
        .flatten()
        .unwrap_or_default()
}
