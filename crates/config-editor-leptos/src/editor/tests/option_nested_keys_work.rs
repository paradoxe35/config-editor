//! The keys of an object nested in a `oneOf` option follow the option the
//! item is in, and telling it costs no more passes in a longer document:
//! counted (see `work_count`) at two sizes, in every language.

use serde_json::json;

use crate::editor::completion::completions_for;
use crate::editor::languages::Language;
use crate::editor::schema::build_schema;
use crate::editor::work_count::{Work, counted_all};

/// Work noted per byte or character read: it grows with the document.
const PER_BYTE: [Work; 6] = [
    Work::OffsetSeek,
    Work::OffsetBytes,
    Work::ParsedBytes,
    Work::ScannedBytes,
    Work::RowText,
    Work::CharacterStep,
];

fn schema() -> crate::editor::schema::SchemaBundle {
    let option = |kind: &str, key: &str| {
        json!({"type": "object", "required": ["kind"], "properties": {
            "kind": {"const": kind},
            "sub": {"type": "object", "additionalProperties": false,
                "properties": {key: {"type": "string"}}}}})
    };
    let items = json!({"oneOf": [option("a", "sa"), option("b", "sb")]});
    build_schema(
        &json!({"type": "object", "properties": {"items": {"type": "array", "items": items}}})
            .to_string(),
    )
    .unwrap()
}

/// `items` items of kind `a` with the caret's item (kind `b`, the caret in
/// its `sub`) in the middle; the text and its UTF-16 caret.
fn document(language: Language, items: usize) -> (String, usize) {
    let (item, caret_item, join, frame): (&str, &str, &str, fn(&str) -> String) =
        match language.label() {
            "JSON" => (
                "{\"kind\": \"a\", \"sub\": {\"sa\": \"x\"}}",
                "{\"kind\": \"b\", \"sub\": {|}}",
                ",\n  ",
                |items| format!("{{\"items\": [\n  {items}\n]}}\n"),
            ),
            "YAML" => (
                "- kind: a\n  sub:\n    sa: x",
                "- kind: b\n  sub:\n    |",
                "\n",
                |items| format!("items:\n{items}\n"),
            ),
            "TOML" => (
                "[[items]]\nkind = \"a\"\nsub = { sa = \"x\" }",
                "[[items]]\nkind = \"b\"\nsub = { | }",
                "\n",
                |items| format!("{items}\n"),
            ),
            "HCL" => (
                "{ kind = \"a\", sub = { sa = \"x\" } }",
                "{ kind = \"b\", sub = { | } }",
                ",\n  ",
                |items| format!("items = [\n  {items},\n]\n"),
            ),
            _ => (
                "<items><kind>a</kind><sub><sa>x</sa></sub></items>",
                "<items><kind>b</kind><sub><|</sub></items>",
                "\n",
                |items| format!("{items}\n"),
            ),
        };
    let mut all = vec![item.to_string(); items];
    all.insert(items / 2, caret_item.to_string());
    let marked = frame(&all.join(join));
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
    let labels: Vec<String> = options.into_iter().map(|option| option.label).collect();
    assert_eq!(labels, ["sb"], "{language}: {text}");
    counts.to_vec()
}

#[test]
fn keys_of_an_object_in_an_option_cost_the_same_passes_in_a_longer_document() {
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
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
