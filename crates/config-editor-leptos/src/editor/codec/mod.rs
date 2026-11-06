//! Conversion between languages through the shared data model: XML to HCL is
//! `xml.parse` then `hcl.serialize`. A language implements only its own side
//! ([`ConfigLanguage::parse`] and [`ConfigLanguage::serialize`]).

mod kept_inline;
mod lists;
mod renames;

pub(super) use renames::{patch_renames, patch_scalar_to_container};

use super::completion::conform_arrays;
use super::inline::matching_close;
use super::language::{ConfigLanguage, ParseError, Repetition, Span};
use serde_json::Value;
use std::fmt;

/// A converted document, and the values the target language could not carry
/// exactly. Empty `losses` means the conversion round-trips.
#[derive(Clone, Debug, PartialEq, Eq)]
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
}

impl fmt::Display for ConvertError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Parse(error) => write!(formatter, "{error}"),
            ConvertError::Unsupported(message) => write!(formatter, "{message}"),
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
    let mut text = language
        .serialize(value, previous)
        .map_err(ConvertError::Unsupported)?;
    text.truncate(text.trim_end_matches('\n').len());
    text.push('\n');
    Ok(text)
}

pub(super) fn convert(
    source: &str,
    from: &dyn ConfigLanguage,
    to: &dyn ConfigLanguage,
    previous: Option<&str>,
    schema: Option<&Value>,
) -> Result<Conversion, ConvertError> {
    let value = read(from, source, schema).map_err(ConvertError::Parse)?;
    render(&value, to, previous, schema)
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
    // a patch would read back equal (TOML writes null as `nan`).
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
        })?;
        // A value never takes the line break after it (a heredoc's span runs
        // through its terminator line).
        let mut range = byte_range(previous, span)?;
        while range.end > range.start && previous[..range.end].ends_with('\n') {
            range.end -= 1;
        }
        let mut text = language.format_value(value);
        // A triple-quoted string (`"""` … `"""`) runs to its closing
        // delimiter, and text with line breaks is written back that way.
        if let Some(delimiter) = triple_quote_at(previous, range.start) {
            range.end = through_triple_quote(previous, range.clone());
            if let Value::String(body) = value
                && body.contains('\n')
            {
                text = if delimiter == "'''" && !body.contains("'''") {
                    format!("'''\n{body}'''")
                } else {
                    format!(
                        "\"\"\"\n{}\"\"\"",
                        body.replace('\\', "\\\\").replace("\"\"\"", "\"\"\\\"")
                    )
                };
            }
        }
        // A value the host wrote in quotes keeps them when the new one is
        // formatted bare (an XML attribute's `"…"`).
        let written = &previous[range.clone()];
        if let Some(quote) = written
            .chars()
            .next()
            .filter(|quote| matches!(quote, '"' | '\''))
            && written.len() >= 2
            && written.ends_with(quote)
            && !text.starts_with(quote)
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
        if range.end > line_end {
            let line = &previous[line_start..line_end];
            if let Some(comment) = language
                .comment_start(line)
                .map(|start| line[start..].trim_end())
                && line_start + line.len() - comment.len() > range.start
                && !comment.is_empty()
            {
                text = format!("{text} {comment}");
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
    (read(language, &patched, schema).ok()? == *new).then_some(patched)
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
    loop {
        let span = language
            .value_span(previous, &pointer)
            .or_else(|| inline_location(language, previous, &pointer, false));
        if let Some(span) = span
            && let Some(patched) = replace_inline(language, previous, &span, new, &pointer)
            && read(language, &patched, schema).ok().as_ref() == Some(new)
        {
            return Some(patched);
        }
        // A list written one item per line (`allow = [` … `]`).
        if let Some(span) = span
            && let Some(range) = kept_inline::multi_line_list(language, previous, span)
            && let Some(Value::Array(items)) = new.pointer(&pointer)
            && let Some(Value::Array(before)) = old.pointer(&pointer)
            && let Some(text) =
                lists::rewrite_list_lines(language, &previous[range.clone()], before, items)
        {
            let mut patched = previous.to_string();
            patched.replace_range(range, &text);
            if read(language, &patched, schema).ok().as_ref() == Some(new) {
                return Some(patched);
            }
        }
        // A one-line block or element (`tcp { nodelay = true }`).
        if let Some(one_line) = language.one_line_container(previous, &pointer)
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
            if read(language, &patched, schema).ok().as_ref() == Some(new) {
                return Some(patched);
            }
        }
        let parent = pointer.rfind('/')?;
        if parent == 0 && pointer.is_empty() {
            return None;
        }
        pointer.truncate(parent);
        if pointer.is_empty() {
            return None;
        }
    }
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
/// `pointer`, or `None` when the span holds no one-line `{…}` or `[…]`.
fn replace_inline(
    language: &dyn ConfigLanguage,
    previous: &str,
    span: &Span,
    new: &Value,
    pointer: &str,
) -> Option<String> {
    let value = new.pointer(pointer)?;
    if !(value.is_object() || value.is_array()) {
        return None;
    }
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
    let text = &line[start..];
    let close = match text.chars().next()? {
        '{' => '}',
        '[' => ']',
        _ => return None,
    };
    let end = start + matching_close(text, close)? + 1;
    let mut patched = previous.to_string();
    let written = &line[start..end];
    patched.replace_range(
        line_start + start..line_start + end,
        &language.format_value_like(value, written),
    );
    Some(patched)
}

/// `previous` with a structural edit (keys or items added or removed) applied
/// line by line, keeping its comments and formatting elsewhere: the host
/// serializer renders the old and the new value, their line diff is mapped
/// onto `previous`, and the result is kept only if it reads back as `new`.
///
/// One-line containers the host wrote inline (`allow: [a, b]`) are kept as
/// written: the serializer would spread them over several lines that match
/// nothing in the host (see [`kept_inline`]).
pub(super) fn patch_structure(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
    schema: Option<&Value>,
) -> Option<String> {
    // An empty host is written whole: there is nothing to keep.
    if previous.trim().is_empty() {
        return None;
    }
    let reads_as_new = |text: &str| read(language, text, schema).ok().as_ref() == Some(new);
    if let Some(kept) = kept_inline::hide(language, previous, old, new)
        && let Some(patched) = patch_lines(language, &kept.host, &kept.old, &kept.new)
    {
        let restored = kept.restore(&patched);
        if reads_as_new(&restored) {
            return Some(restored);
        }
    }
    patch_lines(language, previous, old, new).filter(|patched| reads_as_new(patched))
}

/// [`patch_structure`]'s line diff, unchecked.
fn patch_lines(
    language: &dyn ConfigLanguage,
    previous: &str,
    old: &Value,
    new: &Value,
) -> Option<String> {
    use super::diff::{DiffEdit, myers_edits};
    let before = serialize(language, old, Some(previous)).ok()?;
    let after = serialize(language, new, Some(previous)).ok()?;
    let rendered_old: Vec<&str> = before.lines().collect();
    let rendered_new: Vec<&str> = after.lines().collect();
    let host: Vec<&str> = previous.lines().collect();
    // Where each line of the old rendering sits in the host's text: a
    // forward match that skips the host's comments and blank lines, and
    // compares lines without quotes or trailing comments.
    let normalized_host: Vec<String> = host.iter().map(|line| comparable(language, line)).collect();
    let mut place = vec![None; rendered_old.len()];
    let mut next = 0;
    for (index, line) in rendered_old.iter().enumerate() {
        let wanted = comparable(language, line);
        if wanted.is_empty() {
            continue;
        }
        // Look ahead a bounded distance so a missing line cannot make the
        // rest of the document unmatched. A line of only punctuation (`}`,
        // `],`) says nothing about where it is, so it matches only the next
        // line with code, never one further on.
        let punctuation = wanted
            .chars()
            .all(|character| matches!(character, '{' | '}' | '[' | ']' | '(' | ')' | ','));
        let window = if punctuation {
            (next..host.len())
                .find(|candidate| !normalized_host[*candidate].is_empty())
                .map_or(next, |candidate| candidate + 1)
        } else {
            (next + 400).min(host.len())
        };
        if let Some(found) = (next..window).find(|candidate| normalized_host[*candidate] == wanted)
        {
            place[index] = Some(found);
            next = found + 1;
        }
    }
    // The old → new edits, applied to the host's lines.
    let mut removed = vec![false; host.len()];
    let mut inserted: Vec<Vec<&str>> = vec![Vec::new(); host.len() + 1];
    let (mut old_index, mut new_index) = (0, 0);
    // The host line a run of deletions started at: lines inserted right
    // after them replace them (a renamed table) and go in their place.
    let mut replacing: Option<usize> = None;
    let mut edits = myers_edits(&rendered_old, &rendered_new)?;
    slide_deletions_up(&mut edits, &rendered_old);
    for edit in edits {
        match edit {
            DiffEdit::Equal => {
                old_index += 1;
                new_index += 1;
                replacing = None;
            }
            DiffEdit::Delete => {
                // A rendered line with no host line (a blank line the
                // serializer writes between tables) has nothing to remove;
                // a real mismatch is caught by the read-back below.
                if let Some(at) = place[old_index] {
                    removed[at] = true;
                    replacing.get_or_insert(at);
                }
                old_index += 1;
            }
            DiffEdit::Insert => {
                // Where the lines it replaces were; else after the host line
                // of the nearest placed rendered line above; at the top,
                // before the first placed line (after a header comment);
                // with nothing placed, at the end.
                let slot = match replacing {
                    Some(at) => at,
                    None => match place[..old_index].iter().rposition(Option::is_some) {
                        // Past the host's blank lines that match rendered
                        // ones in between, so the insert sits after them.
                        Some(placed) => {
                            let mut slot = place[placed].unwrap_or_default() + 1;
                            let blanks = rendered_old[placed + 1..old_index]
                                .iter()
                                .filter(|line| line.trim().is_empty())
                                .count();
                            for _ in 0..blanks {
                                if slot < host.len() && host[slot].trim().is_empty() {
                                    slot += 1;
                                }
                            }
                            slot
                        }
                        None => place.iter().find_map(|at| *at).unwrap_or(host.len()),
                    },
                };
                inserted[slot].push(rendered_new[new_index]);
                new_index += 1;
            }
        }
    }
    // A removed entry takes the comment lines directly above it (`# beta`
    // above a deleted `beta { … }`); a file's header comment is never taken.
    // Blank lines are tidied so no gap is left: the one above the comments
    // goes, or, under an opener (`clients {`), the one after the entry.
    for index in 0..host.len() {
        // A replaced line keeps the comment above it.
        if !removed[index] || index == 0 || removed[index - 1] || !inserted[index].is_empty() {
            continue;
        }
        let is_comment =
            |line: &str| !line.trim().is_empty() && comparable(language, line).is_empty();
        let mut top = index;
        while top > 0 && is_comment(host[top - 1]) {
            top -= 1;
        }
        // Comments at the very top are the file's header, unless they name
        // the entry right below them (`# tcp` over `tcp { … }`).
        if top == 0 {
            let key: String = host[index]
                .trim_start()
                .trim_start_matches(['<', '[', '"'])
                .chars()
                .take_while(|character| {
                    character.is_alphanumeric() || matches!(character, '_' | '-')
                })
                .collect::<String>()
                .to_lowercase();
            let names_it = !key.is_empty()
                && host[..index].iter().any(|line| {
                    line.to_lowercase()
                        .split(|character: char| {
                            !(character.is_alphanumeric() || matches!(character, '_' | '-'))
                        })
                        .any(|word| word == key)
                });
            if !names_it {
                continue;
            }
            for line in removed.iter_mut().take(index) {
                *line = true;
            }
            continue;
        }
        for line in removed.iter_mut().take(index).skip(top) {
            *line = true;
        }
        if host[top - 1].trim().is_empty() {
            if top < index {
                removed[top - 1] = true;
            }
        } else {
            let mut end = index;
            while end + 1 < host.len() && removed[end + 1] {
                end += 1;
            }
            if top < index && end + 1 < host.len() && host[end + 1].trim().is_empty() {
                removed[end + 1] = true;
            }
        }
    }
    drop_doubled_blank_lines(&host, &mut removed, &inserted);
    follow_sibling_spacing(&host, &mut inserted);
    let mut inserted: Vec<Vec<String>> = inserted
        .into_iter()
        .map(|slot| slot.into_iter().map(str::to_string).collect())
        .collect();
    // A replaced line (a renamed key) keeps its trailing comment: the
    // comment moves to the line inserted in its place at the same indent.
    for index in 0..host.len() {
        if !removed[index] {
            continue;
        }
        let comment = trailing_comment(language, host[index]);
        let indent = |line: &str| line.len() - line.trim_start().len();
        if let Some(first) = inserted[index].first_mut()
            && !comment.is_empty()
            && indent(first) == indent(host[index])
            && trailing_comment(language, first).is_empty()
        {
            first.push(' ');
            first.push_str(comment);
        }
    }
    let mut lines: Vec<&str> = Vec::with_capacity(host.len() + rendered_new.len());
    for index in 0..=host.len() {
        lines.extend(inserted[index].iter().map(String::as_str));
        if index < host.len() && !removed[index] {
            lines.push(host[index]);
        }
    }
    let mut patched = lines.join("\n");
    if previous.ends_with('\n') {
        patched.push('\n');
    }
    Some(patched)
}

/// A removed run of lines between two blank lines (or after a blank line at
/// the end) would leave the blank lines doubled: the one above goes.
fn drop_doubled_blank_lines(host: &[&str], removed: &mut [bool], inserted: &[Vec<&str>]) {
    let blank = |index: usize| host[index].trim().is_empty();
    let mut index = 0;
    while index < host.len() {
        if !removed[index] {
            index += 1;
            continue;
        }
        let start = index;
        while index < host.len() && removed[index] {
            index += 1;
        }
        let untouched = (start..=index).all(|slot| inserted[slot].is_empty());
        // A closer (`}`, `]`, `</a>`) after the run ends the body: no blank
        // line belongs before it either.
        let closes = |line: &str| {
            let trimmed = line.trim();
            trimmed.starts_with(['}', ']', ')']) || trimmed.starts_with("</")
        };
        let after_blank = index == host.len() || blank(index) || closes(host[index]);
        if untouched && start > 0 && !removed[start - 1] && blank(start - 1) && after_blank {
            removed[start - 1] = true;
        }
        // Removed from the top of the file: no blank line left to start it.
        if untouched && start == 0 && index < host.len() && blank(index) {
            removed[index] = true;
        }
    }
}

/// An inserted entry starting with a blank line (as the serializer separates
/// blocks) keeps it only where the host separates siblings that way: the
/// earlier sibling at the slot's indent ending its block is followed by a
/// blank line there.
fn follow_sibling_spacing(host: &[&str], inserted: &mut [Vec<&str>]) {
    let indent = |line: &str| line.len() - line.trim_start().len();
    for slot in 1..inserted.len() {
        let starts_blank = inserted[slot]
            .first()
            .is_some_and(|line| line.trim().is_empty());
        let before = host[slot - 1];
        if !starts_blank || before.trim().is_empty() {
            continue;
        }
        let level = indent(before);
        // The closer of the sibling before `before`'s block, if any.
        let mut earlier = None;
        for index in (0..slot - 1).rev() {
            let line = host[index];
            if line.trim().is_empty() {
                continue;
            }
            if indent(line) < level {
                break;
            }
            if indent(line) == level && line.trim().starts_with(['}', ']']) {
                earlier = Some(index);
                break;
            }
        }
        if let Some(index) = earlier
            && host
                .get(index + 1)
                .is_some_and(|line| !line.trim().is_empty())
        {
            inserted[slot].remove(0);
        }
    }
}

/// Shifts each run of deleted lines up while the line before it equals its
/// last line, so a repeated block (`[[web.certificates]]` items) is deleted
/// whole from its first line rather than straddling two items. Closing
/// brackets stay where the diff put them: they end the block above.
fn slide_deletions_up(edits: &mut [super::diff::DiffEdit], old: &[&str]) {
    use super::diff::DiffEdit;
    let mut changed = true;
    while changed {
        changed = false;
        let mut old_index = 0;
        let mut index = 0;
        while index < edits.len() {
            if edits[index] == DiffEdit::Equal {
                let run = edits[index + 1..]
                    .iter()
                    .take_while(|edit| **edit == DiffEdit::Delete)
                    .count();
                // A closer (`}`) ends the block above; sliding past it would
                // delete that block's end and keep the deleted one's.
                let closer = old[old_index]
                    .trim()
                    .chars()
                    .all(|character| matches!(character, '}' | ']' | ')' | ','));
                if run > 0 && !closer && old[old_index] == old[old_index + run] {
                    edits[index] = DiffEdit::Delete;
                    edits[index + run] = DiffEdit::Equal;
                    changed = true;
                }
            }
            if edits[index] != DiffEdit::Insert {
                old_index += 1;
            }
            index += 1;
        }
    }
}

/// A line as the structural patch compares it: trimmed, without quotes and
/// without its trailing comment (as the language finds it).
fn comparable(language: &dyn ConfigLanguage, line: &str) -> String {
    let code = line[..comment_start(language, line)].trim();
    let unquoted = code.replace(['"', '\''], "");
    // A line of quotes alone (a multi-line string's `"""`) still is a line.
    if unquoted.is_empty() {
        code.to_string()
    } else {
        unquoted
    }
}

/// The trailing comment of `line`, or `""`.
fn trailing_comment<'a>(language: &dyn ConfigLanguage, line: &'a str) -> &'a str {
    line[comment_start(language, line)..].trim_end()
}

/// Where the trailing comment of `line` starts, or its length.
fn comment_start(language: &dyn ConfigLanguage, line: &str) -> usize {
    language
        .comment_start(line)
        .filter(|start| *start <= line.len() && line.is_char_boundary(*start))
        .unwrap_or(line.len())
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
    use super::language::{SchemaPathSegment, pointer_segments};
    let segments = pointer_segments(pointer);
    for depth in (1..segments.len()).rev() {
        let ancestor: String = segments[..depth]
            .iter()
            .map(|segment| format!("/{}", segment.replace('~', "~0").replace('/', "~1")))
            .collect();
        let Some(span) = behavior.value_span(source, &ancestor) else {
            continue;
        };
        let line_start: usize = source
            .split_inclusive('\n')
            .take(span.line.saturating_sub(1))
            .map(str::len)
            .sum();
        let start = line_start
            + source[line_start..]
                .char_indices()
                .nth(span.start_column.saturating_sub(1))
                .map_or(0, |(index, _)| index);
        let text = &source[start..];
        if !text.starts_with(['{', '[']) {
            return None;
        }
        let rest: Vec<SchemaPathSegment> = segments[depth..]
            .iter()
            .map(|segment| match segment.parse::<usize>() {
                Ok(index) => SchemaPathSegment::ArrayItem(index),
                Err(_) => SchemaPathSegment::Key(segment.clone()),
            })
            .collect();
        let (from, to) = super::inline::inline_path_span(text, &rest, key)?;
        let (from, to) = (start + from, start + to);
        let line = source[..from].matches('\n').count() + 1;
        let from_line = source[..from].rfind('\n').map_or(0, |index| index + 1);
        let column = source[from_line..from].chars().count() + 1;
        let to_column = if source[from..to].contains('\n') {
            column + 1
        } else {
            column + source[from..to].chars().count()
        };
        return Some(Span {
            line,
            start_column: column,
            end_column: to_column.max(column + 1),
        });
    }
    None
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

/// The triple quote (`"""`, `'''`) a value starts with at `start`.
fn triple_quote_at(source: &str, start: usize) -> Option<&'static str> {
    ["\"\"\"", "'''"]
        .into_iter()
        .find(|delimiter| source[start..].starts_with(delimiter))
}

/// The end of `range`, run through the closing delimiter when the value is
/// a triple-quoted string spanning lines.
fn through_triple_quote(source: &str, range: std::ops::Range<usize>) -> usize {
    triple_quote_at(source, range.start)
        .and_then(|delimiter| source[range.start + 3..].find(delimiter))
        .map_or(range.end, |close| range.start + 3 + close + 3)
}

/// The byte range of a one-based line/character-column span.
fn byte_range(source: &str, span: Span) -> Option<std::ops::Range<usize>> {
    let line_start = if span.line <= 1 {
        0
    } else {
        source.match_indices('\n').nth(span.line - 2)?.0 + 1
    };
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
fn differences(expected: &Value, actual: &Value, pointer: String, out: &mut Vec<String>) {
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

fn escape(key: &str) -> String {
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
