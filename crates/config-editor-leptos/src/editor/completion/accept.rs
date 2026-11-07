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
/// whitespace at the caret (`key = false |`), as the language reads it.
pub(super) fn line_value_before(
    source: &str,
    cursor_byte: usize,
    behavior: &dyn ConfigLanguage,
) -> bool {
    let line_start = source[..cursor_byte]
        .rfind('\n')
        .map_or(0, |index| index + 1);
    behavior.line_has_value(source[line_start..cursor_byte].trim_end())
}

/// Completing a key that already has its separator and value after the caret
/// (`keep|: 99`, `"publ|": "x"`) renames just the key and keeps the value.
pub(super) fn rename_key_with_value(
    option: &mut Completion,
    source: &str,
    cursor: usize,
    behavior: &dyn ConfigLanguage,
) {
    if option.is_value || option.start > cursor {
        return;
    }
    let caret = utf16_to_byte_index(source, cursor);
    let line_rest = source[caret..].split('\n').next().unwrap_or("");
    let typed = source[utf16_to_byte_index(source, option.start)..caret].to_string();
    // Inside a quoted key (`"pub|lic": 1`, `'pub|lic': 1`) the rest of the
    // key goes too, and its quotes stay as written.
    let quote = typed
        .chars()
        .next()
        .filter(|first| matches!(first, '"' | '\''));
    let word = if quote.is_some() {
        line_rest
            .find(|character: char| {
                !(character.is_alphanumeric() || matches!(character, '_' | '-' | '.'))
            })
            .unwrap_or(0)
    } else {
        0
    };
    let line_rest = &line_rest[word..];
    let after_quote = quote
        .and_then(|quote| line_rest.strip_prefix(quote))
        .unwrap_or(line_rest);
    // A value follows, or the rest of a dotted key (`"web|".path = 1`).
    if !behavior.value_follows_key(after_quote)
        && !(quote.is_some() && after_quote.starts_with('.'))
    {
        return;
    }
    let quoted = quote.or_else(|| option.insert.trim_start().starts_with('"').then_some('"'));
    option.insert = match quoted {
        Some(quote) => format!("{quote}{}{quote}", option.label),
        None => option.label.clone(),
    };
    // Up to the caret, plus the rest of a quoted key and its closing quote.
    let rest_of_key = source[caret..caret + word].encode_utf16().count();
    option.end = cursor
        + rest_of_key
        + if quoted.is_some() {
            line_rest.len() - after_quote.len()
        } else {
            0
        };
    option.cursor_offset = option.insert.encode_utf16().count();
    option.selected = 0;
    option.renames = true;
}

/// The next empty value slot after UTF-16 `caret` and before `end` in
/// `source`: inside `""`, or where the language sees an empty value
/// (`key: `, `<k></k>`). Tab walks the slots of an accepted block.
pub(crate) fn next_slot(
    source: &str,
    caret: usize,
    end: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<usize> {
    let from = utf16_to_byte_index(source, caret);
    let until = utf16_to_byte_index(source, end).min(source.len());
    let region = source.get(from..until)?;
    let mut candidates: Vec<usize> = region.match_indices("\"\"").map(|(at, _)| at + 1).collect();
    candidates.extend(behavior.empty_value_slots(region));
    candidates
        .into_iter()
        .filter(|at| *at > 0)
        .min()
        .map(|at| source[..from + at].encode_utf16().count())
}

/// The closers `completion` wrote after the caret: brackets and quotes
/// (`[|]`, `"|"`), and markup the language says closes an element (an XML
/// end tag). `selection` is the (anchor, caret) left after accepting; a
/// value typed from the anchor ends at the closer at the caret.
pub(crate) fn inserted_closers(
    completion: &Completion,
    updated: &str,
    (anchor, caret): (usize, usize),
    behavior: &dyn ConfigLanguage,
) -> Vec<Closer> {
    let length = updated.encode_utf16().count();
    let insert = completion.insert.as_str();
    let mut offset = caret.saturating_sub(completion.cursor_offset);
    let mut closers = Vec::new();
    // The character before the value a closer ends: before the anchor for
    // the closer at the caret (a default may sit in between), right before
    // the closer for empty ones further on (`""`, `<k></k>`).
    let units: Vec<u16> = updated.encode_utf16().collect();
    let opener = |at: usize| {
        let value_start = if at == caret { anchor } else { at };
        let before = value_start.checked_sub(1)?;
        char::decode_utf16([*units.get(before)?]).next()?.ok()
    };
    for (byte, character) in insert.char_indices() {
        if offset >= caret {
            if matches!(character, ')' | ']' | '}') {
                closers.push(Closer {
                    distance: length - offset,
                    len: 1,
                    opener: None,
                });
            } else if matches!(character, '"' | '\'') {
                closers.push(Closer {
                    distance: length - offset,
                    len: 1,
                    opener: opener(offset),
                });
            } else if let Some(len) = behavior.closing_markup_len(&insert[byte..]) {
                closers.push(Closer {
                    distance: length - offset,
                    len: insert[byte..byte + len].encode_utf16().count(),
                    opener: opener(offset),
                });
            }
        }
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
