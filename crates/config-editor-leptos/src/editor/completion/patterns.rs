//! `patternProperties` regexes, compiled once: when a schema is built, not
//! on every key lookup, and checked so one the engine cannot read is
//! reported instead of silently matching nothing.

use crate::editor::pattern_engine::Regex;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashMap;

/// Patterns kept compiled before the cache starts over; schemas use a
/// handful, so only a host loading many different schemas reaches it.
const MAX_CACHED_PATTERNS: usize = 4096;

thread_local! {
    /// Each pattern text compiled once, `None` when the engine rejects it.
    static COMPILED: RefCell<HashMap<String, Option<Regex>>> = RefCell::new(HashMap::new());
}

/// Whether `key` matches the regex `pattern`. A pattern the engine cannot
/// compile matches nothing; [`unsupported_patterns`] reports it.
pub(crate) fn pattern_matches(pattern: &str, key: &str) -> bool {
    COMPILED.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(compiled) = cache.get(pattern) {
            return compiled.as_ref().is_some_and(|regex| regex.is_match(key));
        }
        if cache.len() >= MAX_CACHED_PATTERNS {
            cache.clear();
        }
        let compiled = Regex::new(pattern).ok();
        let matched = compiled.as_ref().is_some_and(|regex| regex.is_match(key));
        cache.insert(pattern.to_string(), compiled);
        matched
    })
}

/// Whether `text` matches the regex `pattern`; `None` when the engine
/// cannot compile it.
pub(crate) fn pattern_decides(pattern: &str, text: &str) -> Option<bool> {
    let matched = pattern_matches(pattern, text);
    (matched || compiles(pattern)).then_some(matched)
}

/// Whether the engine can compile `pattern`, compiling it into the cache.
fn compiles(pattern: &str) -> bool {
    pattern_matches(pattern, "");
    COMPILED.with(|cache| cache.borrow().get(pattern).is_some_and(Option::is_some))
}

/// Compiles every `patternProperties` regex in `schema`, and describes each
/// one the engine cannot read: keys it was meant to match get no
/// completions or hover docs.
pub(crate) fn unsupported_patterns(schema: &Value) -> Vec<String> {
    let mut warnings = Vec::new();
    collect(schema, &mut String::new(), &mut warnings, 0);
    warnings
}

fn collect(schema: &Value, pointer: &mut String, warnings: &mut Vec<String>, depth: usize) {
    if depth > 64 {
        return;
    }
    let length = pointer.len();
    match schema {
        Value::Object(map) => {
            for (key, child) in map {
                pointer.push('/');
                pointer.push_str(&key.replace('~', "~0").replace('/', "~1"));
                if key == "patternProperties"
                    && let Some(patterns) = child.as_object()
                {
                    for pattern in patterns.keys().filter(|pattern| !compiles(pattern)) {
                        let place = pointer.as_str();
                        warnings.push(format!(
                            "{place}: the pattern \"{pattern}\" is not supported by the editor's \
                             regex engine; keys it matches get no completions or hover docs"
                        ));
                    }
                }
                collect(child, pointer, warnings, depth + 1);
                pointer.truncate(length);
            }
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                pointer.push('/');
                pointer.push_str(&index.to_string());
                collect(item, pointer, warnings, depth + 1);
                pointer.truncate(length);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_patterns_are_reported_once_each_with_their_place() {
        let schema = serde_json::json!({
            "properties": {
                "labels": {
                    "patternProperties": {
                        "^(?!x-)[a-z]+$": {"type": "string"},
                        "^[a-z]+$": {"type": "string"}
                    }
                }
            },
            "$defs": {"env": {"patternProperties": {"^[A-Z_]+$": {}}}}
        });
        let warnings = unsupported_patterns(&schema);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(
            warnings[0].starts_with(
                "/properties/labels/patternProperties: the pattern \"^(?!x-)[a-z]+$\""
            ),
            "{warnings:?}"
        );
        // Compiled when the schema is built, then only looked up.
        assert!(pattern_matches("^[a-z]+$", "abc"));
        assert!(!pattern_matches("^(?!x-)[a-z]+$", "abc"));
    }

    /// The light engine reads no Unicode classes; the full one does.
    #[test]
    fn unicode_classes_follow_the_engine() {
        let schema = serde_json::json!({"patternProperties": {"^\\p{L}+$": {}}});
        let warnings = unsupported_patterns(&schema);
        assert_eq!(
            warnings.is_empty(),
            cfg!(feature = "schema-validation"),
            "{warnings:?}"
        );
    }
}
