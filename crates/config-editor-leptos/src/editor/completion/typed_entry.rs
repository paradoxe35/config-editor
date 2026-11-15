//! The text with only the entry being typed at the caret left out, so a
//! list item written on one line (`{"kind": "a", "na|"}`, `{ a = 1, | }`,
//! `<item><na|</item>`, `<item k="a" m="|"/>`) stays where it is, its own (possibly empty) item:
//! the items after it keep their places. Read from the language's config
//! (its brackets, item separator, quotes and markup delimiters), so it
//! works alike in every language.

use super::ConfigLanguage;

/// `source` without the entry at `cursor` (a byte index) and one item
/// separator next to it, when the caret's line holds structure around it
/// (a bracket opened or a separator before it, a bracket closed or a
/// separator after it, on the same line), or a markup tag left open
/// before it (`<na|`), with the byte where the entry was (what is before
/// it is as in `source`). `None` when the line is only the entry: leaving
/// the line out reads the rest then.
pub(crate) fn typed_entry_left_out(
    behavior: &dyn ConfigLanguage,
    source: &str,
    cursor: usize,
) -> Option<(String, usize)> {
    let line_start = source[..cursor].rfind('\n').map_or(0, |at| at + 1);
    let line_end = source[cursor..]
        .find('\n')
        .map_or(source.len(), |at| cursor + at);
    if let Some(delimiters) = behavior.markup_delimiters() {
        return open_tag_left_out(behavior, source, line_start, cursor, line_end, delimiters);
    }
    let config = behavior.config();
    let quotes: Vec<char> = config
        .quotes
        .iter()
        .filter_map(|quote| single_char(quote))
        .collect();
    let literal: Vec<char> = config
        .literal_quotes
        .iter()
        .filter_map(|quote| single_char(quote))
        .collect();
    let separator = config.item_separator;
    let opens = |character: char| config.brackets.iter().any(|(open, _)| *open == character);
    let closes = |character: char| config.brackets.iter().any(|(_, close)| *close == character);

    // Before the caret: the brackets still open, and where the entry
    // starts (past the innermost one, or past the last separator in it).
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut open_at: Vec<usize> = Vec::new();
    let mut left: Option<(usize, bool)> = None;
    for (offset, character) in source[line_start..cursor].char_indices() {
        let at = line_start + offset;
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && !literal.contains(&open) {
                escaped = true;
            } else if character == open {
                quote = None;
            }
            continue;
        }
        if quotes.contains(&character) {
            quote = Some(character);
        } else if opens(character) {
            open_at.push(at);
            left = Some((at + 1, false));
        } else if closes(character) {
            open_at.pop();
            left = open_at.last().map(|open| (open + 1, false));
        } else if Some(character) == separator {
            left = Some((at + 1, true));
        }
    }

    // After it: the first separator or closing bracket at the caret's
    // depth, on the line.
    let mut depth = 0usize;
    let mut right: Option<(usize, bool)> = None;
    for (offset, character) in source[cursor..line_end].char_indices() {
        let at = cursor + offset;
        if let Some(open) = quote {
            if escaped {
                escaped = false;
            } else if character == '\\' && !literal.contains(&open) {
                escaped = true;
            } else if character == open {
                quote = None;
            }
            continue;
        }
        if quotes.contains(&character) {
            quote = Some(character);
        } else if opens(character) {
            depth += 1;
        } else if closes(character) {
            if depth == 0 {
                right = Some((at, false));
                break;
            }
            depth -= 1;
        } else if depth == 0 && Some(character) == separator {
            right = Some((at, true));
            break;
        }
    }
    if left.is_none() && right.is_none() {
        return None;
    }
    let (mut start, left_separator) = left.unwrap_or((line_start, false));
    let (mut end, right_separator) = right.unwrap_or((line_end, false));
    if left_separator {
        start -= separator.map_or(0, char::len_utf8);
    } else if right_separator {
        end += separator.map_or(0, char::len_utf8);
    }
    Some((format!("{}{}", &source[..start], &source[end..]), start))
}

/// A markup tag the caret's line opens before the caret and leaves open
/// (`<na|`), left out with the rest of its name after the caret. The
/// source as it is where none is open there (nothing typed to leave out).
fn open_tag_left_out(
    behavior: &dyn ConfigLanguage,
    source: &str,
    line_start: usize,
    cursor: usize,
    line_end: usize,
    (open, close): (char, char),
) -> Option<(String, usize)> {
    let before = &source[line_start..cursor];
    let Some(opened) = before.rfind(open) else {
        return (!before.trim().is_empty()).then(|| (source.to_string(), cursor));
    };
    if before[opened..].contains(close) {
        return Some((source.to_string(), cursor));
    }
    let end = source[cursor..line_end]
        .find(|character: char| {
            character.is_whitespace() || character == open || character == close
        })
        .map_or(line_end, |at| cursor + at);
    // In an open tag's attributes (`<a k="1" b|`, `<a k="1" b="|"/>`):
    // only the attribute typed is left out, so its name and the other
    // attributes (which decide its values) stay.
    if source[line_start + opened..cursor].contains(char::is_whitespace) {
        let quotes: Vec<char> = behavior
            .config()
            .quotes
            .iter()
            .filter_map(|quote| single_char(quote))
            .collect();
        let tag = Tag {
            start: line_start + opened,
            open,
            close,
            quotes: &quotes,
        };
        return tag.attribute_left_out(source, cursor, line_end);
    }
    let start = line_start + opened;
    Some((format!("{}{}", &source[..start], &source[end..]), start))
}

/// A markup tag left open on the caret's line: where it starts, its
/// delimiters and the language's quotes.
struct Tag<'a> {
    start: usize,
    open: char,
    close: char,
    quotes: &'a [char],
}

impl Tag<'_> {
    /// `source` without the attribute at `cursor` (from the space before
    /// it through its value's closing quote, or up to the mark closing the
    /// tag), with the byte where it was.
    fn attribute_left_out(
        &self,
        source: &str,
        cursor: usize,
        line_end: usize,
    ) -> Option<(String, usize)> {
        let mut quote: Option<char> = None;
        let mut start = cursor;
        for (offset, character) in source[self.start..cursor].char_indices() {
            match quote {
                Some(open) if character == open => quote = None,
                Some(_) => {}
                None if self.quotes.contains(&character) => quote = Some(character),
                None if character.is_whitespace() => start = self.start + offset,
                None => {}
            }
        }
        let rest = &source[cursor..line_end];
        let mut end = line_end;
        for (offset, character) in rest.char_indices() {
            let at = cursor + offset;
            match quote {
                Some(open) if character == open => {
                    end = at + character.len_utf8();
                    break;
                }
                Some(_) => {}
                None if self.quotes.contains(&character) => quote = Some(character),
                // A mark the closing delimiter follows (`/>`) stays.
                None if character.is_whitespace()
                    || character == self.open
                    || character == self.close
                    || (!character.is_alphanumeric()
                        && rest[offset + character.len_utf8()..].starts_with(self.close)) =>
                {
                    end = at;
                    break;
                }
                None => {}
            }
        }
        Some((format!("{}{}", &source[..start], &source[end..]), start))
    }
}

fn single_char(text: &str) -> Option<char> {
    let mut characters = text.chars();
    let first = characters.next()?;
    characters.next().is_none().then_some(first)
}
