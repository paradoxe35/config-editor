//! The values a sibling decides (an `if` on the object's `kind`, the
//! `oneOf` option its `kind` picks) narrow the suggestions in every
//! language and form: beside an XML attribute as beside an element, and
//! with a syntax error above the caret.

use crate::editor::languages::Language;
use crate::editor::schema::Schema;
use crate::editor::work_count::{Work, counted};

const SCHEMA: &str = r#"{"type": "object", "properties": {
    "svc": {"type": "object", "properties": {"kind": {"enum": ["a", "b"]}},
        "if": {"properties": {"kind": {"const": "a"}}, "required": ["kind"]},
        "then": {"properties": {"mode": {"enum": ["a1", "a2"]}, "onlyA": {"type": "string"}}},
        "else": {"properties": {"mode": {"enum": ["b1", "b2"]}, "onlyB": {"type": "string"}}}},
    "any": {"oneOf": [
        {"type": "object", "required": ["kind"], "properties": {
            "kind": {"const": "a"},
            "sub": {"type": "object", "properties": {"sa": {"type": "string"}}}}},
        {"type": "object", "required": ["kind"], "properties": {
            "kind": {"const": "b"},
            "sub": {"type": "object", "properties": {"sb": {"type": "string"}}}}}
    ]}
}}"#;

fn labels(schema: &Schema, language: Language, marked: &str) -> Vec<String> {
    let at = marked.find('|').unwrap();
    let text = marked.replacen('|', "", 1);
    let cursor = text[..at].encode_utf16().count();
    let mut labels: Vec<String> = schema
        .suggestions(language, &text, cursor)
        .iter()
        .map(|suggestion| suggestion.label().to_string())
        .collect();
    labels.sort();
    labels
}

fn check(cases: &[(Language, &str, &[&str])]) {
    let schema = Schema::compile(SCHEMA).unwrap();
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(language, marked, wanted)| {
            let found = labels(&schema, *language, marked);
            (found != *wanted).then(|| format!("{language} {marked:?}: {found:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_sibling_narrows_values_and_keys_in_every_form() {
    check(&[
        (
            Language::YAML,
            "svc:\n  kind: a\n  mode: |\n",
            &["a1", "a2"],
        ),
        (
            Language::JSON,
            "{\"svc\": {\"kind\": \"a\", \"mode\": \"|\"}}",
            &["a1", "a2"],
        ),
        (
            Language::TOML,
            "[svc]\nkind = \"a\"\nmode = \"|\"\n",
            &["a1", "a2"],
        ),
        (
            Language::HCL,
            "svc {\n  kind = \"a\"\n  mode = \"|\"\n}\n",
            &["a1", "a2"],
        ),
        (
            Language::XML,
            "<svc><kind>a</kind><mode>|</mode></svc>\n",
            &["a1", "a2"],
        ),
        (
            Language::XML,
            "<svc kind=\"a\"><mode>|</mode></svc>\n",
            &["a1", "a2"],
        ),
        // An attribute's values and names, beside another attribute.
        (
            Language::XML,
            "<svc kind=\"a\" mode=\"|\"/>\n",
            &["a1", "a2"],
        ),
        (
            Language::XML,
            "<svc kind=\"b\" mode=\"b|\"></svc>\n",
            &["b1", "b2"],
        ),
        (
            Language::XML,
            "<svc mode=\"|\" kind=\"b\"/>\n",
            &["b1", "b2"],
        ),
        (Language::XML, "<svc kind=\"a\" |/>\n", &["mode", "onlyA"]),
        (Language::XML, "<svc kind=\"a\" mo|/>\n", &["mode"]),
    ]);
}

#[test]
fn a_syntax_error_above_the_caret_leaves_the_narrowing() {
    check(&[
        (
            Language::YAML,
            "x: \"unclosed\nany:\n  kind: b\n  sub:\n    |\n",
            &["sb"],
        ),
        (
            Language::YAML,
            "x: 'unclosed\nany:\n  kind: b\n  sub:\n    |\n",
            &["sb"],
        ),
        (
            Language::JSON,
            "{\n  \"x\": tru,\n  \"any\": {\n    \"kind\": \"b\",\n    \"sub\": {\"|\"}\n  }\n}\n",
            &["sb"],
        ),
        (
            Language::TOML,
            "x = \n[any]\nkind = \"b\"\n[any.sub]\n|\n",
            &["sb"],
        ),
        (
            Language::HCL,
            "x = }\nany {\n  kind = \"b\"\n  sub {\n    |\n  }\n}\n",
            &["sb"],
        ),
        (
            Language::XML,
            "<x a=></x>\n<any>\n  <kind>b</kind>\n  <sub>\n    <|\n  </sub>\n</any>\n",
            &["sb"],
        ),
    ]);
}

/// Per language: a syntax error, the filler entry (`N` its number)
/// repeated after it (`FILL`), then `any` of kind `b` with the caret in
/// its `sub`.
const ERROR_ABOVE: [(Language, &str, &str); 5] = [
    (
        Language::YAML,
        "fN: x\n",
        "x: \"unclosed\nFILLany:\n  kind: b\n  sub:\n    |\n",
    ),
    (
        Language::JSON,
        "  \"fN\": \"x\",\n",
        "{\n  \"x\": tru,\nFILL  \"any\": {\n    \"kind\": \"b\",\n    \"sub\": {\"|\"}\n  }\n}\n",
    ),
    (
        Language::TOML,
        "fN = \"x\"\n",
        "x = \nFILL[any]\nkind = \"b\"\n[any.sub]\n|\n",
    ),
    (
        Language::HCL,
        "fN = \"x\"\n",
        "x = }\nFILLany {\n  kind = \"b\"\n  sub {\n    |\n  }\n}\n",
    ),
    (
        Language::XML,
        "<fN>x</fN>\n",
        "<x a=></x>\nFILL<any>\n  <kind>b</kind>\n  <sub>\n    <|\n  </sub>\n</any>\n",
    ),
];

/// Leaving out the line of an error above reads the text a few times
/// more, never once per line: counted at two document sizes.
#[test]
fn reading_past_an_error_above_costs_a_few_reads_of_the_text() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, filler, template) in ERROR_ABOVE {
        let mut reads = Vec::new();
        for lines in [100, 1000] {
            let filled: String = (0..lines)
                .map(|line| filler.replace('N', &line.to_string()))
                .collect();
            let marked = template.replace("FILL", &filled);
            let (found, parsed) = counted(Work::ParsedBytes, || labels(&schema, language, &marked));
            if found != ["sb"] {
                failures.push(format!("{language} at {lines} lines offers {found:?}"));
            }
            reads.push(parsed as f64 / marked.len() as f64);
        }
        if reads.iter().any(|read| *read > 10.0) || reads[1] > reads[0] * 1.1 {
            failures.push(format!("{language} parses the text {reads:?} times"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
