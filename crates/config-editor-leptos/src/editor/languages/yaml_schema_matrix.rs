//! A schema written in YAML gives every document language what the same
//! schema written in JSON gives: the same problems, suggestions, hover docs
//! and schema warnings.

use super::Language;
use super::harness::{At, Written, completing, hovering, key_place, report, text, write};
use crate::{Schema, TextPosition};
use serde_json::json;

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

/// In `language`: a document with a problem (`listen` below its minimum,
/// `name` missing), a text with the caret (`|`) where a new key is typed,
/// and a position on the key `listen` in the document.
fn case(language: Language) -> Result<(String, Written, TextPosition), String> {
    let document = json!({"listen": 0});
    let written = text(language, &document, &[])?;
    let (line, column) = key_place(language, &written, "/listen", 1)?;
    let at = At::NewKey {
        object: "",
        typed: "",
    };
    let typing = write(language, &json!({}), &[], at)?;
    Ok((written, typing, TextPosition::new(line + 1, column + 1)))
}

#[test]
fn a_yaml_schema_helps_every_language_as_its_json_does() {
    let from_json = Schema::compile(JSON_SCHEMA).unwrap();
    let from_yaml = Schema::compile(YAML_SCHEMA).unwrap_or_else(|error| panic!("{error}"));
    assert_eq!(from_yaml.warnings(), from_json.warnings());
    assert_eq!(from_yaml.warnings().len(), 1, "{:?}", from_yaml.warnings());
    let mut failures = Vec::new();
    for language in completing()
        .into_iter()
        .filter(|language| hovering().contains(language))
    {
        let (document, typing, on_listen) = match case(language) {
            Ok(case) => case,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let document = document.as_str();
        let caret = typing.cursor();
        let typing = typing.text;
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
    report(&failures);
}
