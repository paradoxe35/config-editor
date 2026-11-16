//! Writing an accepted suggestion into the document, and what the editor
//! remembers about it (inserted closers, Tab slots).

use super::*;
use crate::editor::text::text_size;

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
    crate::editor::language::value_after_separator(
        source[line_start..cursor_byte].trim_end(),
        &behavior.config(),
    )
}

/// Completing a key that already has its separator and value after the caret
/// (`keep|: 99`, `"publ|": "x"`) renames just the key and keeps the value.
pub(super) fn rename_key_with_value(
    option: &mut Completion,
    source: &str,
    at: TextSize,
    behavior: &dyn ConfigLanguage,
) {
    if option.is_value || option.start > at {
        return;
    }
    let (Some(head), Some(rest)) = (slice::before(source, at), slice::after(source, at)) else {
        return;
    };
    let line_rest = rest.split('\n').next().unwrap_or("");
    let typed = slice::after(head, option.start)
        .unwrap_or_default()
        .to_string();
    // Inside a quoted key (`"pub|lic": 1`, `'pub|lic': 1`) the rest of the
    // key goes too, and its quotes stay as written.
    let config = behavior.config();
    let is_quote = |character: char| {
        config
            .quotes
            .iter()
            .any(|quote| quote.starts_with(character))
    };
    let quote = typed.chars().next().filter(|first| is_quote(*first));
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
    if !behavior.value_follows_key(after_quote) {
        return;
    }
    let quoted = quote.or_else(|| {
        option
            .insert
            .trim_start()
            .chars()
            .next()
            .filter(|first| is_quote(*first))
    });
    option.insert = match quoted {
        Some(quote) => format!("{quote}{}{quote}", option.label),
        None => option.label.clone(),
    };
    // Up to the caret, plus the rest of a quoted key and its closing quote.
    let closing_quote = if quoted.is_some() {
        line_rest.len() - after_quote.len()
    } else {
        0
    };
    option.end = at + text_size(word + closing_quote);
    option.cursor_offset = TextSize::of(option.insert.as_str());
    option.selected = TextSize::default();
    option.renames = true;
}

/// The next empty value slot after byte `caret` and before byte `end` of
/// `source`: inside `""`, or where the language sees an empty value
/// (`key: `, `<k></k>`). Tab walks the slots of an accepted block.
pub(crate) fn next_slot(
    source: &str,
    caret: TextSize,
    end: TextSize,
    behavior: &dyn ConfigLanguage,
) -> Option<TextSize> {
    let len = TextSize::of(source);
    let (from, until) = (caret.min(len), end.min(len));
    let region = (from <= until)
        .then(|| slice::slice(source, TextRange::new(from, until)))
        .flatten()?;
    // Inside an empty string of one of the language's quotes.
    let mut candidates: Vec<usize> = behavior
        .config()
        .quotes
        .iter()
        .filter(|quote| quote.chars().count() == 1)
        .flat_map(|quote| {
            region
                .match_indices(&format!("{quote}{quote}"))
                .map(|(at, _)| at + quote.len())
                .collect::<Vec<_>>()
        })
        .collect();
    candidates.extend(behavior.empty_value_slots(region));
    candidates
        .into_iter()
        .filter(|at| *at > 0)
        .min()
        .map(|at| from + text_size(at))
}

/// The closers `completion` wrote after the caret: brackets and quotes
/// (`[|]`, `"|"`), and markup the language says closes an element (an XML
/// end tag). `selection` is the (anchor, caret) left after accepting; a
/// value typed from the anchor ends at the closer at the caret.
pub(crate) fn inserted_closers(
    completion: &Completion,
    updated: &str,
    (anchor, caret): (TextSize, TextSize),
    behavior: &dyn ConfigLanguage,
) -> Vec<Closer> {
    let insert = completion.insert.as_str();
    let insert_start = caret
        .checked_sub(completion.cursor_offset)
        .unwrap_or_default();
    let mut closers = Vec::new();
    // The character before the value a closer ends: before the anchor for
    // the closer at the caret (a default may sit in between), right before
    // the closer for empty ones further on (`""`, `<k></k>`).
    let opener = |at: TextSize| {
        let value_start = if at == caret { anchor } else { at };
        slice::before(updated, value_start)?.chars().next_back()
    };
    let config = behavior.config();
    for (byte, character) in insert.char_indices() {
        let offset = insert_start + text_size(byte);
        if offset >= caret {
            if config.brackets.iter().any(|(_, close)| *close == character) {
                closers.push(Closer {
                    start: offset,
                    len: TextSize::of(character),
                    opener: None,
                    completion: false,
                });
            } else if config
                .quotes
                .iter()
                .any(|quote| quote.starts_with(character))
            {
                closers.push(Closer {
                    start: offset,
                    len: TextSize::of(character),
                    opener: opener(offset),
                    completion: false,
                });
            } else if let Some(len) = behavior.closing_markup_len(&insert[byte..]) {
                closers.push(Closer {
                    start: offset,
                    len: text_size(len),
                    opener: opener(offset),
                    completion: false,
                });
            }
        }
    }
    closers
}

/// The text with `completion` applied, and the selection after it as
/// `(anchor, caret)` (bytes of it); they are equal unless a default is
/// left selected.
pub(crate) fn apply_completion(
    source: &str,
    completion: &Completion,
) -> (String, TextSize, TextSize) {
    let len = TextSize::of(source);
    let start = completion.start.min(len);
    let end = completion.end.clamp(start, len);
    let (Some(head), Some(tail)) = (slice::before(source, start), slice::after(source, end)) else {
        return (source.to_string(), start, start);
    };
    let updated = format!("{head}{}{tail}", completion.insert);
    let caret = start + completion.cursor_offset;
    let anchor = caret.checked_sub(completion.selected).unwrap_or_default();
    (updated, anchor, caret)
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
    let byte = usize::from(caret);
    format!("{}|{}", &updated[..byte], &updated[byte..])
}
