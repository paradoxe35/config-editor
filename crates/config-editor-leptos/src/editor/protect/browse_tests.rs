//! Suggestions inside a read-only block (`Ctrl+Space` on a new line in it):
//! its keys are listed, each with the reason as its detail, in every
//! language, as when browsing the block's own text.

use super::super::completion::completions_in;
use super::super::diagnostics::build_schema;
use super::super::paths::FieldPath;
use super::locks::{Sources, protect};
use super::suggest::{Around, adjust};
use super::tests::documents;

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "meta": {
      "type": "object",
      "readOnly": true,
      "x-readonly-reason": "Set by the server",
      "properties": {"a": {"type": "integer"}, "b": {"type": "integer"}, "c": {"type": "integer"}}
    },
    "port": {"type": "integer"}
  }
}"#;

/// The suggestions listed at a new line inside `meta` in every language,
/// `meta` locked by `schema` and the host's `paths`, the texts `strings`;
/// the failures where `c` does not show `reason` as its detail.
fn listed_with_reason(
    schema: &str,
    paths: &[FieldPath],
    strings: &crate::EditorStrings,
    reason: &str,
) -> Vec<String> {
    let bundle = build_schema(schema).unwrap();
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let (after, typed) = match language.label() {
            "YAML" => ("  b: 2\n", "  "),
            "JSON" => ("\"b\": 2", ",\n    "),
            "TOML" => ("b = 2\n", ""),
            "HCL" => ("  b = 2\n", "  "),
            _ => ("  <b>2</b>\n", "  "),
        };
        let at = text.find(after).unwrap() + after.len();
        let text = format!("{}{typed}{}", &text[..at], &text[at..]);
        let cursor = at + typed.len();
        let behavior = language.behavior();
        let options = completions_in(
            &text,
            cursor,
            true,
            Some(&bundle),
            true,
            behavior,
            false,
            &[],
        );
        let protected = protect(
            behavior,
            &text,
            Sources {
                marking_schema: Some(&bundle.schema),
                reading_schema: Some(&bundle.schema),
                paths,
                ..Sources::default()
            },
        );
        let listed = adjust(
            options,
            &Around {
                behavior,
                source: &text,
                cursor,
                explicit: true,
                schema: Some(&bundle.schema),
                host_paths: paths,
                protected: &protected,
                prefixes: &[],
                strings,
            },
        );
        let c = listed.iter().find(|option| option.label.contains('c'));
        if c.is_none_or(|option| option.detail != reason) {
            let labels: Vec<_> = listed
                .iter()
                .map(|option| (&option.label, &option.detail))
                .collect();
            failures.push(format!("{language}: {labels:?} in {text:?}"));
        }
    }
    failures
}

#[test]
fn keys_of_a_read_only_block_are_listed_with_the_reason_in_every_language() {
    let failures = listed_with_reason(
        SCHEMA,
        &[],
        &crate::EditorStrings::default(),
        "Set by the server",
    );
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Without a reason of its own (a `readOnly` block, or one the host
/// names), the block's keys show the `read_only_field` text, as the host's
/// strings set it.
#[test]
fn keys_of_a_block_locked_without_a_reason_show_the_strings_text() {
    let strings = crate::EditorStrings {
        read_only_field: "Locked by the host".to_string(),
        ..crate::EditorStrings::default()
    };
    let unexplained = SCHEMA.replace(r#""x-readonly-reason": "Set by the server","#, "");
    let mut failures = listed_with_reason(&unexplained, &[], &strings, "Locked by the host");
    let unmarked = unexplained.replace(r#""readOnly": true,"#, "");
    failures.extend(listed_with_reason(
        &unmarked,
        &[FieldPath::from("/meta")],
        &strings,
        "Locked by the host",
    ));
    assert!(failures.is_empty(), "{failures:#?}");
}
