//! A schema written in YAML gives every document language what the same
//! schema written in JSON gives: the same problems, suggestions, hover docs
//! and schema warnings.

use super::Language;
use crate::{Schema, TextPosition};

const JSON_SCHEMA: &str = r##"{
  "type": "object",
  "$defs": {"port": {"type": "integer", "minimum": 1, "description": "A TCP port."}},
  "properties": {
    "name": {"type": "string", "description": "The service name."},
    "listen": {"$ref": "#/$defs/port"},
    "mode": {"enum": ["on", "off", "007"]}
  },
  "patternProperties": {"^(?!x-)": {}},
  "required": ["name"]
}"##;

const YAML_SCHEMA: &str = "\
# The same schema, as YAML.
type: object
$defs:
  port: {type: integer, minimum: 1, description: A TCP port.}
properties:
  name:
    type: string
    description: The service name.
  listen:
    $ref: '#/$defs/port'
  mode:
    enum: [on, off, '007']
patternProperties:
  ^(?!x-): {}
required: [name]
";

/// Per language: a document with a problem (`listen` below its minimum,
/// `name` missing), a text with the caret (`|`) where a new key is typed,
/// and a position on the key `listen` in the document.
fn cases() -> [(Language, &'static str, &'static str, TextPosition); 5] {
    [
        (
            Language::YAML,
            "listen: 0\n",
            "|\n",
            TextPosition::new(1, 2),
        ),
        (
            Language::JSON,
            "{\n  \"listen\": 0\n}\n",
            "{\n  |\n}\n",
            TextPosition::new(2, 4),
        ),
        (
            Language::TOML,
            "listen = 0\n",
            "|\n",
            TextPosition::new(1, 2),
        ),
        (
            Language::HCL,
            "listen = 0\n",
            "|\n",
            TextPosition::new(1, 2),
        ),
        (
            Language::XML,
            "<listen>0</listen>\n",
            "<|\n",
            TextPosition::new(1, 3),
        ),
    ]
}

#[test]
fn a_yaml_schema_helps_every_language_as_its_json_does() {
    let from_json = Schema::compile(JSON_SCHEMA).unwrap();
    let from_yaml = Schema::compile(YAML_SCHEMA).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(from_yaml.warnings(), from_json.warnings());
    assert_eq!(from_yaml.warnings().len(), 1, "{:?}", from_yaml.warnings());
    let mut failures = Vec::new();
    for (language, document, typing, on_listen) in cases() {
        let caret = typing.find('|').expect("a | caret marker");
        let typing = typing.replacen('|', "", 1);
        let problems = from_json.validate(language, document);
        // Without the validator no document has schema problems; the two
        // schemas must still agree.
        let expected = cfg!(feature = "schema-validation");
        if from_yaml.validate(language, document) != problems || problems.is_empty() == expected {
            failures.push(format!("{language:?} problems: {problems:?}"));
        }
        let offered = from_json.suggestions(language, &typing, caret);
        if from_yaml.suggestions(language, &typing, caret) != offered || offered.is_empty() {
            let labels: Vec<&str> = offered.iter().map(|option| option.label()).collect();
            failures.push(format!("{language:?} suggestions: {labels:?}"));
        }
        let hover = from_json.hover(language, document, on_listen);
        if from_yaml.hover(language, document, on_listen) != hover
            || !format!("{hover:?}").contains("A TCP port.")
        {
            failures.push(format!("{language:?} hover: {hover:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
