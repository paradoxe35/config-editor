//! Where the caret is, for a language known by its declared syntax: the
//! line's entry, the containers opened before the caret on it (its map and
//! list brackets, item separator and quotes), and for a line starting with
//! the item marker, the item of the list whose key is written above it.

use super::super::flow_syntax::Flow;
use super::super::keyboard::utf16_to_byte_index;
use super::super::language::{CompletionContext, EntryPosition, LanguageConfig, SchemaPathSegment};

impl LanguageConfig {
    /// [`ConfigLanguage::completion_context`](super::super::language::ConfigLanguage::completion_context)
    /// for a language known by its declared syntax: before the line's first
    /// separator the caret is at a top-level key (the word before it typed);
    /// after it, in that key's value, or inside the maps and lists opened
    /// there (`tcp: <nodelay: true; |>` is at a key of `tcp`). A line
    /// starting with the [`item_marker`](Self::item_marker), indented below
    /// a key with no value, is an item of that key's list. Typed in front
    /// of an entry (`hea|tcp: 1`), a new entry goes before it, or renames
    /// it when the suggestion completes its key. Nothing is offered on a
    /// comment line or without separators. `cursor` is a UTF-16 index, as
    /// the hook's.
    ///
    /// ```
    /// use leptos_config_editor::language::{LanguageConfig, SchemaPathSegment};
    /// let env = LanguageConfig { separators: &["="], ..LanguageConfig::default() };
    /// let key = env.flat_completion_context("PORT=1\nLO", 9);
    /// assert_eq!((key.typed.as_str(), key.value_key), ("LO", None));
    /// let value = env.flat_completion_context("LOG=de", 6);
    /// assert_eq!((value.typed.as_str(), value.value_key.as_deref()), ("de", Some("LOG")));
    /// let dot = LanguageConfig {
    ///     separators: &[":"],
    ///     map_brackets: Some(('<', '>')),
    ///     item_separator: Some(';'),
    ///     ..LanguageConfig::default()
    /// };
    /// let nested = dot.flat_completion_context("tcp: <a: 1; b>", 13);
    /// assert_eq!(nested.path, [SchemaPathSegment::Key("tcp".into())]);
    /// assert_eq!(nested.typed, "b");
    /// ```
    pub fn flat_completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        let nothing = CompletionContext {
            no_properties: true,
            replace_start: cursor,
            replace_end: cursor,
            ..CompletionContext::default()
        };
        if self.separators.is_empty() {
            return nothing;
        }
        let cursor_byte = utf16_to_byte_index(source, cursor.min(source.encode_utf16().count()));
        let line_start = source[..cursor_byte].rfind('\n').map_or(0, |at| at + 1);
        let line_end = source[cursor_byte..]
            .find('\n')
            .map_or(source.len(), |at| cursor_byte + at);
        let before = &source[line_start..cursor_byte];
        let rest = &source[cursor_byte..line_end];
        let code = before.trim_start();
        let base_indent = before[..before.len() - code.len()].to_string();
        if self.is_comment_line(code) {
            return nothing;
        }
        let (mut path, body, root) = match self.item_of_list_above(source, line_start, before) {
            Some((path, body)) => (path, body, Root::Item),
            None => (Vec::new(), code, Root::Entry),
        };
        let flow = Flow::from_config(*self);
        let frames = read_frames(self, &flow, body, root);
        let frame = frames.last().expect("the line's own frame stays");
        let nested = frames.len() > 1;
        path.extend(frames.iter().filter_map(|frame| frame.segment.clone()));
        let units = |text: &str| text.encode_utf16().count();
        let mut context = CompletionContext {
            path,
            replace_end: cursor,
            base_indent,
            ..CompletionContext::default()
        };
        if frame.list {
            let typed = body[frame.start..].trim_start();
            if nested {
                context.path.push(SchemaPathSegment::ArrayItem(frame.index));
            }
            context.is_array_item = true;
            context.typed = typed.to_string();
            context.replace_start = cursor - units(typed);
            return context;
        }
        if let Some(key) = &frame.key {
            if key.is_empty() {
                return nothing;
            }
            let after = &body[frame.value_start..];
            let typed = after.trim_start();
            context.typed = typed.to_string();
            context.replace_start = cursor - units(typed);
            context.value_key = Some(key.clone());
            // A value goes after the spacing a new entry writes.
            context.value_needs_space = after.is_empty()
                && self
                    .flat_separator()
                    .is_some_and(|written| written.ends_with(' '));
            return context;
        }
        // The word before the caret (`export LO|` types `LO`).
        let typed = body[frame.start..]
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or_default();
        context.typed = typed.to_string();
        context.replace_start = cursor - units(typed);
        if nested {
            // An entry of a map written on one line stays on it.
            context.position = EntryPosition::Inline;
        }
        let word: String = rest
            .chars()
            .take_while(|character| character.is_alphanumeric() || matches!(character, '_' | '-'))
            .collect();
        if word.is_empty() {
            return context;
        }
        let entry_follows = self
            .first_separator(&rest[word.len()..])
            .is_some_and(|(at, _)| rest[word.len()..][..at].trim().is_empty());
        if entry_follows {
            // The entry after the caret moves past the new one: beside it in
            // a container, to a line of its own at the top level.
            context.entry_suffix = match flow.separator().filter(|_| nested) {
                Some(separator) => format!("{separator} "),
                None => format!("\n{}", context.base_indent),
            };
            context.entry_after = Some(word);
        } else {
            // A bare word after the caret is the rest of the key typed.
            context.replace_end += units(&word);
        }
        context
    }

    /// When the line before the caret (`before`, the line starting at byte
    /// `line_start`) starts with the item marker, indented below a line
    /// writing a key with no value, with only items between them: the path
    /// of that item (the key and its index) and the text after the marker.
    fn item_of_list_above<'a>(
        &self,
        source: &str,
        line_start: usize,
        before: &'a str,
    ) -> Option<(Vec<SchemaPathSegment>, &'a str)> {
        let marker = self.item_marker?;
        let code = before.trim_start();
        let indent = before.len() - code.len();
        let body = code
            .strip_prefix(marker)
            .filter(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))?;
        if indent == 0 {
            return None;
        }
        let above = source[..line_start].strip_suffix('\n')?;
        let mut index = 0;
        for line in above.split('\n').rev() {
            let code = line.trim_start();
            if code.is_empty() || self.is_comment_line(code) {
                continue;
            }
            let depth = line.len() - code.len();
            let item = code
                .strip_prefix(marker)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace));
            if item && depth > 0 {
                index += 1;
                continue;
            }
            let entry = self.flat_line(line)?;
            return (depth < indent && entry.value.is_none()).then(|| {
                (
                    vec![
                        SchemaPathSegment::Key(entry.name),
                        SchemaPathSegment::ArrayItem(index),
                    ],
                    body,
                )
            });
        }
        None
    }
}

/// What a line's text after any item marker holds: an entry (`key: value`)
/// or an item's value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Root {
    Entry,
    Item,
}

/// A map or list open before the caret, or the line itself.
struct Frame {
    list: bool,
    /// Its place in the container around it (none for the line).
    segment: Option<SchemaPathSegment>,
    /// The index of the item the caret's text is in, in a list.
    index: usize,
    /// The key of the entry being written, once its separator is.
    key: Option<String>,
    /// Where the entry or item being written starts.
    start: usize,
    /// Where its value starts, after the key's separator.
    value_start: usize,
}

/// The containers `body` (the line's text before the caret, after any item
/// marker) leaves open, outermost first, the line's own frame first.
fn read_frames(config: &LanguageConfig, flow: &Flow, body: &str, root: Root) -> Vec<Frame> {
    let mut frames = vec![Frame {
        list: root == Root::Item,
        segment: None,
        index: 0,
        key: None,
        start: 0,
        value_start: 0,
    }];
    let mut at = 0;
    while let Some(character) = body[at..].chars().next() {
        if flow.quote(character).is_some() {
            at = flow.quote_end(body, at);
            continue;
        }
        let length = character.len_utf8();
        let nested = frames.len() > 1;
        let frame = frames.last_mut().expect("the line's own frame stays");
        let in_value = frame.list || frame.key.is_some();
        let opens_map = flow.map().is_some_and(|(open, _)| open == character);
        let opens_list = flow.list().is_some_and(|(open, _)| open == character);
        if in_value && (opens_map || opens_list) {
            let segment = if frame.list {
                Some(SchemaPathSegment::ArrayItem(frame.index))
            } else {
                frame.key.clone().map(SchemaPathSegment::Key)
            };
            // The line's own item is already in the path.
            let segment = segment.filter(|_| nested || root == Root::Entry);
            frames.push(Frame {
                list: opens_list,
                segment,
                index: 0,
                key: None,
                start: at + length,
                value_start: at + length,
            });
        } else if nested && flow.opener_of(character).is_some() {
            frames.pop();
        } else if nested && flow.is_separator(character) {
            if frame.list {
                frame.index += 1;
            }
            frame.key = None;
            frame.start = at + length;
        } else if !frame.list
            && frame.key.is_none()
            && let Some(separator) = config
                .separators
                .iter()
                .filter(|separator| body[at..].starts_with(**separator))
                .max_by_key(|separator| separator.len())
        {
            frame.key = Some(flow.strip_quotes(&body[frame.start..at]));
            frame.value_start = at + separator.len();
            at += separator.len();
            continue;
        }
        at += length;
    }
    frames
}

#[cfg(test)]
mod tests {
    use super::super::tests::{COLON, ENV};
    use super::*;

    #[test]
    fn contexts_on_flat_lines() {
        // (source with `|` for the caret, typed, value key, needs a space)
        type Expected = Option<(&'static str, Option<&'static str>, bool)>;
        let cases: &[(&LanguageConfig, &str, Expected)] = &[
            (&ENV, "|", Some(("", None, false))),
            (&ENV, "PO|", Some(("PO", None, false))),
            (&ENV, "A=1\n  PO|", Some(("PO", None, false))),
            (&ENV, "export PO|", Some(("PO", None, false))),
            (&ENV, "PORT=|", Some(("", Some("PORT"), false))),
            (&ENV, "PORT = 80|", Some(("80", Some("PORT"), false))),
            (&ENV, "URL=a=b|", Some(("a=b", Some("URL"), false))),
            (&COLON, "level:|", Some(("", Some("level"), true))),
            (&COLON, "level: d|", Some(("d", Some("level"), false))),
            (&ENV, "# PO|", None),
            (&ENV, "=x|", None),
        ];
        let mut failures = Vec::new();
        for (config, marked, expected) in cases {
            let cursor = marked.find('|').unwrap();
            let source = marked.replace('|', "");
            let context = config.flat_completion_context(&source, cursor);
            let got = (!context.no_properties).then_some({
                (
                    context.typed.as_str(),
                    context.value_key.as_deref(),
                    context.value_needs_space,
                )
            });
            let replace_ok = context.replace_end == cursor
                && context.replace_start == cursor - context.typed.len();
            if got != *expected || !replace_ok {
                failures.push(format!("{marked:?}: {got:?}, expected {expected:?}"));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    fn dot() -> LanguageConfig {
        LanguageConfig {
            line_comments: &["#"],
            separators: &[":"],
            quotes: &["'"],
            brackets: &[('<', '>'), ('(', ')')],
            map_brackets: Some(('<', '>')),
            list_brackets: Some(('(', ')')),
            item_separator: Some(';'),
            item_marker: Some("*"),
            written_separator: Some(": "),
            ..LanguageConfig::default()
        }
    }

    fn at(marked: &str) -> CompletionContext {
        let cursor = marked[..marked.find('|').unwrap()].encode_utf16().count();
        dot().flat_completion_context(&marked.replacen('|', "", 1), cursor)
    }

    fn key(name: &str) -> SchemaPathSegment {
        SchemaPathSegment::Key(name.to_string())
    }

    #[test]
    fn a_key_typed_in_front_of_an_entry_goes_before_it_or_renames_it() {
        let top = at("a: 1\nhea|tcp: <b: 1>\n");
        assert_eq!(
            (
                top.typed.as_str(),
                top.entry_after.as_deref(),
                top.entry_suffix.as_str()
            ),
            ("hea", Some("tcp"), "\n")
        );
        let nested = at("tcp: <k|nodelay: true>\n");
        assert_eq!(nested.path, [key("tcp")]);
        assert_eq!(
            (nested.entry_after.as_deref(), nested.entry_suffix.as_str()),
            (Some("nodelay"), "; ")
        );
        assert_eq!(nested.position, EntryPosition::Inline);
        // A bare word after the caret is the rest of the key typed.
        let bare = at("PO|RT");
        assert_eq!(
            (bare.replace_start, bare.replace_end, bare.entry_after),
            (0, 4, None)
        );
    }

    #[test]
    fn the_caret_inside_one_line_values_is_at_their_keys_and_items() {
        let entry = at("tcp: <nodelay: true; |>\n");
        assert_eq!((entry.path, entry.value_key), (vec![key("tcp")], None));
        let value = at("tcp: <nodelay: t|>\n");
        assert_eq!(value.path, [key("tcp")]);
        assert_eq!(
            (value.value_key.as_deref(), value.typed.as_str()),
            (Some("nodelay"), "t")
        );
        let quoted = at("tcp: <a: 'x;>'; |>\n");
        assert_eq!((quoted.path, quoted.value_key), (vec![key("tcp")], None));
        let item = at("ports: (1; |)\n");
        assert_eq!(item.path, [key("ports"), SchemaPathSegment::ArrayItem(1)]);
        assert!(item.is_array_item);
        let deeper = at("a: (<b: (1; <c: 1; |>)>)\n");
        assert_eq!(
            deeper.path,
            [
                key("a"),
                SchemaPathSegment::ArrayItem(0),
                key("b"),
                SchemaPathSegment::ArrayItem(1),
            ]
        );
    }

    #[test]
    fn an_item_line_is_an_item_of_the_list_above() {
        let second = at("ws:\n  * <p: 1>\n  # c\n  * <|>\n");
        assert_eq!(second.path, [key("ws"), SchemaPathSegment::ArrayItem(1)]);
        assert_eq!(second.value_key, None);
        let bare = at("ws:\n  * |\n");
        assert_eq!(bare.path, [key("ws"), SchemaPathSegment::ArrayItem(0)]);
        assert!(bare.is_array_item);
        // Below a key with a value, a marker starts no item.
        assert!(at("ws: 1\n  * <|>\n").path.is_empty());
    }
}
