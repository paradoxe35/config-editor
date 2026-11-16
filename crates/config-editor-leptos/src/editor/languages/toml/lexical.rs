//! The one reading of a TOML line's strings and comment: where a comment
//! starts, which string is open at a place, whether a multi-line string
//! is left open. Every feature asking "is this in a string, a comment or
//! code" on a line (comments, splitting a line's code from its comment,
//! Enter, the caret's lexical state, the multi-line string bodies) reads
//! it here, so they agree.
//!
//! As TOML reads it: `"…"` escapes with a backslash, `'…'` holds its text
//! as written, `"""` and `'''` open strings that may go on over lines (up
//! to two more quotes before the closing three are the string's), and a
//! `#` outside a string starts a comment wherever it is (`n = 1#c`).

use super::parse::closing_delimiter;

/// What a line holds up to a place: the quote of the string open there
/// (one of three quotes for a multi-line string), the delimiter of a
/// multi-line string left open, and where a comment started.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct LineRead {
    pub(super) quote: Option<char>,
    pub(super) multi_line: Option<&'static str>,
    pub(super) comment: Option<usize>,
}

/// `text` (a line, or a part of one outside a string) read up to byte
/// `until`.
pub(super) fn read_line(text: &str, until: usize) -> LineRead {
    read(text, until, |_| false).0
}

/// `text` read up to byte `until`, `code` called with each byte outside
/// strings and before any comment; the reading stops at the first byte
/// `code` is true for, which is given back.
pub(super) fn read(
    text: &str,
    until: usize,
    mut code: impl FnMut(usize) -> bool,
) -> (LineRead, Option<usize>) {
    let bytes = text.as_bytes();
    let until = until.min(text.len());
    let mut read = LineRead::default();
    let mut quote: Option<u8> = None;
    let mut at = 0;
    while at < until {
        let byte = bytes[at];
        match quote {
            Some(b'"') if byte == b'\\' => {
                at += 2;
                continue;
            }
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None => {
                if byte == b'#' {
                    read.comment = Some(at);
                    return (read, None);
                }
                if code(at) {
                    return (read, Some(at));
                }
                if matches!(byte, b'"' | b'\'') {
                    let delimiter = if byte == b'"' { "\"\"\"" } else { "'''" };
                    if text[at..].starts_with(delimiter) {
                        let Some(close) = closing_delimiter(&text[at + 3..], delimiter) else {
                            read.quote = Some(char::from(byte));
                            read.multi_line = Some(delimiter);
                            return (read, None);
                        };
                        // Up to two more quotes end the string's own text.
                        let end = at + 3 + close + 3;
                        let extra = bytes[end..]
                            .iter()
                            .take_while(|next| **next == byte)
                            .count()
                            .min(2);
                        if end + extra > until {
                            read.quote = Some(char::from(byte));
                            return (read, None);
                        }
                        at = end + extra;
                        continue;
                    }
                    quote = Some(byte);
                }
            }
        }
        at += 1;
    }
    read.quote = quote.map(char::from);
    (read, None)
}

/// `line` split before its comment, if it has one.
pub(super) fn split_comment(line: &str) -> (&str, Option<&str>) {
    match read_line(line, line.len()).comment {
        Some(at) => (&line[..at], Some(&line[at..])),
        None => (line, None),
    }
}

#[cfg(test)]
mod tests {
    use super::{read_line, split_comment};

    #[test]
    fn a_comment_starts_at_a_hash_outside_strings() {
        let cases: &[(&str, Option<usize>)] = &[
            ("a = 1 # note", Some(6)),
            ("a = 1#note", Some(5)),
            ("# whole line", Some(0)),
            ("a = \"x # in\" # note", Some(13)),
            ("a = 'x # in' # note", Some(13)),
            ("a = \"q\\\" # in\" # note", Some(15)),
            ("a = \"\"\"x # in\"\"\" # note", Some(17)),
            ("a = '''x''''' # note", Some(14)),
            ("a = \"\"\"open # in", None),
            ("a = [\"x\", '#'] # note", Some(15)),
        ];
        let mut failures = Vec::new();
        for (line, expected) in cases {
            let found = read_line(line, line.len()).comment;
            if found != *expected {
                failures.push(format!("{line:?}: {found:?}, expected {expected:?}"));
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
        assert_eq!(split_comment("a = 1 # n"), ("a = 1 ", Some("# n")));
    }

    #[test]
    fn the_string_open_at_a_place_is_found() {
        let open = |line: &str| read_line(line, line.len());
        assert_eq!(open("a = \"abc").quote, Some('"'));
        assert_eq!(open("a = 'abc").quote, Some('\''));
        assert_eq!(open("a = \"abc\"").quote, None);
        assert_eq!(open("a = \"\"\"abc").multi_line, Some("\"\"\""));
        assert_eq!(open("a = '''abc").multi_line, Some("'''"));
        assert_eq!(open("a = \"\"\"abc\"\"\"").multi_line, None);
        assert_eq!(read_line("a = \"\"\"abc\"\"\"", 8).quote, Some('"'));
        assert_eq!(open("# 'abc").quote, None);
    }
}
