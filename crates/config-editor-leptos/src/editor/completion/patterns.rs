//! `patternProperties` regexes, compiled once: when a schema is built, not
//! on every key lookup, and checked so one the engine cannot read is
//! reported instead of silently matching nothing.

use crate::editor::pattern_engine::Regex;
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
/// compile matches nothing; the schema's warnings name it.
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

/// Whether the engine can compile `pattern`, compiling it into the cache
/// (each of a schema's patterns is compiled with the schema, see
/// `schema::compile`).
pub(crate) fn compiles(pattern: &str) -> bool {
    pattern_matches(pattern, "");
    COMPILED.with(|cache| cache.borrow().get(pattern).is_some_and(Option::is_some))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::schema::build_schema;

    fn warnings(schema: &serde_json::Value) -> Vec<String> {
        build_schema(&schema.to_string())
            .map(|bundle| bundle.warnings)
            .unwrap_or_default()
    }

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
        let warnings = warnings(&schema);
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
        let warnings = warnings(&schema);
        assert_eq!(
            warnings.is_empty(),
            cfg!(feature = "schema-validation"),
            "{warnings:?}"
        );
    }

    /// A pattern deep in the schema is warned about as one near its root.
    #[test]
    fn an_unsupported_pattern_deep_in_the_schema_is_warned_about() {
        let mut schema = serde_json::json!({"patternProperties": {"^(?!x-)": {}}});
        for _ in 0..50 {
            schema = serde_json::json!({"properties": {"a": schema}});
        }
        let warnings = warnings(&schema);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }
}
