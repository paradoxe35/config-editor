//! One-line containers shared by the brace languages: inline tables and
//! arrays (TOML), flow mappings and sequences (YAML), value maps and lists
//! (HCL). A language passes its key separator (`=` or `:`); quoting and
//! nesting are handled here once.

use super::language::SchemaPathSegment;

/// A `key <separator>` on one line: the key's byte range and the byte just past
/// the separator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InlineKey {
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) after_separator: usize,
}

/// What the caret is on inside an inline container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum InlineEntry {
    /// Typing a key that starts at byte `start`.
    Key { start: usize },
    /// Typing the value of `key`, which starts at byte `start`.
    Value { key: String, start: usize },
    /// Typing item `index` of an array; it starts at byte `start`.
    Item { index: usize, start: usize },
}

/// The caret inside one or more inline containers opened on its line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct InlineCaret {
    /// Path from the line's own key down to the container holding the caret.
    pub(super) path: Vec<SchemaPathSegment>,
    pub(super) entry: InlineEntry,
}

/// Every key on a line of code: the line's own key and each key inside its
/// inline containers.
pub(super) fn inline_keys(code: &str, separator: u8) -> Vec<InlineKey> {
    let mut keys = Vec::new();
    walk(code, separator, |event| {
        if let Event::Key(key) = event {
            keys.push(key);
        }
    });
    keys
}

/// The key under byte `at`, if any.
pub(super) fn inline_key_at(code: &str, separator: u8, at: usize) -> Option<InlineKey> {
    inline_keys(code, separator)
        .into_iter()
        .find(|key| (key.start..key.end).contains(&at))
}

/// Where the caret at the end of `code_before` sits, when it is inside an
/// inline container opened on the line; `None` at the line's top level.
pub(super) fn inline_caret(code_before: &str, separator: u8) -> Option<InlineCaret> {
    let mut frames: Vec<Frame> = Vec::new();
    let mut entry_key: Option<String> = None;
    let mut entry_start = 0;
    let mut value_start = None;
    walk(code_before, separator, |event| match event {
        Event::Key(key) => {
            entry_key = Some(unquote(&code_before[key.start..key.end]));
            value_start = Some(skip_blanks(code_before, key.after_separator));
        }
        Event::Open(bracket, at) => {
            let segment = match frames.last() {
                Some(Frame::Array { index, .. }) => Some(SchemaPathSegment::ArrayItem(*index)),
                _ => entry_key.take().map(SchemaPathSegment::Key),
            };
            frames.push(if bracket == b'{' {
                Frame::Table { segment }
            } else {
                Frame::Array { segment, index: 0 }
            });
            entry_key = None;
            value_start = None;
            entry_start = at + 1;
        }
        Event::Close => {
            frames.pop();
            entry_key = None;
            value_start = None;
        }
        Event::Comma(at) => {
            if let Some(Frame::Array { index, .. }) = frames.last_mut() {
                *index += 1;
            }
            entry_key = None;
            value_start = None;
            entry_start = at + 1;
        }
    });
    if frames.is_empty() {
        return None;
    }
    let path = frames.iter().filter_map(Frame::segment).cloned().collect();
    let entry = match (frames.last(), entry_key, value_start) {
        (_, Some(key), Some(start)) => InlineEntry::Value { key, start },
        (Some(Frame::Array { index, .. }), _, _) => InlineEntry::Item {
            index: *index,
            start: skip_blanks(code_before, entry_start),
        },
        _ => InlineEntry::Key {
            start: skip_blanks(code_before, entry_start),
        },
    };
    Some(InlineCaret { path, entry })
}

/// The byte just past the quoted string starting at `start`, or the end.
pub(super) fn skip_quoted(code: &str, start: usize) -> usize {
    let quote = code.as_bytes()[start];
    let mut index = start + 1;
    while index < code.len() {
        match code.as_bytes()[index] {
            b'\\' if quote == b'"' => index += 2,
            byte if byte == quote => return index + 1,
            _ => index += 1,
        }
    }
    code.len()
}

/// Length of an inline value starting at the caret: through its closing quote
/// when quoted, else up to the next `,` or closer, without trailing blanks.
pub(super) fn inline_value_len(text: &str) -> usize {
    if text.starts_with(['"', '\'']) {
        return skip_quoted(text, 0);
    }
    let end = text.find([',', '}', ']']).unwrap_or(text.len());
    text[..end].trim_end().len()
}

enum Frame {
    Table {
        segment: Option<SchemaPathSegment>,
    },
    Array {
        segment: Option<SchemaPathSegment>,
        index: usize,
    },
}

impl Frame {
    fn segment(&self) -> Option<&SchemaPathSegment> {
        match self {
            Frame::Table { segment } | Frame::Array { segment, .. } => segment.as_ref(),
        }
    }
}

enum Event {
    Key(InlineKey),
    Open(u8, usize),
    Close,
    Comma(usize),
}

/// Walks `code` once, reporting keys, brackets, and separating commas outside
/// quotes. A key is a bare word or quoted string at an entry start followed by
/// `separator`.
fn walk(code: &str, separator: u8, mut on: impl FnMut(Event)) {
    let bytes = code.as_bytes();
    let mut index = 0;
    let mut at_entry = true;
    while index < bytes.len() {
        match bytes[index] {
            b'{' | b'[' => {
                on(Event::Open(bytes[index], index));
                at_entry = true;
                index += 1;
            }
            b'}' | b']' => {
                on(Event::Close);
                at_entry = false;
                index += 1;
            }
            b',' => {
                on(Event::Comma(index));
                at_entry = true;
                index += 1;
            }
            b' ' | b'\t' => index += 1,
            b'"' | b'\'' if !at_entry => index = skip_quoted(code, index),
            _ if at_entry => {
                at_entry = false;
                let start = index;
                let end = if matches!(bytes[index], b'"' | b'\'') {
                    skip_quoted(code, index)
                } else {
                    index
                        + code[index..]
                            .find(|character: char| {
                                !(character.is_alphanumeric()
                                    || matches!(character, '_' | '-' | '.' | '/' | '@'))
                            })
                            .unwrap_or(code.len() - index)
                };
                let after = skip_blanks(code, end);
                let separated = bytes.get(after) == Some(&separator)
                    && !(separator == b'=' && bytes.get(after + 1) == Some(&b'='));
                if end > start && separated {
                    on(Event::Key(InlineKey {
                        start,
                        end,
                        after_separator: after + 1,
                    }));
                    index = after + 1;
                } else {
                    index = end.max(index + 1);
                }
            }
            _ => index += 1,
        }
    }
}

fn skip_blanks(code: &str, from: usize) -> usize {
    let from = from.min(code.len());
    from + (code[from..].len() - code[from..].trim_start_matches([' ', '\t']).len())
}

fn unquote(key: &str) -> String {
    let trimmed = key.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = trimmed
            .strip_prefix(quote)
            .and_then(|rest| rest.strip_suffix(quote))
        {
            return inner.to_string();
        }
    }
    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use SchemaPathSegment::{ArrayItem, Key};

    fn key(name: &str) -> SchemaPathSegment {
        Key(name.to_string())
    }

    #[test]
    fn keys_are_found_on_the_line_and_inside_its_containers() {
        let code = "acme = { secret = \"a, b = c\", max = 3 }";
        let names: Vec<&str> = inline_keys(code, b'=')
            .iter()
            .map(|found| &code[found.start..found.end])
            .collect();
        assert_eq!(names, ["acme", "secret", "max"]);
        let yaml = "acme: { secret: x, max: 3 }";
        let names: Vec<&str> = inline_keys(yaml, b':')
            .iter()
            .map(|found| &yaml[found.start..found.end])
            .collect();
        assert_eq!(names, ["acme", "secret", "max"]);
    }

    #[test]
    fn the_caret_resolves_keys_values_and_items_at_any_depth() {
        assert_eq!(
            inline_caret("acme = { se", b'='),
            Some(InlineCaret {
                path: vec![key("acme")],
                entry: InlineEntry::Key { start: 9 }
            })
        );
        assert_eq!(
            inline_caret("clients = { acme = { max = 1, mode = f", b'='),
            Some(InlineCaret {
                path: vec![key("clients"), key("acme")],
                entry: InlineEntry::Value {
                    key: "mode".to_string(),
                    start: 37
                }
            })
        );
        assert_eq!(
            inline_caret("a: { b: { c: 1 }, d", b':'),
            Some(InlineCaret {
                path: vec![key("a")],
                entry: InlineEntry::Key { start: 18 }
            })
        );
        assert_eq!(
            inline_caret("hosts = [\"a\", \"b", b'='),
            Some(InlineCaret {
                path: vec![key("hosts")],
                entry: InlineEntry::Item {
                    index: 1,
                    start: 14
                }
            })
        );
        assert_eq!(
            inline_caret("clients = [{ name = \"a\" }, { na", b'='),
            Some(InlineCaret {
                path: vec![key("clients"), ArrayItem(1)],
                entry: InlineEntry::Key { start: 29 }
            })
        );
    }

    #[test]
    fn the_top_level_of_a_line_is_not_inline() {
        assert_eq!(inline_caret("acme = ", b'='), None);
        assert_eq!(inline_caret("acme = { a = 1 }", b'='), None);
    }

    #[test]
    fn separators_inside_quotes_and_comparisons_are_not_keys() {
        assert!(inline_keys("a == b", b'=').is_empty());
        let code = "x = { \"k:=\" = 1 }";
        let found = inline_keys(code, b'=');
        assert_eq!(&code[found[1].start..found[1].end], "\"k:=\"");
    }

    #[test]
    fn inline_values_end_at_a_comma_or_closer() {
        assert_eq!(inline_value_len("fast, b = 1 }"), 4);
        assert_eq!(inline_value_len("\"a, b\" }"), 6);
        assert_eq!(inline_value_len("12 }"), 2);
    }
}
