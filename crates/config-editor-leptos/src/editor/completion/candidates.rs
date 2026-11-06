//! The suggestions for a caret position: keys and values the schema
//! allows there, framed for the language.

use super::*;

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

#[allow(clippy::too_many_arguments)]
pub(super) fn value_completions(
    schemas: &[&Value],
    typed: &str,
    start: usize,
    end: usize,
    root: &Value,
    leading: &str,
    open: bool,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    let query = unquoted(typed);
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    // A language without null (TOML) is not offered one.
    let null_writable = behavior
        .serialize(&serde_json::json!({ "value": null }), None)
        .is_ok();
    let numeric_allowed = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
        .any(|branch| type_includes(branch, "integer") || type_includes(branch, "number"));
    for schema in schemas {
        for branch in resolve_branches(schema, root, 0) {
            let description = branch
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let type_label = schema_type_label(branch, root, "value");
            for value in schema_value_candidates(branch, open) {
                let value = numeric_example_as_number(value, numeric_allowed);
                let label = value_label(&value);
                if !seen.insert(label.clone()) || (value.is_null() && !null_writable) {
                    continue;
                }
                if let Some(score) = match_score(&label, query) {
                    let insert = format!("{leading}{}", behavior.format_value(&value));
                    candidates.push((
                        score,
                        Completion {
                            label: label.clone(),
                            detail: description.to_string(),
                            type_label: type_label.clone(),
                            cursor_offset: insert.encode_utf16().count(),
                            insert,
                            start,
                            end,
                            selected: 0,
                            is_value: true,
                            renames: false,
                            matched: match_ranges(&label, query),
                        },
                    ));
                }
            }
        }
    }
    sorted_completions(candidates)
}

/// `(cursor_offset, selected)` for an inserted key: a scalar default the
/// insert wrote just before the caret (`max_pending: 512|`, `mode = "fail|"`)
/// is selected, so typing replaces it instead of appending to it.
pub(super) fn default_selection(
    insert: &str,
    cursor_offset: usize,
    property: &Value,
    root: &Value,
) -> (usize, usize) {
    let unselected = (cursor_offset, 0);
    let Some(default) = schema_default(property, root)
        .or_else(|| schema_const(property, root))
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
    let before = &insert[..utf16_to_byte_index(insert, cursor_offset)];
    let length = text.encode_utf16().count();
    if before.ends_with(&text) {
        return (cursor_offset, length);
    }
    // The caret is after the closing quote: select inside the quotes.
    if let Some(quoted) = before.strip_suffix(['"', '\''])
        && quoted.ends_with(&text)
    {
        return (cursor_offset - 1, length);
    }
    unselected
}

#[allow(clippy::too_many_arguments)]
pub(super) fn property_completions(
    schemas: &[&Value],
    typed: &str,
    start: usize,
    end: usize,
    root: &Value,
    existing: Option<&HashSet<String>>,
    repetition: Repetition,
    insert_context: &InsertContext<'_>,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    let mut candidates = Vec::new();
    for branch in schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root, 0))
    {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        let required: HashSet<&str> = branch
            .get("required")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        for (key, property) in properties {
            // A list key written by repetition can appear again: any list in
            // XML, a list of objects as HCL blocks (never inside `{ … }`).
            let repeatable = match repetition {
                Repetition::Elements => array_branch(property, root).is_some(),
                Repetition::Blocks => {
                    insert_context.position == EntryPosition::Block
                        && array_branch(property, root)
                            .and_then(|array| array.get("items"))
                            .is_some_and(|items| object_branch(items, root).is_some())
                }
                _ => false,
            };
            if !repeatable && existing.is_some_and(|existing| existing.contains(key)) {
                continue;
            }
            let Some(score) = match_score(key, typed) else {
                continue;
            };
            let score = score
                + if required.contains(key.as_str()) {
                    REQUIRED_BONUS
                } else {
                    0
                };
            let Some((insert, cursor_offset)) =
                behavior.property_insert(key, property, root, insert_context)
            else {
                continue;
            };
            let (cursor_offset, selected) =
                default_selection(&insert, cursor_offset, property, root);
            candidates.push((
                score,
                Completion {
                    label: key.clone(),
                    detail: property
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    type_label: schema_type_label(property, root, "property"),
                    insert,
                    start,
                    end,
                    cursor_offset,
                    selected,
                    // A name alone (a table header segment) completes the
                    // entry, so the list does not reopen on it.
                    is_value: insert_context.position == EntryPosition::Name,
                    renames: false,
                    matched: match_ranges(key, typed),
                },
            ));
        }
    }
    sorted_completions(candidates)
}

pub(crate) fn completions(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    if !enabled {
        return Vec::new();
    }
    let Some(bundle) = schema else {
        return Vec::new();
    };
    // Nothing is suggested inside a comment or a multi-line string.
    if behavior.lexical_state(source, cursor).in_comment
        || crate::editor::language::in_string_body(behavior, source, cursor)
    {
        return Vec::new();
    }
    let cursor_byte = utf16_to_byte_index(source, cursor);
    let context = behavior.completion_context(source, cursor);
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
    let mut options =
        completions_for_context(source, cursor_byte, &context, force, bundle, behavior);
    let replaces_word = replaces_word_after_caret(&context, bundle, behavior);
    // Typed in front of a key, that key is a sibling already there.
    if !replaces_word && let Some(word) = context.entry_after.as_deref() {
        options.retain(|option| option.is_value || option.label != word);
    }
    for option in &mut options {
        if frame_insert(option, &context, replaces_word)
            && let Some((name_start, name_end)) = context.paired_name
            && name_start >= option.end
        {
            // The end tag gets the new name too: the insert runs through it.
            let between_start = utf16_to_byte_index(source, option.end);
            let between_end = utf16_to_byte_index(source, name_start);
            option.insert.push_str(&source[between_start..between_end]);
            option.insert.push_str(&option.label);
            option.end = name_end;
        }
        rename_key_with_value(option, source, cursor, behavior);
        // A key typed after an auto-paired quote (`"pub|"`) whose insert is
        // written without quotes takes the closing quote along.
        let typed_start = utf16_to_byte_index(source, option.start);
        if !option.is_value
            && option.end == cursor
            && source[typed_start.min(cursor_byte)..cursor_byte].starts_with('"')
            && !option.insert.starts_with('"')
            && source[cursor_byte..].starts_with('"')
        {
            option.end += 1;
        }
        // A value accepted between quotes it is written without (an XML
        // attribute's `mode="|"`) takes the closing quote along, so the caret
        // ends after it, as values written with their quotes do.
        if option.is_value {
            let start_byte = utf16_to_byte_index(source, option.start);
            let end_byte = utf16_to_byte_index(source, option.end);
            if let Some(quote) = source[..start_byte].chars().next_back()
                && matches!(quote, '"' | '\'')
                && source[end_byte..].starts_with(quote)
                && !option.insert.contains(quote)
            {
                option.insert.push(quote);
                option.end += 1;
                option.cursor_offset = option.insert.encode_utf16().count();
            }
        }
        // An insert ending in a line break before an existing one would leave
        // a blank line, unless the caret goes on that line (`[pool]\n|`).
        let end = utf16_to_byte_index(source, option.end);
        let caret_on_new_line = option.cursor_offset == option.insert.encode_utf16().count();
        if option.insert.ends_with('\n') && source[end..].starts_with('\n') && !caret_on_new_line {
            option.insert.pop();
            let units = option.insert.encode_utf16().count();
            option.cursor_offset = option.cursor_offset.min(units);
        }
    }
    options
}

/// Adds the context's entry prefix and suffix to an accepted insert, and lays
/// out a multi-line insert per `context.block`.
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
    let Some(word) = context
        .entry_after
        .as_deref()
        .filter(|word| !word.is_empty() && !context.typed.is_empty())
    else {
        return false;
    };
    let schemas = schema_at_path(
        &bundle.schema,
        &context.path,
        behavior.repetition() != Repetition::None,
    );
    let known = |name: &str| {
        schemas.iter().any(|schema| {
            object_branch(schema, &bundle.schema)
                .and_then(|branch| branch.get("properties"))
                .and_then(Value::as_object)
                .is_some_and(|properties| properties.contains_key(name))
        })
    };
    let whole = format!("{}{word}", context.typed);
    !(known(word) && !known(&whole))
}

/// `option` written into its context: prefixes and suffixes, a block, or a
/// rename of the key the caret is in (`replaces_word`, see
/// [`replaces_word_after_caret`]).
///
/// Returns whether the option renames that key.
pub(crate) fn frame_insert(
    option: &mut Completion,
    context: &CompletionContext,
    replaces_word: bool,
) -> bool {
    let expand = !option.is_value && option.insert.contains('\n');
    let mut renamed = false;
    match context.block.as_ref().filter(|_| expand) {
        Some(block) => {
            let prefix = format!(
                "{}\n{}",
                context.entry_prefix.trim_end_matches([' ', '\t']),
                block.indent
            );
            option.insert.insert_str(0, &prefix);
            option.insert.push_str(&block.after);
            option.cursor_offset += prefix.encode_utf16().count();
            option.start = block.start;
            option.end = block.end;
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
                .map_or(0, |word| word.encode_utf16().count());
            // What the insert writes before the name stays (XML's `<`).
            let lead = option
                .insert
                .find(option.label.as_str())
                .map_or("", |at| &option.insert[..at]);
            option.insert = format!("{lead}{}", option.label);
            option.cursor_offset = option.insert.encode_utf16().count();
            renamed = true;
        }
        None => {
            option.insert.insert_str(0, &context.entry_prefix);
            // A suffix starting a new line would double an insert's own break.
            if context.entry_suffix.starts_with('\n') && option.insert.ends_with('\n') {
                option.insert.pop();
            }
            option.insert.push_str(&context.entry_suffix);
            option.cursor_offset += context.entry_prefix.encode_utf16().count();
        }
    }
    renamed
}

pub(super) fn completions_for_context(
    source: &str,
    cursor_byte: usize,
    context: &CompletionContext,
    force: bool,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    let schemas = schema_at_path(
        &bundle.schema,
        &context.path,
        behavior.repetition() != Repetition::None,
    );

    if let Some(key) = context.value_key.as_deref() {
        // A number being typed is the user's own value: suggesting a longer
        // one (`1` → `10`) would make Enter replace it. Ctrl+Space still lists them.
        let typed_value = unquoted(&context.typed);
        if !force && !typed_value.is_empty() && typed_value.parse::<f64>().is_ok() {
            return Vec::new();
        }
        let value_schemas = schema_at_path(
            &bundle.schema,
            &path_with_key(&context.path, key),
            behavior.repetition() != Repetition::None,
        );
        let leading = if context.value_needs_space { " " } else { "" };
        let mut options = value_completions(
            &value_schemas,
            &context.typed,
            context.replace_start,
            context.replace_end,
            &bundle.schema,
            leading,
            force || typed_value.is_empty(),
            behavior,
        );
        if !force && unquoted(&context.typed).is_empty() {
            options.retain(|option| option.label != "null");
        }
        let object_value = value_schemas
            .iter()
            .any(|schema| object_branch(schema, &bundle.schema).is_some());
        if context.value_or_children && (options.is_empty() || object_value) {
            let children = CompletionContext {
                path: path_with_key(&context.path, key),
                value_key: None,
                value_or_children: false,
                explicit_entry_start: true,
                ..context.clone()
            };
            return completions_for_context(
                source,
                cursor_byte,
                &children,
                force,
                bundle,
                behavior,
            );
        }
        return options;
    }

    if context.is_array_item {
        let has_object_properties = schemas
            .iter()
            .flat_map(|schema| resolve_branches(schema, &bundle.schema, 0))
            .any(|branch| {
                branch.get("properties").is_some()
                    || branch.get("patternProperties").is_some()
                    || type_includes(branch, "object")
            });
        if !has_object_properties {
            return value_completions(
                &schemas,
                &context.typed,
                context.replace_start,
                context.replace_end,
                &bundle.schema,
                "",
                force || unquoted(&context.typed).is_empty(),
                behavior,
            );
        }
    }

    let query = unquoted(&context.typed);
    if query.is_empty() && !force && !context.explicit_entry_start {
        return Vec::new();
    }
    if query.contains([':', '#', '\n']) {
        return Vec::new();
    }

    if context.no_properties {
        return Vec::new();
    }
    let existing_keys = behavior.existing_keys(source, cursor_byte, &context.path);
    let unit = indent_unit(source);
    let base_indent = context
        .block
        .as_ref()
        .map_or(context.base_indent.as_str(), |block| block.indent.as_str());
    let mut options = property_completions(
        &schemas,
        query,
        context.replace_start,
        context.replace_end,
        &bundle.schema,
        existing_keys.as_ref(),
        behavior.repetition(),
        &InsertContext {
            base_indent,
            unit: &unit,
            position: context.position,
            path: &context.path,
            array_item: context.is_array_item,
        },
        behavior,
    );
    // A key typed directly in an array starts a new object item, which needs
    // its delimiters: `[|]` becomes `[{ hosts = [] }]`, not `[hosts = []]`.
    if let Some((open, close)) = context.entry_wrap {
        for option in &mut options {
            option.insert = format!("{open}{}{close}", option.insert);
            option.cursor_offset += open.encode_utf16().count();
        }
    }
    options
}

/// Accepting a key keeps the list open so its child keys can be suggested. Accepting
/// a value ends the entry, so the list closes instead of offering the same value again.
/// A key whose default is left selected is a placeholder: typing replaces it,
/// so there is nothing to suggest.
pub(crate) fn accept_reopens_suggestions(completion: &Completion, auto_trigger: bool) -> bool {
    auto_trigger && !completion.is_value && !completion.renames && completion.selected == 0
}
