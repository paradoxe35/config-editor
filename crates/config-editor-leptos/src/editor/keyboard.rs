use super::language::ConfigLanguage;

/// The byte index of UTF-16 offset `offset` (as the browser reports carets),
/// clamped to the end of `source`.
pub fn utf16_to_byte_index(source: &str, offset: usize) -> usize {
    // Read off the text's lines while a batch runs for it.
    if let Some(byte) = super::paths::byte_at(source, offset) {
        return byte;
    }
    // ASCII runs a block at a time (one unit per byte), the rest by
    // character: a long document is mostly ASCII.
    const BLOCK: usize = 64;
    let bytes = source.as_bytes();
    let (mut byte, mut unit) = (0, 0);
    while byte < bytes.len() && unit < offset {
        let block = (offset - unit).min(BLOCK).min(bytes.len() - byte);
        // The ASCII bytes the block starts with, then one other character.
        let run = &bytes[byte..byte + block];
        let ascii = if run.is_ascii() {
            block
        } else {
            run.iter()
                .position(|byte| !byte.is_ascii())
                .unwrap_or(block)
        };
        byte += ascii;
        unit += ascii;
        if ascii == block {
            continue;
        }
        let Some(character) = source[byte..].chars().next() else {
            break;
        };
        super::work_count::note(super::work_count::Work::CharacterStep);
        byte += character.len_utf8();
        unit += character.len_utf16();
    }
    super::work_count::note_many(super::work_count::Work::OffsetSeek, byte);
    byte
}

pub(super) fn replace_selection(
    source: &str,
    start: usize,
    end: usize,
    insert: &str,
) -> (String, usize) {
    let start = utf16_to_byte_index(source, start);
    let end = utf16_to_byte_index(source, end);
    let mut result = String::with_capacity(source.len() + insert.len());
    result.push_str(&source[..start]);
    result.push_str(insert);
    result.push_str(&source[end..]);
    let new_cursor = source[..start].encode_utf16().count() + insert.encode_utf16().count();
    (result, new_cursor)
}

/// Characters before which an opening bracket or quote auto-closes (VS Code's
/// `autoCloseBefore` default), besides whitespace and the end of the text.
const AUTO_CLOSE_BEFORE: &str = ";:.,=}])>\"'`";

/// The closer `opening` pairs with in `behavior`: one of its brackets, or
/// one of its one-character quotes, with whether it is a quote. Only what
/// the language writes as a bracket or a quote pairs (no `(` in JSON, no
/// `'` in HCL).
pub(super) fn pair_of(behavior: &dyn ConfigLanguage, opening: char) -> Option<(char, bool)> {
    let config = behavior.config();
    if let Some((_, closing)) = config.brackets.iter().find(|(open, _)| *open == opening) {
        return Some((*closing, false));
    }
    is_quote(behavior, opening).then_some((opening, true))
}

/// Whether `character` is one of `behavior`'s one-character quotes.
pub(super) fn is_quote(behavior: &dyn ConfigLanguage, character: char) -> bool {
    behavior.config().quotes.iter().any(|quote| {
        let mut characters = quote.chars();
        characters.next() == Some(character) && characters.next().is_none()
    })
}

/// Auto-closing and selection wrapping for a typed bracket or quote; the
/// language's `lexical_state` blocks pairing inside strings and comments.
pub(super) fn paired_edit(
    source: &str,
    start: usize,
    end: usize,
    opening: char,
    behavior: &dyn ConfigLanguage,
) -> Option<(String, usize, usize)> {
    let (closing, quote) = pair_of(behavior, opening)?;
    let start_byte = utf16_to_byte_index(source, start);
    let end_byte = utf16_to_byte_index(source, end);
    let selected = &source[start_byte..end_byte];
    let lexical = behavior.lexical_state(source, start);
    if lexical.in_comment || lexical.quote.is_some() {
        return None;
    }
    if quote
        && source[..start_byte]
            .chars()
            .rev()
            .take_while(|character| *character == '\\')
            .count()
            % 2
            == 1
    {
        return None;
    }

    // As in VS Code: a quote never pairs right after a word character (`don't`).
    if start == end {
        let next = source[start_byte..].chars().next();
        let closes_here =
            next.is_none_or(|next| next.is_whitespace() || AUTO_CLOSE_BEFORE.contains(next));
        let after_word = quote
            && source[..start_byte]
                .chars()
                .next_back()
                .is_some_and(|previous| previous.is_alphanumeric() || previous == '_');
        if !closes_here || after_word {
            return None;
        }
    }
    // A closer already ahead belongs to an outer pair, so the new pair is
    // complete on its own (`{"web": |}` → `{"web": {|}}`).
    let (updated, _) =
        replace_selection(source, start, end, &format!("{opening}{selected}{closing}"));
    let selection_start = start + opening.len_utf16();
    let selection_end = selection_start + selected.encode_utf16().count();
    Some((updated, selection_start, selection_end))
}

/// The caret after typing `closing` over the same closer at `cursor`; `None`
/// after a `\`, where the character is escaped text.
pub(super) fn skip_closing_pair(source: &str, cursor: usize, closing: char) -> Option<usize> {
    let byte_index = utf16_to_byte_index(source, cursor);
    let escaped = source[..byte_index]
        .chars()
        .rev()
        .take_while(|character| *character == '\\')
        .count()
        % 2
        == 1;
    (!escaped && source[byte_index..].starts_with(closing)).then_some(cursor + closing.len_utf16())
}

/// Both characters of the empty pair around `cursor`, when `behavior`
/// pairs them.
pub(super) fn delete_empty_pair(
    source: &str,
    cursor: usize,
    behavior: &dyn ConfigLanguage,
) -> Option<(String, usize)> {
    let byte_index = utf16_to_byte_index(source, cursor);
    let previous = source[..byte_index].chars().next_back()?;
    let next = source[byte_index..].chars().next()?;
    let (expected, _) = pair_of(behavior, previous)?;
    if next != expected {
        return None;
    }

    let start = cursor - previous.len_utf16();
    let end = cursor + next.len_utf16();
    Some(replace_selection(source, start, end, ""))
}

/// Whether `character` closes what `behavior` pairs: one of its closing
/// brackets or one-character quotes.
pub(super) fn is_closer(behavior: &dyn ConfigLanguage, character: char) -> bool {
    is_closing_bracket(behavior, character) || is_quote(behavior, character)
}

/// Whether `character` is one of `behavior`'s closing brackets.
pub(super) fn is_closing_bracket(behavior: &dyn ConfigLanguage, character: char) -> bool {
    behavior
        .config()
        .brackets
        .iter()
        .any(|(_, close)| *close == character)
}
