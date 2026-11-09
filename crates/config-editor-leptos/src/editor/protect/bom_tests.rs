//! Locks in a text with a leading byte order mark: each field is locked
//! over the same text as without it, the first line's included.

use super::super::Language;
use super::super::paths::FieldPath;
use super::locks::{Sources, protect};
use super::tests::documents;

const BOM: char = '\u{FEFF}';

/// What each lock of `text` covers, by pointer.
fn covered(
    language: Language,
    text: &str,
    paths: &[FieldPath],
) -> Vec<(Option<String>, Vec<String>)> {
    let units: Vec<u16> = text.encode_utf16().collect();
    let protected = protect(
        language.behavior(),
        text,
        Sources {
            paths,
            ..Sources::default()
        },
    );
    let mut covered: Vec<_> = protected
        .locks
        .iter()
        .map(|lock| {
            let texts = lock
                .ranges
                .iter()
                .map(|range| String::from_utf16_lossy(&units[range.clone()]))
                .collect();
            (lock.pointer.clone(), texts)
        })
        .collect();
    covered.sort();
    covered
}

#[test]
fn every_field_is_locked_after_a_byte_order_mark() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let document = language.parse(text).unwrap();
        let fields: Vec<String> = document
            .as_object()
            .map(|object| object.keys().map(|key| format!("/{key}")).collect())
            .unwrap_or_default();
        let paths: Vec<FieldPath> = fields
            .iter()
            .map(|field| FieldPath::from(field.as_str()))
            .collect();
        let marked = format!("{BOM}{text}");
        let expected = covered(language, text, &paths);
        let found = covered(language, &marked, &paths);
        if expected.is_empty() || found != expected {
            failures.push(format!("{language}: {found:?}\nexpected {expected:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
