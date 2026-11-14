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
            "a plain key or value cannot start with @ or ` in YAML: quote it, as in \"@a\" (line 3, column 3)",
        ),
        // QA 35: each hint quotes the user's own key and value.
        (
            "type: object\nproperties:\n  a:\n    title: @me\n",
            "a plain key or value cannot start with @ or ` in YAML: quote it, as in \"@me\" (line 4, column 12)",
        ),
        (
            "type: object\nproperties:\n  a:\n    pattern: [a-z]{3}\n",
            "a value starting with [ or { is a list or map in YAML, and the text after its closing bracket does not read: quote the value, as in pattern: '[a-z]{3}' (line 4, column 19)",
        ),
        (
            "type: object\nproperties:\n  a:\n    pattern: \"^\\w+$\"\n",
            "unknown escape '\\w' in a double-quoted string: in double quotes a backslash starts an escape, so write the text in single quotes, as in pattern: '^\\w+$', or double each backslash (line 4, column 17)",
        ),
        (
            "type: object\nproperties:\n  a:\n    description: see: docs\n",
            "a colon and a space inside a plain value start a map in YAML: quote the value, as in description: \"see: docs\" (line 4, column 21)",
        ),
        // QA 35: the YAML reader's own words, a schema rule's words.
        (
            "type: object\nproperties: [a, b\n",
            "a list opened with [ is not closed: a , or ] is missing (line 3, column 1)",
        ),
        (
            "type: object\nproperties:\n  a: {type: string\n",
            "a map opened with { is not closed: a , or } is missing (line 4, column 1)",
        ),
        (
            "type: object\nproperties:\n  \"a: {type: string}\n",
            "a text opened with \" is not closed: a closing \" is missing (line 4, column 1)",
        ),
        (
            "type: object\nproperties: {a: {type: string}}}\n",
            "this } closes nothing opened with {: remove it (line 2, column 32)",
        ),
        (
            "type: object\nproperties:\n  a:\n    pattern: \"(unclosed\"\n",
            "\"(unclosed\" is no regular expression the editor reads: check its brackets and escapes (at /properties/a/pattern)",
        ),
        (
            "type: object\nproperties:\n  a: {type: [string, objekt]}\n",
            "\"objekt\" is no type JSON Schema knows: use \"array\", \"boolean\", \"integer\", \"null\", \"number\", \"object\" or \"string\" (at /properties/a/type)",
        ),
        (
            "type: object\nadditionalProperties: no\n",
            "additionalProperties must be true or false, and \"no\" reads as text in YAML: write false (at /additionalProperties)",
        ),
        (
            "type: object\n...\nfoo: 1\n",
            "this text holds more than one YAML document (a line --- starts one, a line ... ends one): the editor reads one document per file. Keep one, or split them into files",
        ),
        // A JSON schema text with a key written twice is refused, as YAML
        // refuses it.
        (
            "{\"type\": \"object\", \"type\": \"string\"}",
            "duplicate key \"type\" (line 1, column 25)",
        ),
        (
            "{\"properties\": {\"a\": {}, \"a\": {}}}",
            "duplicate key \"a\" (line 1, column 28)",
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
            "unknown escape '\\d' in a double-quoted string: in double quotes a backslash starts an escape, so write the text in single quotes, as in pattern: '\\d+', or double each backslash (line 5, column 16)",
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
            "this text holds more than one YAML document (a line --- starts one, a line ... ends one): the editor reads one document per file. Keep one, or split them into files (line 2, column 1)",
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
            r#""a" must not be set here"#,
        ),
        // QA 35: no `not` is said as JSON: a pattern by its pattern, one
        // every value matches as no value allowed, a `type` beside an
        // `enum` by the values, any other in plain words.
        (
            r#"{"properties":{"a":{"not":{"pattern":"^x"}}}}"#,
            "a: x\n",
            r#"/a: "x" must not match the pattern "^x""#,
        ),
        (
            r#"{"properties":{"a":{"not":{}}}}"#,
            "a: x\n",
            r#"/a: "x" is not allowed"#,
        ),
        (
            r#"{"properties":{"a":{"not":true}}}"#,
            "a: x\n",
            r#"/a: "x" is not allowed"#,
        ),
        (
            r#"{"properties":{"a":{"type":"string","not":{"type":"string","enum":["root","admin"]}}}}"#,
            "a: root\n",
            r#"/a: "root" must not be "root" or "admin""#,
        ),
        (
            r##"{"$defs":{"s":{"type":"string"}},"properties":{"a":{"not":{"$ref":"#/$defs/s"}}}}"##,
            "a: x\n",
            r#"/a: "x" is not allowed here"#,
        ),
        (
            r#"{"properties":{"a":{"not":{"maximum":10}}}}"#,
            "a: 3\n",
            "/a: 3 is not allowed here",
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
