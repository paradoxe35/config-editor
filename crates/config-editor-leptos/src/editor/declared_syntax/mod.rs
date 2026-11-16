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

use super::language::{
    CompletionContext, ConfigLanguage, InsertContext, InsertPlan, LanguageConfig, LanguageInfo,
    ParseError, SaveLayout, Span, Typing, flat_value_text, flat_write_insert,
};

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

    /// As the bridge reads a language without `contextAt`; DotBare keeps
    /// the trait's default, as it declares no brackets to read the caret
    /// inside the one-line values the shared suggestion cases write.
    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        if self.full {
            self.config().flat_completion_context(source, cursor)
        } else {
            CompletionContext::default()
        }
    }

    fn write_insert(
        &self,
        key: &str,
        plan: &InsertPlan<'_>,
        context: &InsertContext<'_>,
    ) -> Option<(String, usize)> {
        flat_write_insert(self, key, plan, context)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        self.config().flat_hover_key(line, column)
    }

    fn format_value(&self, value: &Value) -> String {
        flat_value_text(self, value)
    }

    fn typing(&self) -> &dyn Typing {
        self
    }

    fn layout(&self) -> &dyn SaveLayout {
        self
    }
}

impl Typing for Dot {
    fn empty_value_slots(&self, language: &dyn ConfigLanguage, text: &str) -> Vec<usize> {
        language.config().flat_empty_value_slots(text)
    }
}

impl SaveLayout for Dot {
    fn format_value_like(
        &self,
        _language: &dyn ConfigLanguage,
        value: &Value,
        written: &str,
    ) -> String {
        self.config()
            .quoted_like(value, written, self.format_value(value))
    }
}
