//! A language as a JavaScript host declares one (`registerLanguage`): a
//! `parse` and a `serialize`, its syntax only declared (`comments`,
//! `brackets`, `mapBrackets`, `itemMarker`, `literalQuotes`, …), and fields
//! found on their lines as for a flat language (no `keySpan` / `valueSpan`).
//! "Dot": `key: value` lines, `#` comments, maps `<a: 1; b: 'x'>` and lists
//! `(1; 2)` on one line, `'…'` literal strings, and `*` before each list item
//! written on a line of its own. Edit-as saves through it must work from
//! what it declares, as through a built-in language.

mod read;
mod save_tests;

use serde_json::Value;

use super::language::{ConfigLanguage, LanguageConfig, LanguageInfo, ParseError, Span};

/// Dot as declared: `full` with all its syntax, else only its comments and
/// separator (a host that declares no more).
pub(super) struct Dot {
    full: bool,
    /// Whether it declares `'` a literal quote (else a backslash in it
    /// reads as an escape, which its parser does not do).
    literal: bool,
}

pub(super) static DOT: Dot = Dot {
    full: true,
    literal: true,
};
pub(super) static DOT_BARE: Dot = Dot {
    full: false,
    literal: true,
};
/// Dot declared with lists and maps but no literal quotes.
pub(super) static DOT_ESCAPING: Dot = Dot {
    full: true,
    literal: false,
};

impl ConfigLanguage for Dot {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: match (self.full, self.literal) {
                (false, _) => "DotBare",
                (true, true) => "Dot",
                (true, false) => "DotEscaping",
            },
            icon: "D",
            extensions: &[],
        }
    }

    /// As the JavaScript bridge builds it from the definition.
    fn config(&self) -> LanguageConfig {
        let bare = LanguageConfig {
            line_comments: &["#"],
            separators: &[":"],
            quotes: &["\"", "'"],
            ..LanguageConfig::default()
        };
        if !self.full {
            return bare;
        }
        LanguageConfig {
            brackets: &[('<', '>'), ('(', ')')],
            literal_quotes: if self.literal { &["'"] } else { &[] },
            map_brackets: Some(('<', '>')),
            list_brackets: Some(('(', ')')),
            item_separator: Some(';'),
            item_marker: Some("*"),
            written_separator: Some(": "),
            ..bare
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        read::document(source)
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        read::write_document(value)
    }

    fn key_span(&self, source: &str, pointer: &str) -> Option<Span> {
        self.config().flat_key_span(source, pointer)
    }

    fn value_span(&self, source: &str, pointer: &str) -> Option<Span> {
        self.config().flat_value_span(source, pointer)
    }

    fn format_value(&self, value: &Value) -> String {
        self.config().flat_format_value(value)
    }

    fn format_value_like(&self, value: &Value, written: &str) -> String {
        self.config()
            .quoted_like(value, written, self.format_value(value))
    }
}
