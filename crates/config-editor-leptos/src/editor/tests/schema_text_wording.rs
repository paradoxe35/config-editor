//! A schema text JSON or YAML reads otherwise than meant, or one the
//! rules for schemas refuse, says what is wrong in plain words and how to
//! write it (quote a value YAML reads as a list, map or escape), each
//! message starting lower case after "Invalid JSON Schema:"; and a value
//! a one-keyword `not` refuses is told in words, not as the `not`'s JSON.

use crate::editor::Language;
use crate::editor::diagnostics::{Schema, build_schema};

#[test]
fn schema_text_problems_read_plainly() {
    let cases = [
        (
            "[1,2",
            "the text ends before the list is closed: a ] is missing (line 1, column 4)",
        ),
        (
            "{\"type\": \"object\"",
            "the text ends before the map is closed: a } is missing (line 1, column 17)",
        ),
        (
            "{\"type\": \"obj",
            "the text ends inside a string: a closing \" is missing (line 1, column 13)",
        ),
        (
            "{\"type\": ",
            "the text ends where a value should follow (line 1, column 9)",
        ),
        (
            "{\"type\": \"object\"}}",
            "more text follows the end of the schema (line 1, column 19)",
        ),
        (
            "type: object\nproperties:\n  a:\n    type: string\n   maxLength: 3\n",
            "the indentation does not line up: indent each key of a map as far as its other keys (line 5, column 4)",
        ),
        (
            "type: object\nproperties:\n  @a: {type: string}\n",
            "a plain key or value cannot start with @ or ` in YAML: quote it, as in \"@name\" (line 3, column 3)",
        ),
        (
            "type: object\nproperties:\n  a:\n    type: string\n    pattern: [a-z]+\n",
            "a value starting with [ or { is a list or map in YAML, and the text after its closing bracket does not read: quote the value, as in pattern: '[a-z]+' (line 5, column 19)",
        ),
        (
            "type: object\nproperties:\n  a:\n    description: note: this\n    type: string\n",
            "a colon and a space inside a plain value start a map in YAML: quote the value, as in description: \"note: this\" (line 4, column 22)",
        ),
        (
            "type: object\nproperties:\n  a:\n    type: string\n    pattern: \"\\d+\"\n",
            "unknown escape '\\d' in a double-quoted string: in double quotes a backslash starts an escape, so write the text in single quotes, as in pattern: '^\\d+$', or double each backslash (line 5, column 16)",
        ),
        (
            "type: object\nproperties:\n  a:\n    type: string\n    pattern: {3}\n",
            "pattern must be text, and reads as {\"3\":null}: in YAML a value starting with { or [ is a map or list, so quote it, as in pattern: '…' (at /properties/a/pattern)",
        ),
        (
            "type: objekt\n",
            "\"objekt\" is no type JSON Schema knows: use \"array\", \"boolean\", \"integer\", \"null\", \"number\", \"object\" or \"string\" (at /type)",
        ),
        (
            "type: object\n---\ntype: string\n",
            "this text holds more than one YAML document (separated by ---): the editor reads one document per file. Keep one, or split them into files (line 2, column 1)",
        ),
    ];
    let mut failures = Vec::new();
    for (text, wanted) in cases {
        let said = build_schema(text).err().unwrap_or_default();
        if said != wanted {
            failures.push(format!("{text:?}\n  said   {said:?}\n  wanted {wanted:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_one_keyword_not_says_what_it_refuses() {
    let cases = [
        (
            r#"{"properties":{"a":{"not":{"type":"string"}}}}"#,
            "a: x\n",
            r#"/a: "x" must not be of type "string""#,
        ),
        (
            r#"{"properties":{"a":{"not":{"type":["string","integer"]}}}}"#,
            "a: x\n",
            r#"/a: "x" must not be of type "string", "integer""#,
        ),
        (
            r#"{"properties":{"a":{"not":{"enum":["x","y"]}}}}"#,
            "a: x\n",
            r#"/a: "x" must not be "x" or "y""#,
        ),
        (
            r#"{"properties":{"a":{"not":{"const":"x"}}}}"#,
            "a: x\n",
            r#"/a: "x" must not be "x""#,
        ),
        (
            r#"{"properties":{"a":{},"b":{},"c":{}},
                "not":{"anyOf":[{"required":["a"]},{"required":["b"]}]}}"#,
            "a: x\nc: z\n",
            r#"{"a":"x","c":"z"} must not have "a""#,
        ),
    ];
    let mut failures = Vec::new();
    for (schema, document, wanted) in cases {
        let found = Schema::compile(schema)
            .unwrap()
            .validate(Language::YAML, document);
        let said: Vec<&str> = found
            .iter()
            .map(|problem| problem.message.as_str())
            .collect();
        if said != [wanted] {
            failures.push(format!("{schema}\n  said   {said:?}\n  wanted {wanted:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
