pub(super) fn utf16_to_byte_index(source: &str, offset: usize) -> usize {
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

fn yaml_lexical_state(source: &str, cursor: usize) -> (Option<char>, bool) {
    let cursor = utf16_to_byte_index(source, cursor);
    let line_start = source[..cursor].rfind('\n').map_or(0, |index| index + 1);
    let line = &source[line_start..cursor];
    let characters: Vec<_> = line.char_indices().collect();
    let mut quote = None;
    let mut comment = false;
    let mut escaped = false;
    let mut index = 0;
    while index < characters.len() {
        let (byte_index, character) = characters[index];
        if comment {
            break;
        }
        if let Some(active_quote) = quote {
            if active_quote == '\'' && character == '\'' {
                if characters
                    .get(index + 1)
                    .is_some_and(|(_, next)| *next == '\'')
                {
                    index += 2;
                    continue;
                }
                quote = None;
            } else if active_quote == '"' && escaped {
                escaped = false;
            } else if active_quote == '"' && character == '\\' {
                escaped = true;
            } else if character == active_quote {
                quote = None;
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character == '#'
            && (byte_index == 0
                || line[..byte_index]
                    .chars()
                    .next_back()
                    .is_some_and(char::is_whitespace))
        {
            comment = true;
        }
        index += 1;
    }
    (quote, comment)
}

pub(super) fn paired_edit(
    source: &str,
    start: usize,
    end: usize,
    opening: char,
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
    let (active_quote, in_comment) = yaml_lexical_state(source, start);
    if in_comment || active_quote.is_some() {
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

    let has_closing_ahead = start == end && source[start_byte..].starts_with(closing);
    let insertion = if has_closing_ahead {
        opening.to_string()
    } else {
        format!("{opening}{selected}{closing}")
    };
    let (updated, _) = replace_selection(source, start, end, &insertion);
    let selection_start = start + opening.len_utf16();
    let selection_end = if start == end || has_closing_ahead {
        selection_start
    } else {
        selection_start + selected.encode_utf16().count()
    };
    Some((updated, selection_start, selection_end))
}

pub(super) fn skip_closing_pair(source: &str, cursor: usize, closing: char) -> Option<usize> {
    let byte_index = utf16_to_byte_index(source, cursor);
    source[byte_index..]
        .starts_with(closing)
        .then_some(cursor + closing.len_utf16())
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
