//! The fields of the host's text by where they are written, for host ranges
//! placed while editing as another language: which field a range starts on
//! is a lookup by line here, where asking the language's `path_at` for each
//! range read the text from the top every time (thousands of ranges froze
//! the editor for seconds on every edit).

use std::collections::HashMap;

use serde_json::Value;

use super::super::language::ConfigLanguage;
use super::super::paths::{TextPosition, memo_key_span, memo_value_span};

/// Every field of a document, by pointer (the root left out), and whether
/// it holds a scalar.
pub(super) fn fields(document: &Value) -> Vec<(String, bool)> {
    fn walk(value: &Value, pointer: &mut String, found: &mut Vec<(String, bool)>) {
        let mut visit = |segment: &str, child: &Value, found: &mut Vec<(String, bool)>| {
            let length = pointer.len();
            pointer.push('/');
            pointer.push_str(&segment.replace('~', "~0").replace('/', "~1"));
            found.push((pointer.clone(), !(child.is_object() || child.is_array())));
            walk(child, pointer, found);
            pointer.truncate(length);
        };
        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    visit(key, child, found);
                }
            }
            Value::Array(items) => {
                for (index, child) in items.iter().enumerate() {
                    visit(&index.to_string(), child, found);
                }
            }
            _ => {}
        }
    }
    let mut found = Vec::new();
    walk(document, &mut String::new(), &mut found);
    found
}

/// A stretch of one line naming a field: its key, or its value written on
/// that line (a scalar: a container's span is only where it opens).
struct Written {
    start: usize,
    end: usize,
    pointer: usize,
    key: bool,
}

/// Where each field of a host's text is written, by line.
pub(super) struct HostFields {
    pointers: Vec<String>,
    by_line: HashMap<usize, Vec<Written>>,
}

impl HostFields {
    /// The `fields` of `text` (in `host`, see [`fields`]), from the spans
    /// the running batch read for them.
    pub(super) fn new(host: &dyn ConfigLanguage, text: &str, fields: Vec<(String, bool)>) -> Self {
        let mut by_line: HashMap<usize, Vec<Written>> = HashMap::new();
        let mut pointers = Vec::with_capacity(fields.len());
        for (index, (pointer, scalar)) in fields.into_iter().enumerate() {
            if let Some(span) = memo_key_span(host, text, &pointer).filter(|_| scalar) {
                by_line.entry(span.line).or_default().push(Written {
                    start: span.start_column,
                    end: span.end_column,
                    pointer: index,
                    key: true,
                });
            }
            if let Some(span) = memo_value_span(host, text, &pointer).filter(|_| scalar) {
                by_line.entry(span.line).or_default().push(Written {
                    start: span.start_column,
                    end: span.end_column,
                    pointer: index,
                    key: false,
                });
            }
            pointers.push(pointer);
        }
        Self { pointers, by_line }
    }

    /// The field whose key `position` is on, else the one whose value it is
    /// in (the narrowest); `None` when it is on neither (between them, on
    /// punctuation), which only reading the text can tell.
    pub(super) fn at(&self, position: TextPosition) -> Option<&str> {
        let written = self.by_line.get(&position.line)?;
        let column = position.column;
        let narrowest = |key: bool| {
            written
                .iter()
                .filter(|found| found.key == key && found.start <= column && column < found.end)
                .min_by_key(|found| found.end - found.start)
        };
        narrowest(true)
            .or_else(|| narrowest(false))
            .map(|found| self.pointers[found.pointer].as_str())
    }
}
