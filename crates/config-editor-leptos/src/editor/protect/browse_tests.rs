//! Suggestions inside a read-only block (`Ctrl+Space` on a new line in it):
//! its keys are listed, each with the reason as its detail, in every
//! language, as when browsing the block's own text.

use super::super::completion::completions_in;
use super::super::diagnostics::build_schema;
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

#[test]
fn keys_of_a_read_only_block_are_listed_with_the_reason_in_every_language() {
    let bundle = build_schema(SCHEMA).unwrap();
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
                host_paths: &[],
                protected: &protected,
                prefixes: &[],
                strings: &crate::EditorStrings::default(),
            },
        );
        let c = listed.iter().find(|option| option.label.contains('c'));
        if c.is_none_or(|option| option.detail != "Set by the server") {
            let labels: Vec<_> = listed
                .iter()
                .map(|option| (&option.label, &option.detail))
                .collect();
            failures.push(format!("{language}: {labels:?} in {text:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
