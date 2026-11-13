//! The suggestions for a caret position: keys and values the schema
//! allows there, framed for the language.

use std::ops::Range;

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
    let query = unquoted(behavior, typed);
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    // A language without null (TOML) is not offered one.
    let null_writable = behavior
        .serialize(&serde_json::json!({ "value": null }), None)
        .is_ok();
    let numeric_allowed = schemas
        .iter()
        .flat_map(|schema| resolve_branches(schema, root))
        .any(|branch| type_includes(branch, "integer") || type_includes(branch, "number"));
    for schema in schemas {
        for branch in resolve_branches(schema, root) {
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
                    // The value's own words, else its branch's.
                    let detail = value_doc(branch, &value)
                        .and_then(|doc| doc.detail())
                        .unwrap_or_else(|| description.to_string());
                    candidates.push((
                        score,
                        Completion {
                            label: label.clone(),
                            detail,
                            type_label: type_label.clone(),
                            cursor_offset: insert.encode_utf16().count(),
                            insert,
                            start,
                            end,
                            selected: 0,
                            is_value: true,
                            renames: false,
                            matched: match_ranges(&label, query),
                            deprecated: false,
                            fills: Vec::new(),
                            closes_tag: false,
                            read_only: false,
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
    behavior: &dyn ConfigLanguage,
) -> (usize, usize) {
    let unselected = (cursor_offset, 0);
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
    let before = &insert[..utf16_to_byte_index(insert, cursor_offset)];
    let length = text.encode_utf16().count();
    if before.ends_with(&text) {
        return (cursor_offset, length);
    }
    // The caret is after the closing quote: select inside the quotes.
    let quotes = quote_starts(behavior);
    if let Some(quoted) = before.strip_suffix(quotes.as_slice())
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
    choices: &ChoicesAt<'_>,
    instance: Option<&Value>,
) -> Vec<Completion> {
    let mut candidates = Vec::new();
    for branch in schemas
        .iter()
        .flat_map(|schema| branches_for(schema, root, instance))
    {
        let Some(properties) = branch.get("properties").and_then(Value::as_object) else {
            continue;
        };
        // A key of `oneOf`/`anyOf` options names them (the matched one
        // alone when it has the key); once an option matches, another
        // option's keys are not offered (adding one would leave it).
        let choice = choices.of(branch);
        if choices.matched.is_some() && choice.is_some_and(|(_, matched)| !matched) {
            continue;
        }
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
                + bonus
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
                default_selection(&insert, cursor_offset, property, root, behavior);
            let holding = choices.holding(key);
            let (title, type_label) = if holding.is_empty() {
                (None, schema_type_label(property, root, "property"))
            } else {
                let schemas: Vec<&Value> = holding.iter().map(|(_, schema)| *schema).collect();
                (
                    choices.titles_holding(key),
                    type_label_of_all(&schemas, root, "property"),
                )
            };
            candidates.push((
                score,
                Completion {
                    label: key.clone(),
                    detail: ValueDoc {
                        title,
                        description: property
                            .get("description")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                    }
                    .detail()
                    .unwrap_or_default(),
                    type_label,
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
                    deprecated: is_deprecated(property, root),
                    fills: required_fills(behavior, property, root),
                    closes_tag: false,
                    read_only: false,
                },
            ));
        }
    }
    sorted_completions(candidates)
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
    completions_in(
        source,
        cursor,
        force,
        schema,
        enabled,
        behavior,
        replace_word,
        &bodies,
    )
}

/// [`completions_for`] with the text's multi-line string bodies already
/// found (`ConfigLanguage::string_body_lines`): the live editor keeps them
/// in a memo, so a keystroke does not parse the whole document for them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn completions_in(
    source: &str,
    cursor: usize,
    force: bool,
    schema: Option<&SchemaBundle>,
    enabled: bool,
    behavior: &dyn ConfigLanguage,
    replace_word: bool,
    string_bodies: &[Range<usize>],
) -> Vec<Completion> {
    if !enabled {
        return Vec::new();
    }
    let Some(bundle) = schema else {
        return Vec::new();
    };
    // Nothing is suggested inside a comment or a multi-line string.
    let line = source[..utf16_to_byte_index(source, cursor)]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count();
    if super::super::parse_cache::lexical_state(behavior, source, cursor).in_comment
        || string_bodies.iter().any(|lines| lines.contains(&line))
    {
        return Vec::new();
    }
    let cursor_byte = utf16_to_byte_index(source, cursor);
    // In a line's indentation, before its text: as where the text starts,
    // so a new entry lines up with it instead of splitting its indent.
    let line_start = source[..cursor_byte].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[cursor_byte..]
        .find('\n')
        .map_or(source.len(), |at| cursor_byte + at);
    let indent_end = line_start + source[line_start..line_end].len()
        - source[line_start..line_end].trim_start().len();
    if cursor_byte < indent_end && indent_end < line_end {
        let text_start = source[..indent_end].encode_utf16().count();
        return completions_in(
            source,
            text_start,
            force,
            schema,
            enabled,
            behavior,
            replace_word,
            string_bodies,
        );
    }
    let mut context = super::super::parse_cache::completion_context(behavior, source, cursor);
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
            read_only: false,
            cursor_offset: insert.encode_utf16().count(),
            insert,
            start: context.replace_start,
            end: context.replace_end,
            selected: 0,
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
        &mut context,
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
    let mut options =
        completions_for_context(source, cursor_byte, &context, force, bundle, behavior);
    let replaces_word =
        replace_word || (!entry_of_table && replaces_word_after_caret(&context, bundle, behavior));
    // Typed in front of a key, that key is a sibling already there.
    if !replaces_word && let Some(word) = context.entry_after.as_deref() {
        options.retain(|option| option.is_value || option.label != word);
    }
    for option in &mut options {
        let renamed = frame_insert(option, &context, replaces_word);
        // A whole key replaced: every other place it is written follows.
        let names_a_key = !option.is_value || context.position == EntryPosition::Name;
        let elsewhere = if names_a_key {
            other_places(source, &context, option, behavior)
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
                option.cursor_offset = option.insert.encode_utf16().count();
                option.selected = 0;
                option.renames = true;
            }
            let mut context = context.clone();
            for place in elsewhere {
                if !context.paired_names.contains(&place) {
                    context.paired_names.push(place);
                }
            }
            // A rename keeps what the insert writes before the name (`<`)
            // only where the document has it there too (not in `</name>`).
            let start_byte = utf16_to_byte_index(source, option.start);
            if let Some(at) = option.insert.find(option.label.as_str())
                && at > 0
                && !source[start_byte..].starts_with(&option.insert[..at])
            {
                let lead = option.insert[..at].encode_utf16().count();
                option.insert.drain(..at);
                option.cursor_offset = option.cursor_offset.saturating_sub(lead);
            }
            // The key's other places get the new name too: the insert runs
            // through them.
            // Places before it (an end tag's start tag) come first.
            let mut before: Vec<(usize, usize)> = context
                .paired_names
                .iter()
                .copied()
                .filter(|(_, end)| *end <= option.start)
                .collect();
            before.sort_unstable();
            for (name_start, name_end) in before.into_iter().rev() {
                let between_start = utf16_to_byte_index(source, name_end);
                let between_end = utf16_to_byte_index(source, option.start);
                let prefix = format!("{}{}", option.label, &source[between_start..between_end]);
                option.cursor_offset += prefix.encode_utf16().count();
                option.insert.insert_str(0, &prefix);
                option.start = name_start;
            }
            let mut places: Vec<(usize, usize)> = context
                .paired_names
                .iter()
                .copied()
                .filter(|(start, _)| *start >= option.end)
                .collect();
            places.sort_unstable();
            for (name_start, name_end) in places {
                if name_start < option.end {
                    continue;
                }
                let between_start = utf16_to_byte_index(source, option.end);
                let between_end = utf16_to_byte_index(source, name_start);
                option.insert.push_str(&source[between_start..between_end]);
                option.insert.push_str(&option.label);
                option.end = name_end;
            }
        }
        // Typing in front of a key (`"|tcp"`) inserts; it never renames it.
        let in_front = !replaces_word
            && context
                .entry_after
                .as_deref()
                .is_some_and(|word| !word.is_empty());
        if !in_front && !renamed_elsewhere {
            rename_key_with_value(option, source, cursor, behavior);
        }
        // The key already written, the caret at its end and the key going on
        // (`tcp|.nodelay`, `tcp|: …`, `tcp| {`): accepting changes nothing.
        let rest = &source[cursor_byte..];
        if !option.is_value && option.label == context.typed && behavior.value_follows_key(rest) {
            option.insert = option.label.clone();
            option.end = cursor;
            option.start = cursor - option.label.encode_utf16().count().min(cursor);
            option.cursor_offset = option.insert.encode_utf16().count();
            option.selected = 0;
            option.renames = true;
        }
        // A key typed after an auto-paired quote (`"pub|"`) whose insert is
        // written without quotes takes the closing quote along.
        let typed_start = utf16_to_byte_index(source, option.start);
        let quotes = quote_starts(behavior);
        if !option.is_value
            && option.end == cursor
            && let Some(quote) = source[typed_start.min(cursor_byte)..cursor_byte]
                .chars()
                .next()
                .filter(|first| quotes.contains(first))
            && !option.insert.starts_with(quote)
            && source[cursor_byte..].starts_with(quote)
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
                && quotes.contains(&quote)
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

/// Where else the key the option replaces is written (UTF-16 ranges), per
/// the language's [`ConfigLanguage::key_renames`]: other dotted keys and
/// headers through it, other blocks or elements of its name. Empty unless
/// the replaced text is a whole key written in the document.
fn other_places(
    source: &str,
    context: &CompletionContext,
    option: &Completion,
    behavior: &dyn ConfigLanguage,
) -> Vec<(usize, usize)> {
    const STAND_IN: &str = "configeditorrenamed";
    let stop = |character: char| character.is_whitespace() || "\"'.=:[]{}<>/,#".contains(character);
    let start = utf16_to_byte_index(source, option.start);
    let end = utf16_to_byte_index(source, option.end);
    let replaced = source[start..end].trim_start_matches(|character: char| stop(character));
    let word_start = end - replaced.len();
    let word_len = replaced.find(stop).unwrap_or(replaced.len());
    let old = &replaced[..word_len];
    if old.is_empty() || old == option.label {
        return Vec::new();
    }
    let mut pointer = String::new();
    for segment in &context.path {
        pointer.push('/');
        match segment {
            SchemaPathSegment::Key(key) => {
                pointer.push_str(&key.replace('~', "~0").replace('/', "~1"))
            }
            SchemaPathSegment::ArrayItem(index) => pointer.push_str(&index.to_string()),
        }
    }
    pointer.push('/');
    pointer.push_str(&old.replace('~', "~0").replace('/', "~1"));
    let Some(renamed) = behavior
        .key_renames(source, &pointer, STAND_IN)
        .map(|edits| crate::editor::language::with_edits(source, edits))
    else {
        return Vec::new();
    };
    // Each stand-in maps back to the word it replaced.
    let mut places = Vec::new();
    let mut delta: isize = 0;
    for (at, _) in renamed.match_indices(STAND_IN) {
        let Some(place) = at.checked_add_signed(-delta) else {
            return Vec::new();
        };
        let length = source
            .get(place..)
            .map_or(0, |rest| rest.find(stop).unwrap_or(rest.len()));
        if source.get(place..place + length) != Some(old) {
            return Vec::new();
        }
        delta += STAND_IN.len() as isize - length as isize;
        places.push((place, place + length));
    }
    // Only a key written where the caret is: a new key is not a rename.
    if !places.iter().any(|(place, _)| *place == word_start) {
        return Vec::new();
    }
    let utf16 = |byte: usize| source[..byte].encode_utf16().count();
    places
        .into_iter()
        .filter(|(place, _)| *place != word_start)
        .map(|(place, place_end)| (utf16(place), utf16(place_end)))
        .collect()
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
    context: &mut CompletionContext,
    behavior: &dyn ConfigLanguage,
) {
    let typed_start = utf16_to_byte_index(source, context.replace_start);
    if context.value_key.is_some()
        || context.is_array_item
        || typed_start < line_start
        || typed_start > cursor_byte
        || !source[line_start..typed_start].trim().is_empty()
        || !source[cursor_byte..line_end].trim().is_empty()
    {
        return;
    }
    if context.replace_end == source[..cursor_byte].encode_utf16().count() {
        context.replace_end = source[..line_end].encode_utf16().count();
    }
    if line_start == 0 {
        return;
    }
    let previous_end = source[..line_start - 1].encode_utf16().count();
    let newline = behavior.newline(source, previous_end);
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
    context.replace_start = source[..line_start].encode_utf16().count();
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
    schema_at_path(
        &bundle.schema,
        path,
        behavior.repetition() != Repetition::None,
    )
    .iter()
    .any(|schema| {
        object_branch(schema, &bundle.schema)
            .and_then(|branch| branch.get("properties"))
            .and_then(Value::as_object)
            .is_some_and(|properties| properties.contains_key(key))
    })
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
            // A rename writes no value: nothing of it is selected, and the
            // list does not reopen on it.
            option.selected = 0;
            option.renames = true;
            renamed = true;
        }
        None => {
            option.insert.insert_str(0, &context.entry_prefix);
            // A suffix starting a new line would double an insert's own break.
            if context.entry_suffix.starts_with('\n') && option.insert.ends_with('\n') {
                option.insert.pop();
            }
            if let Some((start, end)) = context.insert_range {
                option.start = start;
                option.end = end;
            }
            let before_suffix = option.insert.encode_utf16().count();
            option.insert.push_str(&context.entry_suffix);
            option.cursor_offset += context.entry_prefix.encode_utf16().count();
            if context.suffix_caret > 0 {
                let into_suffix = context.entry_suffix
                    [..context.suffix_caret.min(context.entry_suffix.len())]
                    .encode_utf16()
                    .count();
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
    context: &CompletionContext,
    force: bool,
    bundle: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> Vec<Completion> {
    // A conditional schema follows the document: read it only then.
    let document = bundle
        .has_conditionals
        .then(|| document_around(source, cursor_byte, behavior))
        .flatten();
    let document = document.as_ref();
    let schemas = schema_at_path_in(
        &bundle.schema,
        &context.path,
        behavior.repetition() != Repetition::None,
        document,
    );

    if let Some(key) = context.value_key.as_deref() {
        // A number being typed is the user's own value: suggesting a longer
        // one (`1` → `10`) would make Enter replace it. Ctrl+Space still lists them.
        let typed_value = unquoted(behavior, &context.typed);
        if !force && !typed_value.is_empty() && typed_value.parse::<f64>().is_ok() {
            return Vec::new();
        }
        let value_schemas = schema_at_path_in(
            &bundle.schema,
            &path_with_key(&context.path, key),
            behavior.repetition() != Repetition::None,
            document,
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
        if !force && unquoted(behavior, &context.typed).is_empty() {
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
            .flat_map(|schema| resolve_branches(schema, &bundle.schema))
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
                force || unquoted(behavior, &context.typed).is_empty(),
                behavior,
            );
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
    let existing_keys = behavior.existing_keys(source, cursor_byte, &context.path);
    let choices = ChoicesAt::new(
        &schemas,
        &bundle.schema,
        existing_keys.as_ref().unwrap_or(&HashSet::new()),
        || object_around(source, cursor_byte, &context.path, behavior),
    );
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
        &choices,
        document.and_then(|document| value_at(document, &context.path)),
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

/// The first character of each of the language's quotes.
fn quote_starts(behavior: &dyn ConfigLanguage) -> Vec<char> {
    behavior
        .config()
        .quotes
        .iter()
        .filter_map(|quote| quote.chars().next())
        .collect()
}
