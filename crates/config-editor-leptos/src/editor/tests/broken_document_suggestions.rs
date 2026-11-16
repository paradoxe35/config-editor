//! Suggestions in a document with a syntax error elsewhere read what
//! still parses before the caret to decide the `oneOf` option the keys
//! follow: that read is of the text once or twice, never more, so its cost
//! grows with the document and nothing else. Counted (see `work_count`) at
//! two document sizes, in every language.

use crate::editor::languages::Language;
use crate::editor::schema::Schema;
use crate::editor::work_count::{Work, counted};

const SCHEMA: &str = r#"{"type": "object", "properties": {
    "any": {"oneOf": [
        {"type": "object", "required": ["kind"], "properties": {
            "kind": {"const": "a"},
            "sub": {"type": "object", "properties": {"sa": {"type": "string"}}}}},
        {"type": "object", "required": ["kind"], "properties": {
            "kind": {"const": "b"},
            "sub": {"type": "object", "properties": {"sb": {"type": "string"}}}}}
    ]}
}}"#;

/// Per language: a filler entry (`N` its number), and the document after
/// the filler (`FILL`): `any` of kind `b` with the caret (`|`) in its
/// `sub`, then a syntax error.
const CASES: [(Language, &str, &str); 5] = [
    (
        Language::YAML,
        "fN: x\n",
        "FILLany:\n  kind: b\n  sub:\n    |\nbroken: [\n",
    ),
    (
        Language::JSON,
        "  \"fN\": \"x\",\n",
        "{\nFILL  \"any\": {\n    \"kind\": \"b\",\n    \"sub\": {\n      \"|\"\n    }\n  },\n  \"broken\": \n}\n",
    ),
    (
        Language::TOML,
        "fN = \"x\"\n",
        "FILL[any]\nkind = \"b\"\n[any.sub]\n|\n[broken\n",
    ),
    (
        Language::HCL,
        "fN = \"x\"\n",
        "FILLany {\n  kind = \"b\"\n  sub {\n    |\n  }\n}\nbroken = \n",
    ),
    (
        Language::XML,
        "<fN>x</fN>\n",
        "FILL<any>\n  <kind>b</kind>\n  <sub>\n    <|\n  </sub>\n</any>\n<broken>\n",
    ),
];

#[test]
fn reading_around_a_syntax_error_costs_a_few_reads_of_the_text() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, filler, template) in CASES {
        let mut reads = Vec::new();
        for lines in [100, 1000] {
            let filled: String = (0..lines)
                .map(|line| filler.replace('N', &line.to_string()))
                .collect();
            let marked = template.replace("FILL", &filled);
            let cursor = marked.find('|').unwrap();
            let text = marked.replace('|', "");
            let cursor = text[..cursor].encode_utf16().count();
            let (labels, parsed) = counted(Work::ParsedBytes, || {
                schema
                    .suggestions(language, &text, cursor)
                    .iter()
                    .map(|suggestion| suggestion.label().to_string())
                    .collect::<Vec<_>>()
            });
            if labels != ["sb"] {
                failures.push(format!("{language} at {lines} lines offers {labels:?}"));
            }
            reads.push(parsed as f64 / text.len() as f64);
        }
        if reads.iter().any(|read| *read > 6.0) || reads[1] > reads[0] * 1.1 {
            failures.push(format!("{language} parses the text {reads:?} times"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
