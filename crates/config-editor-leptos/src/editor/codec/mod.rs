//! Conversion between languages through the shared data model: XML to HCL is
//! `xml.parse` then `hcl.serialize`. A language implements only its own side
//! ([`ConfigLanguage::parse`] and [`ConfigLanguage::serialize`]).

mod canonical;
pub(in crate::editor) use canonical::canonical;
mod compare;
mod emptied_items;
mod empty_containers;
mod flow;
mod kept_inline;
mod key_order;
mod lines;
mod lists;
mod moves;
pub(super) mod non_finite;
mod removed_items;
mod renames;
mod reorder;
mod steps;

use compare::{canonical_number, comment_start, comparable};
#[cfg(test)]
#[allow(unused_imports, reason = "the save property tests read through it")]
pub(super) use emptied_items::without_emptied;
pub(super) use key_order::{patch_key_order, same_order};
pub(super) use lines::patch_structure;
pub(super) use moves::stages as move_stages;
pub use non_finite::{float_of, float_value, string_of, string_value};
pub(super) use removed_items::patch_removed_items;
pub(super) use renames::{patch_renames, patch_scalar_to_container, with_renames_undone};
pub(super) use reorder::{patch_reorder, unmoved_comments};
pub(super) use steps::steps;

use super::completion::conform_arrays;
use super::flow_syntax::Flow;
use super::language::{ConfigLanguage, ParseError, Repetition, Span};
use serde_json::Value;
use std::fmt;

/// A converted document, and the values the target language could not carry
/// exactly. Empty `losses` means the conversion round-trips.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Conversion {
    /// The document in the target language.
    pub text: String,
    /// JSON pointers of values the target reads back differently (for example
    /// an empty array, which XML cannot write).
    pub losses: Vec<String>,
}

/// Why a conversion failed.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConvertError {
    /// The source text does not parse in its language.
    Parse(ParseError),
    /// The target language cannot express the document (for example TOML has
    /// no `null`, and XML element names cannot hold `/docs`).
    Unsupported(String),
    /// The source holds text beside child elements (XML mixed content, at
    /// `pointer`), which no other language can show: converting it would
    /// change the document.
    MixedContent {
        /// The JSON pointer of the first element holding mixed content.
        pointer: String,
    },
}

impl fmt::Display for ConvertError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Parse(error) => write!(formatter, "{error}"),
            ConvertError::Unsupported(message) => write!(formatter, "{message}"),
            ConvertError::MixedContent { pointer } => write!(
                formatter,
                "{pointer}: text beside child elements cannot be shown in another language"
            ),
        }
    }
}

impl std::error::Error for ConvertError {}

/// Parses `source`, reading lone values as the arrays `schema` expects in a
/// language that repeats keys for arrays.
pub(super) fn read(
    language: &dyn ConfigLanguage,
    source: &str,
    schema: Option<&Value>,
) -> Result<Value, ParseError> {
    let mut value = super::parse_cache::parse(language, source)?;
    if let Some(schema) = schema.filter(|_| language.repetition() != Repetition::None) {
        conform_arrays(&mut value, schema, language.repetition());
    }
    Ok(value)
}

/// Whether `source` may be saved in `language`, its own: it reads, or
/// all that stands in the way are numbers the editor cannot hold exactly
/// that the language allows (see [`ConfigLanguage::unheld_number`]), each
/// read as `0` to find the next.
pub(super) fn saves_in_own_language(language: &dyn ConfigLanguage, source: &str) -> bool {
    let mut error = match read(language, source, None) {
        Ok(_) => return true,
        Err(error) => error,
    };
    let mut text = source.to_string();
    loop {
        let Some(range) = language.unheld_number(&text, &error) else {
            return false;
        };
        if range.is_empty() || text[range.clone()] == *"0" {
            return false;
        }
        text.replace_range(range, "0");
        // Read aside: the stand-in does not push out the text on screen.
        match language.parse(&text) {
            Ok(_) => return true,
            Err(next) => error = next,
        }
    }
}

/// Whether `text` reads back as `new`, its keys spelled as `new` spells
/// them (see [`ConfigLanguage::spelled_like`]): a patch's check.
pub(super) fn reads_as(
    language: &dyn ConfigLanguage,
    text: &str,
    schema: Option<&Value>,
    new: &Value,
) -> bool {
    read(language, text, schema).is_ok_and(|read| language.spelled_like(text, read, new) == *new)
}

/// [`read`] without the shared parse: a text read once on the side (a
/// stand-in for one that does not parse) does not push out the one on
/// screen.
#[cfg(feature = "protect")]
pub(super) fn read_aside(
    language: &dyn ConfigLanguage,
    source: &str,
    schema: Option<&Value>,
) -> Result<Value, ParseError> {
    let mut value = language.parse(source)?;
    if let Some(schema) = schema.filter(|_| language.repetition() != Repetition::None) {
        conform_arrays(&mut value, schema, language.repetition());
    }
    Ok(value)
}

/// Renders `value` in `target`, reading the result back to find the losses.
pub(super) fn render(
    value: &Value,
    target: &dyn ConfigLanguage,
    previous: Option<&str>,
    schema: Option<&Value>,
) -> Result<Conversion, ConvertError> {
    let text = serialize(target, value, previous)?;
    let reread = read(target, &text, schema).map_err(|error| {
        ConvertError::Unsupported(format!(
            "the converted document does not read back: {}",
            error.message
        ))
    })?;
    // Keys read two ways (an XML attribute `@b` with no child `b` reads
    // `b`) compared as `value` spells them.
    let reread = target.spelled_like(&text, reread, value);
    let mut losses = Vec::new();
    differences(value, &reread, String::new(), &mut losses);
    Ok(Conversion { text, losses })
}

/// Serializes through `language`, ending with exactly one newline.
pub(super) fn serialize(
    language: &dyn ConfigLanguage,
    value: &Value,
    previous: Option<&str>,
) -> Result<String, ConvertError> {
    // A language without infinities writes them as text, which reads back
    // as a string: a loss (see `non_finite`).
    let value = if language.writes_non_finite() {
        std::borrow::Cow::Borrowed(value)
    } else {
        non_finite::as_text(value)
    };
    let value = emptied_items::without_emptied(language, previous, value);
    let mut text = language
        .serialize(&value, previous)
        .map_err(ConvertError::Unsupported)?;
    text.truncate(text.trim_end_matches('\n').len());
    text.push('\n');
    Ok(text)
}

/// Serializes `value` in `language` only when the document reads back as
/// exactly `value`: the check `render` makes for a conversion's losses, so
/// writing data and converting a document agree on what a language holds.
pub(super) fn serialize_exact(
    language: &dyn ConfigLanguage,
    value: &Value,
) -> Result<String, ConvertError> {
    let Conversion { text, losses } = render(value, language, None, None)?;
    let Some((first, rest)) = losses.split_first() else {
        return Ok(text);
    };
    let more = if rest.is_empty() {
        String::new()
    } else {
        format!(", and {} more: {}", rest.len(), rest.join(", "))
    };
    Err(ConvertError::Unsupported(format!(
        "{} cannot write this value so that it reads back the same{more} (at {first})",
        language.info().label
    )))
}

pub(super) fn convert(
    source: &str,
    from: &dyn ConfigLanguage,
    to: &dyn ConfigLanguage,
    previous: Option<&str>,
    schema: Option<&Value>,
) -> Result<Conversion, ConvertError> {
    let value = read(from, source, schema).map_err(ConvertError::Parse)?;
    convertible(from, source)?;
    let conversion = render(&value, to, previous, schema)?;
    // Nor is a document converted to what would not convert back (text
    // beside child elements written from a `#text` key).
    convertible(to, &conversion.text)?;
    Ok(conversion)
}

/// [`convert`] for a caller who keeps the result (the headless
/// `convert`): the values whose tags the target has no place for (YAML's
/// `!Sub`), and those written as expressions (HCL's `var.z`, which the
/// target holds as text), are losses too. (Edit-as keeps the host's tags
/// and expressions itself.)
pub(super) fn convert_away(
    source: &str,
    from: &dyn ConfigLanguage,
    to: &dyn ConfigLanguage,
    schema: Option<&Value>,
) -> Result<Conversion, ConvertError> {
    let mut conversion = convert(source, from, to, None, schema)?;
    // The document is read alone, with no value to spell its keys like: a
    // key it reads back spelled otherwise (an `@a` the XML attribute `a`
    // stands for) is a loss.
    if let Ok(alone) = read(to, &conversion.text, schema) {
        let value = read(from, source, schema).map_err(ConvertError::Parse)?;
        let mut respelled = Vec::new();
        differences(&value, &alone, String::new(), &mut respelled);
        // Listed where the source has them (`/r/@a`, not the `/r/a` read).
        respelled.retain(|pointer| pointer == "/" || value.pointer(pointer).is_some());
        for pointer in respelled {
            if !conversion.losses.contains(&pointer) {
                conversion.losses.push(pointer);
            }
        }
    }
    let tagged = from.tags(source).into_iter().map(|(pointer, _)| pointer);
    for pointer in tagged.chain(from.expressions(source)) {
        if !conversion.losses.contains(&pointer) {
            conversion.losses.push(pointer);
        }
    }
    Ok(conversion)
}

/// Refuses a document holding what no other language can show (XML mixed
/// content, one key in two namespaces): converting it would change it.
pub(super) fn convertible(language: &dyn ConfigLanguage, source: &str) -> Result<(), ConvertError> {
    if let Some(pointer) = language.mixed_content(source) {
        return Err(ConvertError::MixedContent { pointer });
    }
    match language.unconvertible(source) {
        Some(reason) => Err(ConvertError::Unsupported(reason)),
        None => Ok(()),
    }
}

/// `previous` with only its changed scalar values rewritten in place, when
/// `old` and `new` have the same shape (same keys, same array lengths) and
/// every changed value can be located: comments, formatting, and key order
/// survive a value edit. `None` falls back to serializing the whole document.
pub(super) fn patch_scalars(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    // Whatever the language cannot express must still be refused, even when
    // a patch would read back equal.
    language.serialize(new, Some(previous)).ok()?;
    let mut changes = Vec::new();
    scalar_changes(old, new, String::new(), &mut changes)?;
    if changes.is_empty() {
        return Some(previous.to_string());
    }
    let mut edits = Vec::with_capacity(changes.len());
    for (pointer, value) in changes {
        // Values inside flow collections and inline tables are found by the
        // shared scanner when the language has no span for them.
        let find = |pointer: &str| {
            language
                .value_span(previous, pointer)
                .or_else(|| inline_location(language, previous, pointer, false))
        };
        // A lone block or element read as a one-item list is addressed
        // `…/0/…` but written without it.
        let span = find(&pointer).or_else(|| {
            (language.repetition() != Repetition::None && pointer.contains("/0")).then(|| {
                let lone = pointer.replace("/0/", "/");
                find(lone.strip_suffix("/0").unwrap_or(&lone))
            })?
        });
        // A value the host does not write there (a YAML merge key's) may
        // follow from another change (its anchor's): the read-back decides.
        let Some(span) = span else {
            continue;
        };
        let mut range = scalar_range(language, previous, span)?;
        // An alias (`*x`) reads its anchor's value, which the read-back
        // checks: the anchor's own change carries to it.
        let written = previous[range.clone()].trim();
        if language.reference_len(written) == Some(written.len()) {
            continue;
        }
        // In the host's style for what was there (plain, quoted, a block, a
        // long string).
        let mut text = language.format_value_at(value, previous, range.clone());
        // A value the host wrote in quotes keeps them when the new one is
        // formatted bare, where quotes delimit and do not type it (an XML
        // attribute's `"…"`; a TOML string made a number loses them).
        let written = &previous[range.clone()];
        let flow = Flow::of(language);
        if language.scalars_are_text()
            && let Some(quote) = written
                .chars()
                .next()
                .filter(|quote| flow.quote(*quote).is_some())
            && written.len() >= 2 * quote.len_utf8()
            && written.ends_with(quote)
            && !written.starts_with(&quote.to_string().repeat(3))
            && !flow.starts_quoted(&text)
        {
            text = format!("{quote}{text}{quote}");
        }
        // A value written across lines (`key: >- # note` then its text) may
        // hold its first line's comment; the one-line value keeps it.
        let line_start = previous[..range.start]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let line_end = previous[range.start..]
            .find('\n')
            .map_or(previous.len(), |offset| range.start + offset);
        if range.end > line_end && !text.contains('\n') {
            let line = &previous[line_start..line_end];
            // With the spacing written before it.
            let comment = compare::spaced_trailing_comment(language, line);
            if line_start + line.len() - comment.len() > range.start && !comment.is_empty() {
                let gap = if comment.starts_with([' ', '\t']) {
                    ""
                } else {
                    " "
                };
                text = format!("{text}{gap}{comment}");
            }
        }
        // An emptied value takes the space before it along (`key:`).
        if text.is_empty() {
            while range.start > 0 && previous[..range.start].ends_with(' ') {
                range.start -= 1;
            }
        }
        edits.push((range, text));
    }
    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut patched = previous.to_string();
    for (range, text) in edits {
        patched.replace_range(range, &text);
    }
    // Trust the patch only if it reads back as exactly the new document.
    reads_as(language, &patched, schema, new).then_some(patched)
}

/// `previous` with a structural edit inside one inline container (a key
/// added to `tcp: {nodelay: true}`, an item to `allow = ["a"]`) applied by
/// rewriting just that container on its line, in the host's inline syntax.
/// Kept only if it reads back as `new`.
pub(super) fn patch_inline(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    let changed = changed_container(old, new, String::new())?;
    // The container that changed, or the nearest inline one around it.
    let mut pointer = changed;
    let mut ascended = false;
    loop {
        // Where indentation is structure, a root written `[a, b]` holding
        // the change is a flow collection as a nested one is; elsewhere
        // (JSON) the root's brackets are the document's own form, left to
        // the other patches.
        let document_form = ascended && pointer.is_empty() && !language.indent_is_structure();
        let span = language
            .value_span(previous, &pointer)
            .or_else(|| inline_location(language, previous, &pointer, false))
            .filter(|_| !document_form);
        if let Some(span) = span
            && let Some(patched) = replace_inline(language, previous, &span, new, &pointer)
            && reads_as(language, &patched, schema, new)
        {
            return Some(patched);
        }
        // A new container in a collection over lines that is the language's
        // block form (a JSON object) is written over lines too: left to the
        // line patch.
        let over_lines = span
            .and_then(|span| kept_inline::multi_line_list(language, previous, span))
            .zip(old.pointer(&pointer).zip(new.pointer(&pointer)))
            .is_some_and(|(range, (before, after))| {
                flow::adds_containers_over_lines(language, &previous[range], before, after)
            });
        let span_over_lines = span.filter(|_| !over_lines);
        // A list written one item per line (`allow = [` … `]`).
        if let Some(span) = span_over_lines
            && let Some(range) = kept_inline::multi_line_list(language, previous, span)
            && let Some(Value::Array(items)) = new.pointer(&pointer)
            && let Some(Value::Array(before)) = old.pointer(&pointer)
        {
            let unit = super::commands::indent_unit(previous);
            for empty_trailing in [true, false] {
                let Some(text) = lists::rewrite_list_lines(
                    language,
                    &previous[range.clone()],
                    before,
                    items,
                    &unit,
                    empty_trailing,
                ) else {
                    continue;
                };
                let mut patched = previous.to_string();
                patched.replace_range(range.clone(), &text);
                if reads_as(language, &patched, schema, new) {
                    return Some(patched);
                }
            }
        }
        // A flow map or object written one entry per line (`tcp: {` … `}`), its
        // entries kept on their lines (before restyling it whole).
        if let Some(span) = span_over_lines
            && let Some(range) = kept_inline::multi_line_list(language, previous, span)
            && let Some(Value::Object(entries)) = new.pointer(&pointer)
            && let Some(Value::Object(before)) = old.pointer(&pointer)
        {
            for lone_commas in [false, true] {
                let Some(text) = lists::rewrite_map_lines(
                    language,
                    &previous[range.clone()],
                    before,
                    entries,
                    lone_commas,
                ) else {
                    continue;
                };
                let mut patched = previous.to_string();
                patched.replace_range(range.clone(), &text);
                if reads_as(language, &patched, schema, new) {
                    return Some(patched);
                }
            }
        }
        // A flow collection over lines without comments in it (`{a: 1,\n
        // b: 2}`): restyled whole, its untouched items as written.
        if let Some(span) = span_over_lines
            && let Some(range) = kept_inline::multi_line_list(language, previous, span)
            && previous[range.clone()]
                .lines()
                .all(|line| comment_start(language, line) == line.len())
            && let Some(value) = new.pointer(&pointer)
            && let Some(text) = flow::restyle(language, value, &previous[range.clone()])
        {
            let mut patched = previous.to_string();
            patched.replace_range(range, &text);
            if reads_as(language, &patched, schema, new) {
                return Some(patched);
            }
        }
        // A one-line block or element (`tcp { nodelay = true }`).
        if !document_form
            && let Some(one_line) = language.one_line_container(previous, &pointer)
            && let Some(body) = new.pointer(&pointer).and_then(|value| {
                let line_start = previous[..one_line.range.start]
                    .rfind('\n')
                    .map_or(0, |at| at + 1);
                let indent: String = previous[line_start..]
                    .chars()
                    .take_while(|character| matches!(character, ' ' | '\t'))
                    .collect();
                language.one_line_body(value, &indent, &super::commands::indent_unit(previous))
            })
        {
            let mut patched = previous.to_string();
            patched.replace_range(one_line.value, &body);
            if reads_as(language, &patched, schema, new) {
                return Some(patched);
            }
        }
        // A container written inline over lines replaced by a value
        // (`tcp: {a: 1,\n  b: 2}` → `tcp: 7`): the value in its place.
        if let Some(span) = span
            && let Some(range) = kept_inline::multi_line_list(language, previous, span)
            && let Some(value) = new
                .pointer(&pointer)
                .filter(|value| !value.is_object() && !value.is_array())
        {
            let mut patched = previous.to_string();
            patched.replace_range(range, &language.format_value_like(value, ""));
            if reads_as(language, &patched, schema, new) {
                return Some(patched);
            }
        }
        // The root's own container was tried last (a YAML root written
        // `[a, b]`, found by the language's span of the root).
        if pointer.is_empty() {
            // A document that is one container on one line (compact JSON).
            let trimmed = previous.trim();
            let start = previous.len() - previous.trim_start().len();
            if !trimmed.contains('\n')
                && let Some(text) = flow::restyle(language, new, trimmed)
            {
                let mut patched = previous.to_string();
                patched.replace_range(start..start + trimmed.len(), &text);
                if reads_as(language, &patched, schema, new) {
                    return Some(patched);
                }
            }
            return None;
        }
        pointer.truncate(pointer.rfind('/')?);
        ascended = true;
    }
}

/// `key` written as a new entry of a collection whose entry `model_head`
/// (`"a": `, `a = `) shows how keys are written there: as the language
/// writes keys (YAML quotes `on`), in quotes where the model has them.
fn key_like(language: &dyn ConfigLanguage, key: &str, model_head: &str) -> Option<String> {
    let written = language.written_key("", "", key)?;
    let flow = Flow::of(language);
    Some(match model_head.chars().next() {
        Some(quote) if flow.quote(quote).is_some() && !flow.starts_quoted(&written) => {
            format!("{quote}{written}{quote}")
        }
        _ => written,
    })
}

/// Where the markup opening the key whose name starts at byte `name` of
/// `text` starts (XML's `<` of `<web>`), if the language writes one there.
pub(super) fn key_markup_start(
    language: &dyn ConfigLanguage,
    text: &str,
    name: usize,
) -> Option<usize> {
    super::flow_syntax::markup_before(language, text, name)
}

/// Whether `line` writes a reference as a value (`key: *p`, `- *p`):
/// after the item markers it starts with, or after a key's separator.
fn reference_on(language: &dyn ConfigLanguage, flow: &Flow, line: &str) -> bool {
    let mut code = line.trim_start();
    while let Some(rest) = flow.after_item_marker(code) {
        code = rest.trim_start();
    }
    language.reference_len(code).is_some()
        || flow.key_separator(code).is_some_and(|(at, length)| {
            language
                .reference_len(code[at + length..].trim_start())
                .is_some()
        })
}

/// The pointer of the deepest value holding every difference between `old`
/// and `new`.
fn changed_container(old: &Value, new: &Value, pointer: String) -> Option<String> {
    if old == new {
        return None;
    }
    let child = |key: &str| format!("{pointer}/{}", key.replace('~', "~0").replace('/', "~1"));
    match (old, new) {
        (Value::Object(old), Value::Object(new))
            if old.len() == new.len() && old.keys().all(|key| new.contains_key(key)) =>
        {
            let differing: Vec<&String> = old.keys().filter(|key| old[*key] != new[*key]).collect();
            match differing.as_slice() {
                [key] => changed_container(&old[*key], &new[*key], child(key)),
                _ => Some(pointer),
            }
        }
        (Value::Array(old), Value::Array(new)) if old.len() == new.len() => {
            let differing: Vec<usize> = (0..old.len())
                .filter(|index| old[*index] != new[*index])
                .collect();
            match differing.as_slice() {
                [index] => changed_container(&old[*index], &new[*index], child(&index.to_string())),
                _ => Some(pointer),
            }
        }
        _ => Some(pointer),
    }
}

/// `previous` with the inline container at `span` written as `new`'s value at
/// `pointer` (a container or a value), or `None` when the span holds no
/// one-line `{…}` or `[…]`.
fn replace_inline(
    language: &dyn ConfigLanguage,
    previous: &str,
    span: &Span,
    new: &Value,
    pointer: &str,
) -> Option<String> {
    let value = new.pointer(pointer)?;
    let line_start: usize = previous
        .split_inclusive('\n')
        .take(span.line.checked_sub(1)?)
        .map(str::len)
        .sum();
    let line = previous[line_start..].split('\n').next()?;
    let column_byte = |column: usize| {
        line.char_indices()
            .nth(column.saturating_sub(1))
            .map_or(line.len(), |(index, _)| index)
    };
    let start = column_byte(span.start_column);
    let start = start + language.value_properties_len(&line[start..]);
    let text = &line[start..];
    let flow = Flow::of(language);
    let open = text.chars().next()?;
    let close = flow.closer_of(open)?;
    let end = start + flow.matching_close(text)? + close.len_utf8();
    let written = &line[start..end];
    // An empty container where the language writes new ones in block
    // style below their key (YAML's `[]`, the only way it writes an empty
    // list) says nothing about the style its items take: they are written
    // as new ones are (a block list), unless the host writes others of its
    // kind filled on one line. Elsewhere (a TOML inline table, an HCL
    // object) the block form is another form, written in another place: it
    // stays on one line.
    let empty = written[open.len_utf8()..written.len() - close.len_utf8()]
        .trim()
        .is_empty();
    let filled = match value {
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        _ => false,
    };
    let at = line_start + start;
    if empty
        && filled
        && language.indent_is_structure()
        && !flow::writes_filled_on_one_line(language, previous, close, at..at + written.len())
        && language
            .serialize(value, None)
            .is_ok_and(|fresh| !fresh.trim_start().starts_with(open))
    {
        return None;
    }
    let mut patched = previous.to_string();
    // A container replaced by a value (`tcp: {a: 1}` → `tcp: 7`) is that
    // value in its place.
    let text = if value.is_object() || value.is_array() {
        flow::restyle(language, value, written)
            .unwrap_or_else(|| language.format_value_like(value, written))
    } else {
        language.format_value_like(value, "")
    };
    patched.replace_range(line_start + start..line_start + end, &text);
    Some(patched)
}

/// The span of what `pointer` names when it lies inside an inline container
/// (a flow collection, inline table, or one-line object): the deepest
/// ancestor whose value is `{…}` or `[…]` is located by the language, the
/// rest by the shared scanner. `key` asks for the key rather than the value.
pub(super) fn inline_location(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    key: bool,
) -> Option<Span> {
    use super::language::pointer_segments;
    let segments = pointer_segments(pointer);
    for depth in (1..segments.len()).rev() {
        let ancestor: String = segments[..depth]
            .iter()
            .map(|segment| format!("/{}", escape(segment)))
            .collect();
        let Some(span) = super::paths::memo_value_span(behavior, source, &ancestor) else {
            continue;
        };
        return in_flow_container(behavior, source, span, &segments[depth..], key);
    }
    None
}

/// The byte offset of the inline container (`{…}` or `[…]`) written at
/// value `span`, past its properties (a YAML tag or anchor); `None` when
/// the value there is no inline container.
fn inline_start(behavior: &dyn ConfigLanguage, source: &str, span: Span) -> Option<usize> {
    let line_start = super::paths::line_start(source, span.line.saturating_sub(1));
    let start = line_start
        + source[line_start..]
            .char_indices()
            .nth(span.start_column.saturating_sub(1))
            .map_or(0, |(index, _)| index);
    let start = start + behavior.value_properties_len(&source[start..]);
    let flow = Flow::of(behavior);
    source[start..]
        .chars()
        .next()
        .is_some_and(|open| flow.closer_of(open).is_some())
        .then_some(start)
}

/// Whether the value written at `span` is an inline container (`[]`,
/// `{ a = 1 }`): a list written as one has no lone item to stand for it.
pub(super) fn written_inline(behavior: &dyn ConfigLanguage, source: &str, span: Span) -> bool {
    inline_start(behavior, source, span).is_some()
}

/// The span of what `rest` names inside the inline container whose value
/// is written at `span` (`None` when no `{…}` or `[…]` is there): its key
/// when `key`, else its value. A language whose root may be such a
/// container (YAML's `[a, b]`) finds the paths in it with this.
pub(super) fn in_flow_container(
    behavior: &dyn ConfigLanguage,
    source: &str,
    span: Span,
    rest: &[String],
    key: bool,
) -> Option<Span> {
    use super::language::SchemaPathSegment;
    let start = inline_start(behavior, source, span)?;
    let text = &source[start..];
    let rest: Vec<SchemaPathSegment> = rest
        .iter()
        .map(|segment| SchemaPathSegment::from_pointer_segment(segment.clone()))
        .collect();
    // While a batch runs for the text, each container walked is kept for
    // the other paths looked up in it.
    let walked = super::paths::in_batch(source)
        .then(|| super::paths::batch_kept(source, crate::editor::flow_syntax::Walked::default));
    let kept = walked
        .as_deref()
        .map(|walked| crate::editor::flow_syntax::Kept {
            walked,
            language: behavior.info().label,
            base: start,
        });
    let (from, to) = Flow::of(behavior).path_span_in(text, &rest, key, kept)?;
    let (from, to) = (start + from, start + to);
    // A comment after the value (`30 # b` in a flow map over lines) is
    // the language's, not the value's.
    let to = match behavior.comment_start(&source[from..to]) {
        Some(comment) if comment > 0 => from + source[from..from + comment].trim_end().len(),
        _ => to,
    };
    let (line, column) = super::paths::line_column(source, from);
    let to_column = if source[from..to].contains('\n') {
        column + 1
    } else {
        column + source[from..to].chars().count()
    };
    Some(Span {
        line,
        start_column: column,
        end_column: to_column.max(column + 1),
    })
}

/// Collects `(pointer, new value)` for every changed scalar; `None` when the
/// shape changed (keys added or removed, array lengths, types).
fn scalar_changes<'a>(
    old: &Value,
    new: &'a Value,
    pointer: String,
    out: &mut Vec<(String, &'a Value)>,
) -> Option<()> {
    match (old, new) {
        (Value::Object(left), Value::Object(right)) => {
            // Only values are rewritten in place, so key order does not matter.
            if left.len() != right.len() || !right.keys().all(|key| left.contains_key(key)) {
                return None;
            }
            for (key, value) in right {
                scalar_changes(&left[key], value, format!("{pointer}/{}", escape(key)), out)?;
            }
            Some(())
        }
        (Value::Array(left), Value::Array(right)) => {
            if left.len() != right.len() {
                return None;
            }
            for (index, (before, after)) in left.iter().zip(right).enumerate() {
                scalar_changes(before, after, format!("{pointer}/{index}"), out)?;
            }
            Some(())
        }
        (Value::Object(_) | Value::Array(_), _) | (_, Value::Object(_) | Value::Array(_)) => None,
        _ => {
            if old != new {
                out.push((pointer, new));
            }
            Some(())
        }
    }
}

/// The bytes of `source` the scalar whose value span is `span` is written
/// in, without its anchor or tag (`&x two`): what a patch rewrites, and
/// what reads as the value.
pub(super) fn scalar_range(
    language: &dyn ConfigLanguage,
    source: &str,
    span: Span,
) -> Option<std::ops::Range<usize>> {
    // A value never takes the line break after it (a heredoc's span runs
    // through its terminator line).
    let mut range = byte_range(source, span)?;
    while range.end > range.start && source[..range.end].ends_with('\n') {
        range.end -= 1;
    }
    // A string in one of the language's long quotes runs to its closing
    // delimiter.
    range.end = through_long_quote(language, source, range.clone());
    // With the whitespace around it that belongs to the value (XML text).
    range = language.written_value_range(source, range.clone());
    range.start += language.value_properties_len(&source[range.clone()]);
    Some(range)
}

/// The end of `range`, run through the closing delimiter when the value
/// starts with one of the language's multi-character quotes and spans
/// lines.
fn through_long_quote(
    language: &dyn ConfigLanguage,
    source: &str,
    range: std::ops::Range<usize>,
) -> usize {
    language
        .config()
        .quotes
        .iter()
        .filter(|quote| quote.len() > 1)
        .find(|quote| source[range.start..].starts_with(**quote))
        .and_then(|quote| {
            source[range.start + quote.len()..]
                .find(quote)
                .map(|close| range.start + 2 * quote.len() + close)
        })
        .map_or(range.end, |end| end.max(range.end))
}

/// The byte range of a one-based line/character-column span.
pub(super) fn byte_range(source: &str, span: Span) -> Option<std::ops::Range<usize>> {
    // (A lookup while a batch runs for the text; past its last line, the
    // empty text at its end.)
    let line_start = crate::editor::paths::line_start(source, span.line.saturating_sub(1));
    let line = &source[line_start..];
    let byte_of = |column: usize| {
        line.char_indices()
            .nth(column.saturating_sub(1))
            .map_or(line.len(), |(index, _)| index)
    };
    let (start, end) = (byte_of(span.start_column), byte_of(span.end_column));
    (start < end).then_some(line_start + start..line_start + end)
}

/// JSON pointers where `expected` and `actual` differ, at the deepest level
/// where both still have the same shape.
pub(super) fn differences(
    expected: &Value,
    actual: &Value,
    pointer: String,
    out: &mut Vec<String>,
) {
    match (expected, actual) {
        (Value::Object(left), Value::Object(right)) => {
            for (key, value) in left {
                let child = format!("{pointer}/{}", escape(key));
                match right.get(key) {
                    Some(other) => differences(value, other, child, out),
                    None => out.push(child),
                }
            }
            for key in right.keys().filter(|key| !left.contains_key(*key)) {
                out.push(format!("{pointer}/{}", escape(key)));
            }
        }
        (Value::Array(left), Value::Array(right)) if left.len() == right.len() => {
            for (index, (value, other)) in left.iter().zip(right).enumerate() {
                differences(value, other, format!("{pointer}/{index}"), out);
            }
        }
        _ if expected == actual => {}
        _ => out.push(if pointer.is_empty() {
            "/".to_string()
        } else {
            pointer
        }),
    }
}

/// The JSON pointer of the first value, depth-first, matching `predicate`;
/// lets a codec name exactly what it cannot express.
pub fn first_pointer(value: &Value, predicate: &dyn Fn(&Value) -> bool) -> Option<String> {
    if predicate(value) {
        return Some(String::new());
    }
    match value {
        Value::Object(map) => map.iter().find_map(|(key, child)| {
            first_pointer(child, predicate).map(|rest| format!("/{}{rest}", escape(key)))
        }),
        Value::Array(items) => items.iter().enumerate().find_map(|(index, child)| {
            first_pointer(child, predicate).map(|rest| format!("/{index}{rest}"))
        }),
        _ => None,
    }
}

/// `key` as one segment of a JSON pointer.
pub(super) fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

// Conversion tests pair languages, so they need every built-in one.
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
mod tests {
    use super::super::Language;
    use super::{ConvertError, convert, differences};
    use serde_json::json;

    /// Carried exactly by every language: arrays of two or more items (XML),
    /// no nulls (TOML), identifier keys.
    fn portable() -> serde_json::Value {
        json!({
            "listen": "0.0.0.0:7835",
            "insecure_plaintext": false,
            "pool": {"min_idle": 4, "max_idle": 64},
            "web": {"hosts": ["a.example", "b.example"], "letsencrypt": {"email": "ops@example.com"}},
            "clients": {"acme-prod": {"secret": "«secret 1»"}, "beta": {"secret": ["x", "y"]}},
            "reach": {}
        })
    }

    #[test]
    fn a_portable_document_round_trips_between_every_pair_of_languages() {
        let value = portable();
        for &from in Language::ALL {
            let source = from.serialize(&value).expect("serializes");
            assert_eq!(from.parse(&source).expect("reads back"), value, "{from:?}");
            for &to in Language::ALL {
                let converted = from.convert(&source, to).expect("converts");
                assert!(
                    converted.losses.is_empty(),
                    "{from:?} -> {to:?}: {:?}",
                    converted.losses
                );
                assert_eq!(
                    to.parse(&converted.text).expect("parses"),
                    value,
                    "{from:?} -> {to:?}"
                );
            }
        }
    }

    #[test]
    fn conversion_keeps_the_key_order_of_the_source() {
        let converted = Language::JSON
            .convert(r#"{"zeta": 1, "alpha": 2, "mid": 3}"#, Language::YAML)
            .expect("converts");
        assert_eq!(converted.text, "zeta: 1\nalpha: 2\nmid: 3\n");
    }

    #[test]
    fn a_source_syntax_error_is_reported_with_its_position() {
        let Err(ConvertError::Parse(error)) =
            Language::JSON.convert("{\n  \"a\": ,\n}", Language::YAML)
        else {
            panic!("a parse error");
        };
        assert_eq!(error.line, Some(2));
    }

    #[test]
    fn toml_names_the_null_it_cannot_express() {
        let Err(ConvertError::Unsupported(message)) =
            Language::JSON.convert(r#"{"web": {"https": null}}"#, Language::TOML)
        else {
            panic!("TOML rejects null");
        };
        assert!(message.contains("/web/https"), "{message}");
    }

    #[test]
    fn xml_reports_a_one_item_array_it_reads_back_as_a_scalar() {
        let converted = Language::JSON
            .convert(r#"{"hosts": ["only.example"], "port": 80}"#, Language::XML)
            .expect("converts");
        assert_eq!(
            converted.text,
            "<hosts>only.example</hosts>\n<port>80</port>\n"
        );
        assert_eq!(converted.losses, vec!["/hosts"]);
    }

    #[test]
    fn xml_keeps_its_config_root_and_declaration_from_the_previous_text() {
        let previous = "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>1</listen>\n</burrow>\n";
        let value = json!({"listen": 2, "pool": {"min_idle": 4}});
        let converted =
            super::render(&value, Language::XML.behavior(), Some(previous), None).expect("renders");
        assert_eq!(
            converted.text,
            "<?xml version=\"1.0\"?>\n<burrow config-root=\"true\">\n  <listen>2</listen>\n  <pool>\n    <min_idle>4</min_idle>\n  </pool>\n</burrow>\n"
        );
        assert!(converted.losses.is_empty());
    }

    #[test]
    fn json_keeps_the_indentation_of_the_previous_text() {
        let previous = "{\n    \"a\": 1\n}\n";
        let converted = convert(
            "b: 2\n",
            Language::YAML.behavior(),
            Language::JSON.behavior(),
            Some(previous),
            None,
        )
        .expect("converts");
        assert_eq!(converted.text, "{\n    \"b\": 2\n}\n");
    }

    #[test]
    fn hcl_writes_blocks_for_identifier_objects_and_quotes_other_keys() {
        let text = Language::HCL
            .serialize(&json!({"web": {"http": 80}, "names": {"old.example": "new.example"}}))
            .expect("serializes");
        assert_eq!(
            text,
            "web {\n  http = 80\n}\n\nnames = {\n  \"old.example\" = \"new.example\"\n}\n"
        );
    }

    #[test]
    fn hcl_escapes_template_markers_in_strings() {
        let value = json!({"token": "${CLOUDFLARE_API_TOKEN}"});
        let text = Language::HCL.serialize(&value).expect("serializes");
        assert_eq!(Language::HCL.parse(&text).expect("reads back"), value);
    }

    #[test]
    fn differences_point_at_the_deepest_changed_value() {
        let mut out = Vec::new();
        differences(
            &json!({"a": {"b": [1, 2], "c": "x"}, "d/e": 1}),
            &json!({"a": {"b": 1, "c": "x"}, "d/e": 2, "f": 0}),
            String::new(),
            &mut out,
        );
        assert_eq!(out, vec!["/a/b", "/d~1e", "/f"]);
    }

    #[test]
    fn identical_documents_have_no_differences() {
        let mut out = Vec::new();
        let value = json!({"a": [1, {"b": null}]});
        differences(&value, &value, String::new(), &mut out);
        assert!(out.is_empty());
    }
}
