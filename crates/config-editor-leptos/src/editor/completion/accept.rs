//! Writing an accepted suggestion into the document, and what the editor
//! remembers about it (inserted closers, Tab slots).

use super::*;

pub(crate) fn should_auto_trigger(input_type: &str, previous: &str, current: &str) -> bool {
    if current == previous || current.matches('\n').count() > previous.matches('\n').count() {
        return false;
    }
    let grew = current.len() > previous.len();
    let typed_over_selection = matches!(input_type, "insertText" | "insertCompositionText");
    if !grew && !typed_over_selection {
        return false;
    }
    !matches!(
        input_type,
        "insertLineBreak"
            | "insertParagraph"
            | "insertFromPaste"
            | "insertFromDrop"
            | "deleteContentBackward"
            | "deleteContentForward"
            | "deleteByCut"
            | "deleteByDrag"
            | "historyUndo"
            | "historyRedo"
    )
}

/// Whether the caret's line already holds a value before the trailing
/// whitespace at the caret (`key = false |`).
pub(super) fn line_value_before(source: &str, cursor_byte: usize) -> bool {
    let line_start = source[..cursor_byte]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    let before = source[line_start..cursor_byte].trim_end();
    let value = before
        .rsplit_once(['=', ':'])
        .map_or("", |(_, value)| value)
        .trim();
    !value.is_empty() && !value.ends_with(['[', '{', ',', '='])
}

/// Completing a key that already has its separator and value after the caret
/// (`keep|: 99`, `"publ|": "x"`) renames just the key and keeps the value.
pub(super) fn rename_key_with_value(option: &mut Completion, source: &str, cursor: usize) {
    if option.is_value || option.start > cursor {
        return;
    }
    let caret = utf16_to_byte_index(source, cursor);
    let line_rest = source[caret..].split('\n').next().unwrap_or("");
    let typed = source[utf16_to_byte_index(source, option.start)..caret].to_string();
    // Inside a quoted key (`"pub|lic": 1`) the rest of the key goes too.
    let word = if typed.starts_with('"') {
        line_rest
            .find(|character: char| {
                !(character.is_alphanumeric() || matches!(character, '_' | '-' | '.'))
            })
            .unwrap_or(0)
    } else {
        0
    };
    let line_rest = &line_rest[word..];
    let after_quote = line_rest.strip_prefix('"').unwrap_or(line_rest);
    let separator = after_quote.trim_start();
    // An HCL block (`pol| {`, `servi| "ssh" {`) has its body after the name.
    let block_body = (separator.starts_with('{') || separator.starts_with('"'))
        && line_rest.trim_end().ends_with('{');
    if !(separator.starts_with(':')
        || separator.starts_with('=') && !separator.starts_with("==")
        || block_body)
    {
        return;
    }
    let quoted = option.insert.trim_start().starts_with('"');
    option.insert = if quoted {
        format!("\"{}\"", option.label)
    } else {
        option.label.clone()
    };
    // Up to the caret, plus the rest of a quoted key and its closing quote.
    let rest_of_key = source[caret..caret + word].encode_utf16().count();
    option.end = cursor
        + rest_of_key
        + if quoted {
            line_rest.len() - after_quote.len()
        } else {
            0
        };
    option.cursor_offset = option.insert.encode_utf16().count();
    option.selected = 0;
}

/// The next empty value slot after UTF-16 `caret` and before `end` in
/// `source`: inside `""`, between `<k></k>`, or at the end of a line that
/// ends with a separator (`key: `, `key = `). Tab walks the slots of an
/// accepted block.
pub(crate) fn next_slot(source: &str, caret: usize, end: usize) -> Option<usize> {
    let from = utf16_to_byte_index(source, caret);
    let until = utf16_to_byte_index(source, end).min(source.len());
    let region = source.get(from..until)?;
    let mut candidates = Vec::new();
    for (at, _) in region.match_indices("\"\"") {
        candidates.push(at + 1);
    }
    for (at, _) in region.match_indices("></") {
        candidates.push(at + 1);
    }
    let mut offset = 0;
    for line in region.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        if body.ends_with(": ") || body.ends_with(" = ") || body.ends_with(':') {
            candidates.push(offset + body.len());
        }
        offset += line.len();
    }
    candidates
        .into_iter()
        .filter(|at| *at > 0)
        .min()
        .map(|at| source[..from + at].encode_utf16().count())
}

/// The closers `completion` wrote after the caret (`[|]`, `"|"`), as UTF-16
/// distances from the end of `updated`, so typing them steps over them.
pub(crate) fn inserted_closers(completion: &Completion, updated: &str, caret: usize) -> Vec<usize> {
    let length = updated.encode_utf16().count();
    let insert_start = caret.saturating_sub(completion.cursor_offset);
    let mut offset = insert_start;
    let mut closers = Vec::new();
    let mut previous = '\0';
    for character in completion.insert.chars() {
        if offset >= caret && matches!(character, ')' | ']' | '}' | '"' | '\'') {
            closers.push(length - offset);
        }
        // An end tag (`</k>`) written after the caret, by its `<`.
        if offset > caret && previous == '<' && character == '/' {
            closers.push(length - (offset - 1));
        }
        previous = character;
        offset += character.len_utf16();
    }
    closers
}

/// The text with `completion` applied, and the selection after it as
/// `(anchor, caret)`; they are equal unless a default is left selected.
pub(crate) fn apply_completion(source: &str, completion: &Completion) -> (String, usize, usize) {
    let (updated, end_cursor) =
        replace_selection(source, completion.start, completion.end, &completion.insert);
    let end_offset = completion.insert.encode_utf16().count();
    let caret = end_cursor
        .saturating_sub(end_offset)
        .saturating_add(completion.cursor_offset);
    (updated, caret.saturating_sub(completion.selected), caret)
}

/// Accepts `label` at the `|` in `marked` (as after Ctrl+Space) and returns
/// the document with `|` at the new caret.
#[cfg(test)]
#[allow(dead_code, reason = "used by the language test modules")]
pub(crate) fn accept_marked(
    marked: &str,
    label: &str,
    schema: &SchemaBundle,
    behavior: &dyn ConfigLanguage,
) -> String {
    let at = marked.find('|').expect("a | caret marker");
    let source = marked.replacen('|', "", 1);
    let options = completions(
        &source,
        source[..at].encode_utf16().count(),
        true,
        Some(schema),
        true,
        behavior,
    );
    let Some(option) = options.iter().find(|option| option.label == label) else {
        let labels: Vec<&str> = options.iter().map(|option| option.label.as_str()).collect();
        panic!("{label} is not offered at the caret; offered {labels:?}");
    };
    let (updated, _, caret) = apply_completion(&source, option);
    let byte = updated
        .char_indices()
        .map(|(index, _)| index)
        .chain([updated.len()])
        .find(|index| updated[..*index].encode_utf16().count() == caret)
        .expect("caret on a character boundary");
    format!("{}|{}", &updated[..byte], &updated[byte..])
}
