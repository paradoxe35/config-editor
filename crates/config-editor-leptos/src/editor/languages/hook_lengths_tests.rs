//! A length a language's hook gives of what starts a text (the markup
//! closing or opening an element, a container's closer) is checked where
//! the editor reads it: one past the text or inside a character counts as
//! none, so the shared code cutting the text there never panics (P5).

use super::super::paths::{field_range, joined_value_ranges, path_at};
use super::Language;
use crate::editor::language::*;
use serde_json::Value;

/// XML in everything but its lengths, each `extra` bytes too long.
struct Overlong(usize);

fn xml() -> &'static dyn ConfigLanguage {
    Language::XML.behavior()
}

impl ConfigLanguage for Overlong {
    fn info(&self) -> LanguageInfo {
        xml().info()
    }

    fn config(&self) -> LanguageConfig {
        xml().config()
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        xml().parse(source)
    }

    fn serialize(&self, value: &Value, previous: Option<&str>) -> Result<String, String> {
        xml().serialize(value, previous)
    }

    fn completion_context(&self, source: &str, cursor: usize) -> CompletionContext {
        xml().completion_context(source, cursor)
    }

    fn hover_key(&self, line: &str, column: usize) -> Option<(String, usize)> {
        xml().hover_key(line, column)
    }

    fn text(&self) -> &dyn Text {
        self
    }

    fn markup(&self) -> Option<&dyn Markup> {
        Some(self)
    }
}

impl Text for Overlong {
    fn container_closer_len(&self, _language: &dyn ConfigLanguage, text: &str) -> Option<usize> {
        Some(xml().container_closer_len(text).unwrap_or(0) + self.0)
    }

    fn unclosed_containers(&self, _language: &dyn ConfigLanguage, text: &str) -> usize {
        xml().unclosed_containers(text) + 1
    }
}

impl Markup for Overlong {
    fn delimiters(&self) -> (char, char) {
        ('<', '>')
    }

    fn closing_markup_len(&self, text: &str) -> Option<usize> {
        Some(xml().closing_markup_len(text).unwrap_or(0) + self.0)
    }

    fn key_markup_len(&self, text: &str) -> Option<usize> {
        Some(xml().key_markup_len(text).unwrap_or(0) + self.0)
    }
}

#[test]
fn lengths_past_the_text_or_inside_a_character_cut_nothing() {
    let texts = [
        "<a>é</a>\n",
        "<a>\n  <b>ü</b>\n  <c>1</c>\n</a>\n",
        "<r><é>x</é><b>😀</b></r>",
    ];
    for extra in [1, 2, 1000] {
        let language = Overlong(extra);
        for text in texts {
            for cursor in 0..=text.encode_utf16().count() {
                let _ = path_at(&language, text, cursor);
            }
            for pointer in ["/a", "/a/b", "/r/b", "/a/c"] {
                let _ = field_range(&language, text, pointer);
                let _ = joined_value_ranges(&language, text, pointer);
            }
        }
    }
}
