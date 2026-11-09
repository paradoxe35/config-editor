//! Completion and hover for flat key/value languages (`.env`, `.properties`:
//! one `KEY=value` entry per line, no nesting), from the separators of a
//! [`LanguageConfig`] alone. A language with such syntax delegates its
//! completion and hover hooks to these methods instead of writing them.

use std::collections::HashSet;

use serde_json::Value;

use super::keyboard::utf16_to_byte_index;
use super::language::{
    CompletionContext, LanguageConfig, SchemaPathSegment, Span, pointer_segments,
};

impl LanguageConfig {
    /// The separator a flat language writes after a new key: its
    /// [`written_separator`](Self::written_separator), else its first
    /// [`separators`](Self::separators) entry as it is (`KEY=`). `None`
    /// without separators.
    ///
    /// ```
    /// use config_editor_leptos::language::LanguageConfig;
    /// let env = LanguageConfig { separators: &["="], ..LanguageConfig::default() };
    /// assert_eq!(env.flat_separator().as_deref(), Some("="));
    /// let colon = LanguageConfig {
    ///     separators: &[":"],
    ///     written_separator: Some(": "),
    ///     ..LanguageConfig::default()
    /// };
    /// assert_eq!(colon.flat_separator().as_deref(), Some(": "));
    /// ```
    pub fn flat_separator(&self) -> Option<String> {
        let separator = self.separators.first()?;
        Some(self.written_separator.unwrap_or(separator).to_string())
    }

    /// [`ConfigLanguage::completion_context`](super::language::ConfigLanguage::completion_context)
    /// for a flat language: before the line's first separator the caret is
    /// at a top-level key (the word before it typed); after it, in that
    /// key's value. Nothing is offered on a comment line or without
    /// separators. `cursor` is a UTF-16 index, as the hook's.
    ///
    /// ```
    /// use config_editor_leptos::language::LanguageConfig;
    /// let env = LanguageConfig { separators: &["="], ..LanguageConfig::default() };
    /// let key = env.flat_completion_context("PORT=1\nLO", 9);
    /// assert_eq!((key.typed.as_str(), key.value_key), ("LO", None));
    /// let value = env.flat_completion_context("LOG=de", 6);
    /// assert_eq!((value.typed.as_str(), value.value_key.as_deref()), ("de", Some("LOG")));
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
        let before = &source[line_start..cursor_byte];
        let code = before.trim_start();
        let base_indent = before[..before.len() - code.len()].to_string();
        if self.is_comment_line(code) {
            return nothing;
        }
        let units = |text: &str| text.encode_utf16().count();
        match self.first_separator(code) {
            Some((at, separator)) => {
                let key = code[..at].trim();
                if key.is_empty() {
                    return nothing;
                }
                let after = &code[at + separator.len()..];
                let typed = after.trim_start();
                CompletionContext {
                    path: Vec::new(),
                    typed: typed.to_string(),
                    replace_start: cursor - units(typed),
                    replace_end: cursor,
                    value_key: Some(key.to_string()),
                    // A value goes after the spacing a new entry writes.
                    value_needs_space: after.is_empty()
                        && self
                            .flat_separator()
                            .is_some_and(|written| written.ends_with(' ')),
                    base_indent,
                    ..CompletionContext::default()
                }
            }
            None => {
                // The word before the caret (`export LO|` types `LO`).
                let typed = code.rsplit(char::is_whitespace).next().unwrap_or_default();
                CompletionContext {
                    path: Vec::new(),
                    typed: typed.to_string(),
                    replace_start: cursor - units(typed),
                    replace_end: cursor,
                    base_indent,
                    ..CompletionContext::default()
                }
            }
        }
    }

    /// [`ConfigLanguage::existing_keys`](super::language::ConfigLanguage::existing_keys)
    /// for a flat language: the keys written before a separator on every
    /// line but the caret's (`cursor_byte` is a byte index), at the top
    /// level only.
    ///
    /// ```
    /// use config_editor_leptos::language::LanguageConfig;
    /// let env = LanguageConfig { separators: &["="], line_comments: &["#"], ..LanguageConfig::default() };
    /// let keys = env.flat_existing_keys("# A=1\nPORT=1\nLO", 15, &[]).unwrap();
    /// assert_eq!(keys.into_iter().collect::<Vec<_>>(), ["PORT"]);
    /// ```
    pub fn flat_existing_keys(
        &self,
        source: &str,
        cursor_byte: usize,
        path: &[SchemaPathSegment],
    ) -> Option<HashSet<String>> {
        if !path.is_empty() || self.separators.is_empty() {
            return None;
        }
        let cursor_byte = cursor_byte.min(source.len());
        let caret_line = source[..cursor_byte].matches('\n').count();
        Some(
            source
                .split('\n')
                .enumerate()
                .filter(|(index, _)| *index != caret_line)
                .filter_map(|(_, line)| {
                    let code = line.trim_start();
                    if self.is_comment_line(code) {
                        return None;
                    }
                    let (at, _) = self.first_separator(code)?;
                    let key = code[..at].trim();
                    // `export KEY=…` names `KEY`.
                    let key = key.rsplit(char::is_whitespace).next().unwrap_or(key);
                    (!key.is_empty()).then(|| key.to_string())
                })
                .collect(),
        )
    }

    /// [`ConfigLanguage::property_insert`](super::language::ConfigLanguage::property_insert)
    /// for a flat language: the key and [`flat_separator`](Self::flat_separator),
    /// the caret after them. `None` without separators.
    ///
    /// ```
    /// use config_editor_leptos::language::LanguageConfig;
    /// let env = LanguageConfig { separators: &["="], ..LanguageConfig::default() };
    /// assert_eq!(env.flat_property_insert("PORT"), Some(("PORT=".to_string(), 5)));
    /// ```
    pub fn flat_property_insert(&self, key: &str) -> Option<(String, usize)> {
        let text = format!("{key}{}", self.flat_separator()?);
        let caret = text.encode_utf16().count();
        Some((text, caret))
    }

    /// [`ConfigLanguage::hover_key`](super::language::ConfigLanguage::hover_key)
    /// for a flat language: the key before the line's first separator when
    /// character `column` is on it, and the column it starts at.
    ///
    /// ```
    /// use config_editor_leptos::language::LanguageConfig;
    /// let env = LanguageConfig { separators: &["="], ..LanguageConfig::default() };
    /// assert_eq!(env.flat_hover_key("  PORT=1", 3), Some(("PORT".to_string(), 2)));
    /// assert_eq!(env.flat_hover_key("  PORT=1", 7), None);
    /// ```
    pub fn flat_hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        let code = line.trim_start();
        if self.is_comment_line(code) {
            return None;
        }
        let (at, _) = self.first_separator(code)?;
        let written = code[..at].trim_end();
        let key = written
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or(written);
        if key.is_empty() {
            return None;
        }
        let key_byte = line.len() - code.len() + written.len() - key.len();
        let start = line[..key_byte].chars().count();
        (start..start + key.chars().count())
            .contains(&column)
            .then(|| (key.to_string(), start))
    }

    /// [`ConfigLanguage::format_value`](super::language::ConfigLanguage::format_value)
    /// for a flat language: a string bare when it reads back as written
    /// (no blanks around it, no quote, comment marker, separator or line
    /// break), else JSON-quoted; other values as JSON.
    ///
    /// ```
    /// use config_editor_leptos::language::LanguageConfig;
    /// use serde_json::json;
    /// let env = LanguageConfig { separators: &["="], line_comments: &["#"], ..LanguageConfig::default() };
    /// assert_eq!(env.flat_format_value(&json!("debug")), "debug");
    /// assert_eq!(env.flat_format_value(&json!("a b")), "\"a b\"");
    /// assert_eq!(env.flat_format_value(&json!(true)), "true");
    /// ```
    pub fn flat_format_value(&self, value: &Value) -> String {
        match value {
            Value::String(text)
                if !text.is_empty()
                    && text.trim() == text
                    && !text.contains(['"', '\'', '\n', '\r', ' ', '\t'])
                    && !self
                        .separators
                        .iter()
                        .chain(self.line_comments)
                        .any(|marker| text.contains(marker)) =>
            {
                text.clone()
            }
            other => other.to_string(),
        }
    }

    /// [`ConfigLanguage::format_value_like`](super::language::ConfigLanguage::format_value_like)
    /// for a language known by its syntax alone: `formatted` (the value as
    /// the language writes it), or a string in the quotes `written` is in
    /// (`'two'`) when `formatted` is not and those quotes hold it as it is
    /// (no such quote, control character or line separator inside, and no
    /// backslash unless it is one of the
    /// [`literal_quotes`](Self::literal_quotes)).
    ///
    /// ```
    /// use config_editor_leptos::language::LanguageConfig;
    /// use serde_json::json;
    /// let dot = LanguageConfig { quotes: &["\"", "'"], literal_quotes: &["'"], ..LanguageConfig::default() };
    /// assert_eq!(dot.quoted_like(&json!("three"), "'two'", "three".into()), "'three'");
    /// assert_eq!(dot.quoted_like(&json!("a\\b"), "'two'", "a\\b".into()), "'a\\b'");
    /// assert_eq!(dot.quoted_like(&json!("it's"), "'two'", "\"it's\"".into()), "\"it's\"");
    /// assert_eq!(dot.quoted_like(&json!("three"), "two", "three".into()), "three");
    /// assert_eq!(dot.quoted_like(&json!(3), "'two'", "3".into()), "3");
    /// ```
    pub fn quoted_like(&self, value: &Value, written: &str, formatted: String) -> String {
        let Value::String(text) = value else {
            return formatted;
        };
        let written = written.trim();
        let Some(quote) = self
            .quotes
            .iter()
            .find(|quote| written.len() >= 2 * quote.len() && written.starts_with(**quote))
        else {
            return formatted;
        };
        let long = self
            .quotes
            .iter()
            .any(|other| other.len() > quote.len() && written.starts_with(*other));
        let literal = self.literal_quotes.contains(quote);
        let fits = written.ends_with(quote)
            && !long
            && !formatted.starts_with(quote)
            && !text.contains(quote)
            && !text.chars().any(|character| {
                character.is_control() || matches!(character, '\u{2028}' | '\u{2029}')
            })
            && (literal || !text.contains('\\'));
        if fits {
            format!("{quote}{text}{quote}")
        } else {
            formatted
        }
    }

    /// [`ConfigLanguage::key_span`](super::language::ConfigLanguage::key_span)
    /// for a flat language: the key of a top-level `pointer` on the last
    /// line naming it (a later entry wins). `None` for a nested pointer or
    /// without separators.
    ///
    /// ```
    /// use config_editor_leptos::language::{LanguageConfig, Span};
    /// let env = LanguageConfig { separators: &["="], ..LanguageConfig::default() };
    /// let span = env.flat_key_span("A=1\nexport PORT = 80\n", "/PORT").unwrap();
    /// assert_eq!((span.line, span.start_column, span.end_column), (2, 8, 12));
    /// ```
    pub fn flat_key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        let (line, entry) = self.flat_entry(source, pointer)?;
        Some(Span {
            line,
            start_column: entry.key.0,
            end_column: entry.key.1,
        })
    }

    /// [`ConfigLanguage::value_span`](super::language::ConfigLanguage::value_span)
    /// for a flat language: the value of a top-level `pointer` on the last
    /// line naming it, from the first character after the separator's
    /// spaces to the last one that is not blank before a line comment (a
    /// declared marker after whitespace, outside quotes). `None` for a
    /// nested pointer, an empty value, or without separators.
    ///
    /// ```
    /// use config_editor_leptos::language::{LanguageConfig, Span};
    /// let env = LanguageConfig { separators: &["="], line_comments: &["#"], quotes: &["\""], ..LanguageConfig::default() };
    /// let span = env.flat_value_span("PORT = 80 \n", "/PORT").unwrap();
    /// assert_eq!((span.line, span.start_column, span.end_column), (1, 8, 10));
    /// let span = env.flat_value_span("PORT = 80  # c\n", "/PORT").unwrap();
    /// assert_eq!((span.start_column, span.end_column), (8, 10));
    /// let span = env.flat_value_span("TAG=a#b # c\n", "/TAG").unwrap();
    /// assert_eq!((span.start_column, span.end_column), (5, 8));
    /// let span = env.flat_value_span("S = \"a # b\" # c\n", "/S").unwrap();
    /// assert_eq!((span.start_column, span.end_column), (5, 12));
    /// assert_eq!(env.flat_value_span("PORT=\n", "/PORT"), None);
    /// assert_eq!(env.flat_value_span("PORT= # c\n", "/PORT"), None);
    /// ```
    pub fn flat_value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        let (line, entry) = self.flat_entry(source, pointer)?;
        let (start, end) = entry.value?;
        Some(Span {
            line,
            start_column: start,
            end_column: end,
        })
    }

    /// The last line writing the top-level `pointer`: its one-based number
    /// and where its key and value are.
    fn flat_entry(&self, source: &str, pointer: &str) -> Option<(usize, FlatEntry)> {
        let [key] = pointer_segments(pointer).try_into().ok()?;
        source
            .split('\n')
            .enumerate()
            .filter_map(|(index, line)| {
                let entry = self.flat_line(line)?;
                (entry.name == key).then_some((index + 1, entry))
            })
            .last()
    }

    /// One line's entry, columns one-based in characters.
    fn flat_line(&self, line: &str) -> Option<FlatEntry> {
        let indent = line.len() - line.trim_start().len();
        let code = &line[indent..];
        if self.is_comment_line(code) {
            return None;
        }
        let (at, separator) = self.first_separator(code)?;
        let written = code[..at].trim_end();
        let name = written
            .rsplit(char::is_whitespace)
            .next()
            .unwrap_or(written);
        if name.is_empty() {
            return None;
        }
        let column = |byte: usize| line[..byte].chars().count() + 1;
        let key_start = indent + written.len() - name.len();
        let after = indent + at + separator.len();
        let rest = &line[after..self.value_code_end(line, after)];
        let value_start = after + rest.len() - rest.trim_start().len();
        let value_end = after + rest.trim_end().len();
        Some(FlatEntry {
            name: name.to_string(),
            key: (column(key_start), column(key_start + name.len())),
            value: (value_start < value_end).then(|| (column(value_start), column(value_end))),
        })
    }

    /// Where the value from byte `start` of `line` ends: at a line comment
    /// marker after whitespace, outside a string in the language's quotes
    /// (a marker right after a character is data, as in `.env`'s
    /// `KEY=a#b`); else the line's end.
    fn value_code_end(&self, line: &str, start: usize) -> usize {
        let mut quote: Option<&str> = None;
        let mut after_blank = true;
        let mut at = start;
        while let Some(character) = line[at..].chars().next() {
            let rest = &line[at..];
            let mut step = character.len_utf8();
            match quote {
                Some(open) if character == '\\' && !self.literal_quotes.contains(&open) => {
                    step += rest[step..].chars().next().map_or(0, char::len_utf8);
                }
                Some(open) if rest.starts_with(open) => {
                    quote = None;
                    step = open.len();
                }
                Some(_) => {}
                None => {
                    if after_blank
                        && self
                            .line_comments
                            .iter()
                            .any(|marker| rest.starts_with(marker))
                    {
                        return at;
                    }
                    // A quote opens a string where a token starts (`it's` is
                    // a word).
                    let opened = self
                        .quotes
                        .iter()
                        .copied()
                        .find(|open| rest.starts_with(open));
                    if let Some(open) = opened.filter(|_| after_blank) {
                        quote = Some(open);
                        step = open.len();
                    }
                }
            }
            after_blank = character.is_whitespace();
            at += step;
        }
        line.len()
    }

    fn is_comment_line(&self, code: &str) -> bool {
        self.line_comments
            .iter()
            .any(|marker| code.starts_with(marker))
    }

    /// The leftmost separator in `code`: its byte offset and the separator.
    fn first_separator(&self, code: &str) -> Option<(usize, &'static str)> {
        self.separators
            .iter()
            .filter_map(|separator| code.find(separator).map(|at| (at, *separator)))
            .min_by_key(|(at, separator)| (*at, usize::MAX - separator.len()))
    }
}

/// A flat line's entry: its key's name, and the columns of the key and of
/// the value (none when empty).
struct FlatEntry {
    name: String,
    key: (usize, usize),
    value: Option<(usize, usize)>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENV: LanguageConfig = LanguageConfig {
        line_comments: &["#"],
        block_comment: None,
        brackets: &[],
        separators: &["="],
        quotes: &["\""],
        literal_quotes: &[],
        map_brackets: None,
        list_brackets: None,
        item_separator: None,
        item_marker: None,
        written_separator: None,
    };

    const COLON: LanguageConfig = LanguageConfig {
        separators: &[":"],
        written_separator: Some(": "),
        ..ENV
    };

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

    #[test]
    fn no_separators_no_context() {
        let none = LanguageConfig::default();
        assert!(none.flat_completion_context("PO", 2).no_properties);
        assert_eq!(none.flat_property_insert("PO"), None);
        assert_eq!(none.flat_existing_keys("A=1\n", 0, &[]), None);
        assert_eq!(none.flat_hover_key("A=1", 0), None);
    }

    #[test]
    fn existing_keys_skip_the_caret_line_and_comments() {
        let source = "A=1\n# B=2\nexport C=3\nD\nE=";
        let keys = ENV.flat_existing_keys(source, source.len(), &[]).unwrap();
        let mut keys: Vec<_> = keys.into_iter().collect();
        keys.sort();
        assert_eq!(keys, ["A", "C"]);
        assert_eq!(
            ENV.flat_existing_keys(source, 0, &[SchemaPathSegment::Key("A".into())]),
            None
        );
    }

    #[test]
    fn inserts_and_hovers() {
        assert_eq!(ENV.flat_property_insert("é"), Some(("é=".to_string(), 2)));
        assert_eq!(
            COLON.flat_property_insert("level"),
            Some(("level: ".to_string(), 7))
        );
        assert_eq!(
            ENV.flat_hover_key("export PORT=1", 8),
            Some(("PORT".to_string(), 7))
        );
        assert_eq!(ENV.flat_hover_key("export PORT=1", 2), None);
        assert_eq!(ENV.flat_hover_key("# PORT=1", 3), None);
        assert_eq!(ENV.flat_hover_key("PORT", 1), None);
        assert_eq!(ENV.flat_hover_key("éé=1", 1), Some(("éé".to_string(), 0)));
    }

    #[test]
    fn values_bare_when_they_read_back() {
        let cases = [
            (serde_json::json!("debug"), "debug"),
            (serde_json::json!(""), "\"\""),
            (serde_json::json!("a=b"), "\"a=b\""),
            (serde_json::json!("a#b"), "\"a#b\""),
            (serde_json::json!(8080), "8080"),
            (serde_json::json!(false), "false"),
        ];
        for (value, written) in cases {
            assert_eq!(ENV.flat_format_value(&value), written, "{value}");
        }
    }
}
