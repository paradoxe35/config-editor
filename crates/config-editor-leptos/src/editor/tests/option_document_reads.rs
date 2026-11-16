//! A schema with `oneOf` options reads the document around the caret at
//! most once per suggestion run: the read is shared by the option the
//! caret's object is in and the options of the objects around it, and made
//! only where an option is there to decide. Counted (see `work_count`) at
//! two sizes, in every language and both item forms.

use serde_json::json;

use crate::editor::completion::completions_for;
use crate::editor::languages::Language;
use crate::editor::schema::build_schema;
use crate::editor::work_count::{Work, counted};

fn schema() -> crate::editor::schema::SchemaBundle {
    let option = |kind: &str, key: &str| {
        json!({"type": "object", "required": ["kind"], "properties": {
            "kind": {"const": kind},
            "host": {"type": "string"},
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

/// How a language writes the list: one item, the list around items.
type Writing = (&'static str, &'static str, fn(&str) -> String);

fn writing(language: Language, flow: bool) -> Writing {
    match (language.label(), flow) {
        ("JSON", _) => (
            "{\"kind\": \"a\", \"sub\": {\"sa\": \"x\"}}",
            ",\n  ",
            |items| format!("{{\"items\": [\n  {items}\n]}}\n"),
        ),
        ("YAML", false) => ("- kind: a\n  sub:\n    sa: x", "\n", |items| {
            format!("items:\n{items}\n")
        }),
        ("YAML", true) => ("- {kind: a, sub: {sa: x}}", "\n", |items| {
            format!("items:\n{items}\n")
        }),
        ("TOML", _) => (
            "[[items]]\nkind = \"a\"\nsub = { sa = \"x\" }",
            "\n",
            |items| format!("{items}\n"),
        ),
        ("HCL", _) => ("{ kind = \"a\", sub = { sa = \"x\" } }", ",\n  ", |items| {
            format!("items = [\n  {items},\n]\n")
        }),
        _ => (
            "<items><kind>a</kind><sub><sa>x</sa></sub></items>",
            "\n",
            |items| format!("{items}\n"),
        ),
    }
}

/// The caret's item (kind `b`): a key typed in the item itself, or in its
/// `sub` (`nested`).
fn caret_item(language: Language, flow: bool, nested: bool) -> &'static str {
    match (language.label(), flow, nested) {
        ("JSON", _, false) => "{\"kind\": \"b\", \"|\"}",
        ("JSON", _, true) => "{\"kind\": \"b\", \"sub\": {|}}",
        ("YAML", false, false) => "- kind: b\n  |",
        ("YAML", false, true) => "- kind: b\n  sub:\n    |",
        ("YAML", true, false) => "- {kind: b, |}",
        ("YAML", true, true) => "- {kind: b, sub: {|}}",
        ("TOML", _, false) => "[[items]]\nkind = \"b\"\n|",
        ("TOML", _, true) => "[[items]]\nkind = \"b\"\nsub = { | }",
        ("HCL", _, false) => "{ kind = \"b\", | }",
        ("HCL", _, true) => "{ kind = \"b\", sub = { | } }",
        (_, _, false) => "<items><kind>b</kind><|</items>",
        (_, _, true) => "<items><kind>b</kind><sub><|</sub></items>",
    }
}

/// The reads of the document around the caret for one suggestion run,
/// `items` items before and after the caret's.
fn reads(language: Language, flow: bool, nested: bool, items: usize) -> usize {
    let (item, join, frame) = writing(language, flow);
    let mut all = vec![item.to_string(); 2 * items];
    all.insert(items, caret_item(language, flow, nested).to_string());
    let marked = frame(&all.join(join));
    let at = marked.find('|').unwrap();
    let text = marked.replacen('|', "", 1);
    let caret = text[..at].encode_utf16().count();
    let schema = schema();
    let (options, reads) = counted(Work::AroundParse, || {
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
    let expected = if nested { "sb" } else { "host" };
    assert!(
        labels.iter().any(|label| label == expected),
        "{language} flow={flow} nested={nested}: {labels:?} in {text}"
    );
    if nested {
        assert!(
            !labels.iter().any(|label| label == "sa"),
            "{language}: {labels:?}"
        );
    }
    reads
}

#[test]
fn options_read_the_document_around_the_caret_once_per_suggestion_run() {
    for (language, flow) in [
        (Language::YAML, false),
        (Language::YAML, true),
        (Language::JSON, false),
        (Language::TOML, false),
        (Language::HCL, false),
        (Language::XML, false),
    ] {
        for nested in [false, true] {
            for items in [5, 300] {
                let reads = reads(language, flow, nested, items);
                assert!(
                    reads <= 1,
                    "{language} flow={flow} nested={nested} items={items}: {reads} reads"
                );
            }
        }
    }
}
