//! Keys typed in a list item written on one line read the document
//! without only the typed entry, keys a branch declares `false` are left
//! out, and a required list's items get their stops: none of it costs
//! more passes in a longer document. Counted (see `work_count`) at two
//! sizes, in every language that writes such items.

use serde_json::json;

use crate::editor::completion::completions_for;
use crate::editor::diagnostics::build_schema;
use crate::editor::languages::Language;
use crate::editor::work_count::{Work, counted_all};

/// Work noted per byte or character read: it grows with the document.
const PER_BYTE: [Work; 5] = [
    Work::OffsetSeek,
    Work::OffsetBytes,
    Work::ParsedBytes,
    Work::RowText,
    Work::CharacterStep,
];

fn schema() -> crate::editor::diagnostics::SchemaBundle {
    let item = json!({
        "type": "object",
        "properties": {"kind": {"enum": ["file", "http"]}, "path": {"type": "string"},
            "url": {"type": "string"},
            "tags": {"type": "array", "minItems": 2, "items": {"type": "string"}},
            "gone": false},
        "if": {"properties": {"kind": {"const": "http"}}, "required": ["kind"]},
        "then": {"required": ["url", "tags"], "properties": {"path": false}},
        "else": {"required": ["path"], "properties": {"url": false}}
    });
    build_schema(
        &json!({"type": "object", "properties": {"outs": {"type": "array", "items": item}}})
            .to_string(),
    )
    .unwrap()
}

/// A one-line item, the caret's, and the list around items.
type Writing = (&'static str, &'static str, fn(&[String]) -> String);

/// `items` one-line items of `language` with the caret's item (`{kind:
/// http, |}`) in the middle; the text and its UTF-16 caret.
fn document(language: Language, items: usize) -> (String, usize) {
    let (item, caret_item, frame): Writing = match language.label() {
        "JSON" => (
            "{\"kind\": \"file\", \"path\": \"p\"}",
            "{\"kind\": \"http\", \"|\"}",
            |items| format!("{{\"outs\": [\n  {}\n]}}\n", items.join(",\n  ")),
        ),
        "YAML" => ("{kind: file, path: p}", "{kind: http, |}", |items| {
            format!("outs:\n  - {}\n", items.join("\n  - "))
        }),
        _ => (
            "{ kind = \"file\", path = \"p\" }",
            "{ kind = \"http\", | }",
            |items| format!("outs = [\n  {},\n]\n", items.join(",\n  ")),
        ),
    };
    let mut all = vec![item.to_string(); items];
    all.insert(items / 2, caret_item.to_string());
    let marked = frame(&all);
    let at = marked.find('|').unwrap();
    let text = marked.replacen('|', "", 1);
    let caret = text[..at].encode_utf16().count();
    (text, caret)
}

fn passes(language: Language, items: usize) -> Vec<usize> {
    let schema = schema();
    let (text, caret) = document(language, items);
    let (options, counts) = counted_all(|| {
        completions_for(
            &text,
            caret,
            true,
            Some(&schema),
            true,
            language.behavior(),
            false,
        )
    });
    let mut labels: Vec<String> = options.into_iter().map(|option| option.label).collect();
    labels.sort();
    assert_eq!(labels, ["tags", "url"], "{language}: {text}");
    counts.to_vec()
}

#[test]
fn a_one_line_item_costs_the_same_passes_in_a_longer_document() {
    for language in [
        Language::JSON,
        Language::YAML,
        Language::TOML,
        Language::HCL,
    ] {
        let small = passes(language, 10);
        let large = passes(language, 400);
        for (index, work) in Work::ALL.iter().enumerate() {
            if PER_BYTE.contains(work) {
                continue;
            }
            assert_eq!(
                small[index], large[index],
                "{language} {work:?}: {small:?} vs {large:?}"
            );
        }
    }
}
