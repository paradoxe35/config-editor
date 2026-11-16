//! The one reading of a YAML line's strings and comment: where a comment
//! starts, which quote is open at a place, where a key's colon is. Every
//! feature asking "is this in a string, a comment or code" on a line
//! (comments, highlighting, Enter, the suggestions, the caret's lexical
//! state) reads it here, so they agree.
//!
//! A quote opens a quoted scalar only where a scalar starts: at the start
//! of the text read, after an item's `- `, a complex key's `? `, a key's
//! `: `, a node's properties (`&a `, `!tag `), and inside a flow
//! collection after `[`, `{` or `,`. Anywhere else it is part of a plain
//! scalar (`key: don't`). Inside `'…'` a quote is written `''`; inside
//! `"…"` a backslash escapes. A `#` outside a quoted scalar starts a
//! comment at the start of the text or after a space or tab.

/// What a line holds up to a place: the quote of the quoted scalar open
/// there, where a comment started before it, and how many flow
/// collections are open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct LineRead {
    pub(super) quote: Option<char>,
    pub(super) comment: Option<usize>,
    pub(super) flow: usize,
}

/// Whether a `#` after a space starts a comment that ends the reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Comments {
    End,
    /// Read as code (a value's text read for its colons).
    AsCode,
}

/// `text` (a line, or the part of one from where a scalar may start) read
/// up to byte `until`; a comment ends the reading.
pub(super) fn read_line(text: &str, until: usize) -> LineRead {
    read(text, until, Comments::End, |_, _| false).0
}

/// `text` read up to byte `until`, `code` called with each byte outside
/// quoted scalars (and before any comment) and the flow collections open
/// before it; the reading stops at the first byte `code` is true for,
/// which is given back.
pub(super) fn read(
    text: &str,
    until: usize,
    comments: Comments,
    mut code: impl FnMut(usize, usize) -> bool,
) -> (LineRead, Option<usize>) {
    let bytes = text.as_bytes();
    let until = until.min(text.len());
    let blank = |at: usize| {
        bytes
            .get(at)
            .is_none_or(|next| matches!(next, b' ' | b'\t' | b'\r' | b'\n'))
    };
    let mut read = LineRead::default();
    // Whether a scalar may start here (a quote then opens a string).
    let mut scalar_start = true;
    let mut at = 0;
    // A document marker (`--- 'a'`): a scalar may follow it.
    if (text.starts_with("---") || text.starts_with("...")) && blank(3) {
        at = 3;
    }
    while at < until {
        let byte = bytes[at];
        let starts_comment = byte == b'#' && (at == 0 || matches!(bytes[at - 1], b' ' | b'\t'));
        if starts_comment && comments == Comments::End {
            read.comment = Some(at);
            return (read, None);
        }
        if code(at, read.flow) {
            return (read, Some(at));
        }
        match byte {
            b' ' | b'\t' => {}
            b'"' | b'\'' if scalar_start => match quoted_close(text, at) {
                Some(end) if end <= until => {
                    scalar_start = false;
                    at = end;
                    continue;
                }
                _ => {
                    read.quote = Some(char::from(byte));
                    return (read, None);
                }
            },
            b'[' | b'{' if scalar_start || read.flow > 0 => read.flow += 1,
            b']' | b'}' if read.flow > 0 => {
                read.flow -= 1;
                scalar_start = false;
            }
            b',' if read.flow > 0 => scalar_start = true,
            b':' if read.flow > 0 || blank(at + 1) => scalar_start = true,
            b'-' | b'?' if scalar_start && blank(at + 1) => {}
            // A node's properties: a scalar may still start after them.
            b'&' | b'!' if scalar_start => {
                while at < until && !matches!(bytes[at], b' ' | b'\t') {
                    at += 1;
                }
                continue;
            }
            _ => scalar_start = false,
        }
        at += 1;
    }
    (read, None)
}

/// Just past the closing quote of the quoted scalar opening at byte
/// `start` of `text` (over lines too), or `None` when it stays open.
/// Inside `'…'` a quote is written `''`; inside `"…"` a backslash escapes
/// the next character.
pub(super) fn quoted_close(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let quote = bytes[start];
    let mut index = start + 1;
    while index < text.len() {
        match bytes[index] {
            b'\\' if quote == b'"' => index += 2,
            b'\'' if quote == b'\'' && bytes.get(index + 1) == Some(&b'\'') => index += 2,
            byte if byte == quote => return Some(index + 1),
            _ => index += 1,
        }
    }
    None
}

/// [`quoted_close`], the end of `text` for a quoted scalar left open.
pub(super) fn quoted_end(text: &str, start: usize) -> usize {
    quoted_close(text, start).unwrap_or(text.len())
}

/// Whether a node can start after `previous`, the last character before
/// it on its line other than blanks (`None`: none): a quote there opens a
/// quoted scalar, a bracket a flow collection.
pub(super) fn node_starts_after(previous: Option<char>) -> bool {
    previous.is_none_or(|previous| matches!(previous, '[' | '{' | ',' | ':' | '-' | '?'))
}

/// Where the comment on `text` starts, outside quoted scalars.
pub(super) fn comment_start(text: &str) -> Option<usize> {
    // Most lines hold no `#` at all: found without reading them.
    if !text.contains('#') {
        return None;
    }
    read_line(text, text.len()).comment
}

/// The first `:` of `text` outside quoted scalars that whitespace or the
/// end follows (a key's), before any comment or, with
/// [`Comments::AsCode`], anywhere.
pub(super) fn key_colon(text: &str, comments: Comments) -> Option<usize> {
    let bytes = text.as_bytes();
    let blank = |at: usize| bytes.get(at).is_none_or(u8::is_ascii_whitespace);
    read(text, text.len(), comments, |at, _| {
        bytes[at] == b':' && blank(at + 1)
    })
    .1
}

#[cfg(test)]
mod tests {
    use super::{Comments, LineRead, comment_start, key_colon, read_line};

    #[test]
    fn a_comment_starts_outside_quoted_scalars_after_a_space() {
        let cases: &[(&str, Option<usize>)] = &[
            ("key: value # note", Some(11)),
            ("# whole line", Some(0)),
            ("key: don't # note", Some(11)),
            ("it's: here # note", Some(11)),
            ("key: 'it''s # still quoted'", None),
            ("key: 'it''s' # after", Some(13)),
            ("key: \"a \\\" # still\" # after", Some(20)),
            ("key: \"value # not a comment\"", None),
            ("key: a#b", None),
            ("key: 'a'#b", None),
            ("- 'item # in' # note", Some(14)),
            ("- &anchor 'v # in' # note", Some(19)),
            ("key: !!str 'v # in' # n", Some(20)),
            ("? 'k # in' # note", Some(11)),
            ("key: [a, 'b # in', c] # note", Some(22)),
            ("key: {k: 'v # in'} # note", Some(19)),
            ("key: x,'y' # note", Some(11)),
            ("key: [don't] # note", Some(13)),
            ("--- 'doc # in' # note", Some(15)),
            ("key: \t# tab", Some(6)),
            ("ключ: 'é # in' # é", Some(20)),
        ];
        let mut failures = Vec::new();
        for (line, expected) in cases {
            let found = comment_start(line);
            if found != *expected {
                failures.push(format!("{line:?}: {found:?}, expected {expected:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn the_quote_open_at_a_place_is_a_quoted_scalars_only() {
        let open = |line: &str| read_line(line, line.len()).quote;
        assert_eq!(open("key: 'abc"), Some('\''));
        assert_eq!(open("key: \"abc"), Some('"'));
        assert_eq!(open("key: don't"), None);
        assert_eq!(open("key: 'it''"), Some('\''));
        assert_eq!(open("key: 'done'"), None);
        assert_eq!(open("key: [a, 'b"), Some('\''));
        assert_eq!(
            read_line("key: [a, 'b", 11),
            LineRead {
                quote: Some('\''),
                comment: None,
                flow: 1
            }
        );
        assert_eq!(read_line("# 'note", 7).comment, Some(0));
        assert_eq!(read_line("key: 'a' # 'b", 7).quote, Some('\''));
    }

    #[test]
    fn a_keys_colon_is_outside_quoted_scalars() {
        assert_eq!(key_colon("it's: 1", Comments::End), Some(4));
        assert_eq!(key_colon("'a: b': 1", Comments::End), Some(6));
        assert_eq!(key_colon("url: http://x", Comments::End), Some(3));
        assert_eq!(key_colon("a # b: c", Comments::End), None);
        assert_eq!(key_colon("a # b: c", Comments::AsCode), Some(5));
        assert_eq!(key_colon("a:b", Comments::End), None);
    }
}
