use super::language::ConfigLanguage;

/// The byte index of UTF-16 offset `offset` (as the browser reports carets),
/// clamped to the end of `source`.
pub fn utf16_to_byte_index(source: &str, offset: usize) -> usize {
    let mut utf16_offset = 0;
    for (byte_index, character) in source.char_indices() {
        if utf16_offset >= offset {
            return byte_index;
        }
        utf16_offset += character.len_utf16();
    }
    source.len()
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

/// Auto-closing and selection wrapping for a typed bracket or quote; the
/// language's `lexical_state` blocks pairing inside strings and comments.
pub(super) fn paired_edit(
    source: &str,
    start: usize,
    end: usize,
    opening: char,
    behavior: &dyn ConfigLanguage,
) -> Option<(String, usize, usize)> {
    let closing = match opening {
        '"' => '"',
        '\'' => '\'',
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return None,
    };
    let start_byte = utf16_to_byte_index(source, start);
    let end_byte = utf16_to_byte_index(source, end);
    let selected = &source[start_byte..end_byte];
    let lexical = behavior.lexical_state(source, start);
    if lexical.in_comment || lexical.quote.is_some() {
        return None;
    }
    if matches!(opening, '"' | '\'')
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
        let after_word = matches!(opening, '"' | '\'')
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

pub(super) fn delete_empty_pair(source: &str, cursor: usize) -> Option<(String, usize)> {
    let byte_index = utf16_to_byte_index(source, cursor);
    let previous = source[..byte_index].chars().next_back()?;
    let next = source[byte_index..].chars().next()?;
    let expected = match previous {
        '"' => '"',
        '\'' => '\'',
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return None,
    };
    if next != expected {
        return None;
    }

    let start = cursor - previous.len_utf16();
    let end = cursor + next.len_utf16();
    Some(replace_selection(source, start, end, ""))
}
