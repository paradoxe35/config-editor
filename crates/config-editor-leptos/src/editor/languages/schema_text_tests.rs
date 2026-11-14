//! A schema's text read as JSON, or as YAML when it is no JSON and does
//! not start as JSON does (`read_schema_text`).

use serde_json::json;

use super::read_schema_text;
use crate::editor::diagnostics::build_schema;

const TOO_DEEP: &str =
    "it nests objects and lists more than 128 levels deep, deeper than the editor reads";

#[test]
fn json_text_keeps_its_json_reading() {
    // Read as YAML 1.1, `"y"` and `"on"` would be booleans; read as JSON
    // they are the keys they are written as.
    let text = r#"{"properties": {"y": {"const": "on"}, "n": {"const": 1e3}}}"#;
    assert_eq!(
        read_schema_text(text),
        Ok(json!({"properties": {"y": {"const": "on"}, "n": {"const": 1000.0}}}))
    );
}

#[test]
fn json_errors_word_a_text_that_starts_as_json() {
    for text in ["{\"type\": }", "  [1, 2", "\n{type: object}", "{"] {
        let error = read_schema_text(text).unwrap_err();
        let json_error = serde_json::from_str::<serde_json::Value>(text)
            .unwrap_err()
            .to_string();
        assert_eq!(error, json_error, "{text:?}");
    }
    assert_eq!(
        read_schema_text("").unwrap_err(),
        "EOF while parsing a value at line 1 column 0"
    );
}

#[test]
fn deep_json_says_how_deep_the_editor_reads() {
    let deep = format!("{}{}", "[".repeat(200), "]".repeat(200));
    assert_eq!(read_schema_text(&deep).unwrap_err(), TOO_DEEP);
    assert_eq!(build_schema(&deep).err().as_deref(), Some(TOO_DEEP));
}

#[cfg(feature = "yaml")]
mod yaml {
    use serde_json::json;

    use super::{TOO_DEEP, read_schema_text};
    use crate::editor::diagnostics::build_schema;

    #[test]
    fn a_yaml_schema_reads_into_the_same_value_as_its_json() {
        let yaml = "type: object\nproperties:\n  port:\n    type: integer\n    maximum: 65535\nrequired: [port]\n";
        let json_text = r#"{"type":"object","properties":{"port":{"type":"integer","maximum":65535}},"required":["port"]}"#;
        assert_eq!(
            read_schema_text(yaml),
            Ok(serde_json::from_str(json_text).unwrap())
        );
    }

    #[test]
    fn yaml_words_that_were_booleans_once_stay_keys() {
        let yaml = "properties:\n  on: {type: string}\n  yes: {}\n  y: {}\n  no: {}\n  null: {}\n  true: {}\n  off: {const: off}\n";
        let value = read_schema_text(yaml).unwrap();
        let keys: Vec<&str> = value["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(keys, ["on", "yes", "y", "no", "null", "true", "off"]);
        assert_eq!(value["properties"]["off"]["const"], json!("off"));
    }

    #[test]
    fn yaml_scalars_read_as_yaml_documents_read_them() {
        // As the editor reads a YAML document: `007` and `1_000` stay text
        // (no octal guess), `1e3` and `0x1F` are numbers, quotes make text.
        let yaml = "enum:\n  - 007\n  - 1e3\n  - 0x1F\n  - 1_000\n  - '007'\n  - \"1e3\"\n  - plain text\n  - 12:30\n  - ~\n  - true\n";
        assert_eq!(
            read_schema_text(yaml),
            Ok(json!({"enum": [
                "007", 1000.0, 31, "1_000", "007", "1e3", "plain text", "12:30", null, true
            ]}))
        );
    }

    #[test]
    fn a_number_json_cannot_hold_is_refused_where_it_is() {
        for (yaml, name) in [
            ("properties:\n  limit:\n    maximum: .inf\n", ".inf"),
            ("properties:\n  limit:\n    maximum: -.inf\n", "-.inf"),
            ("properties:\n  limit:\n    maximum: .nan\n", ".nan"),
        ] {
            assert_eq!(
                read_schema_text(yaml).unwrap_err(),
                format!(
                    "the value at /properties/limit/maximum is {name}, a number JSON cannot hold: a JSON Schema holds finite numbers only"
                )
            );
        }
        // Quoted, it is the text it spells.
        assert_eq!(
            read_schema_text("const: '.inf'\n"),
            Ok(json!({"const": ".inf"}))
        );
    }

    #[test]
    fn anchors_aliases_and_merge_keys_expand() {
        let yaml = "\
$defs:
  port: &port
    type: integer
    minimum: 1
properties:
  listen: *port
  admin:
    <<: *port
    maximum: 9000
";
        assert_eq!(
            read_schema_text(yaml),
            Ok(json!({
                "$defs": {"port": {"type": "integer", "minimum": 1}},
                "properties": {
                    "listen": {"type": "integer", "minimum": 1},
                    "admin": {"type": "integer", "minimum": 1, "maximum": 9000}
                }
            }))
        );
    }

    #[test]
    fn a_yaml_schema_with_refs_compiles_and_resolves_them() {
        let yaml = "\
$defs:
  port:
    type: integer
    description: A TCP port.
properties:
  listen:
    $ref: '#/$defs/port'
";
        let bundle = build_schema(yaml).unwrap_or_else(|error| panic!("{error}"));
        let schema = crate::Schema::compile(yaml).unwrap();
        assert_eq!(
            bundle.schema["properties"]["listen"]["$ref"],
            "#/$defs/port"
        );
        #[cfg(feature = "schema-validation")]
        assert_eq!(
            schema
                .validate(crate::Language::YAML, "listen: x\n")
                .iter()
                .filter(|problem| problem.message.contains("integer"))
                .count(),
            1
        );
        let hover = schema
            .hover(
                crate::Language::YAML,
                "listen: 1\n",
                crate::TextPosition::new(1, 2),
            )
            .expect("hover on listen");
        assert!(format!("{hover:?}").contains("A TCP port."), "{hover:?}");
    }

    #[test]
    fn a_byte_order_mark_before_yaml_is_no_part_of_it() {
        let bundle = build_schema("\u{feff}properties:\n  name: {type: string}\n")
            .unwrap_or_else(|error| panic!("{error}"));
        assert_eq!(
            bundle.schema,
            json!({"properties": {"name": {"type": "string"}}})
        );
        // And before JSON starting after blank lines.
        assert!(build_schema("\u{feff}\n  {\"type\": \"object\"}").is_ok());
    }

    #[test]
    fn broken_yaml_gets_the_yaml_error() {
        for (text, expected) in [
            (
                "properties:\n  a: b: c\n",
                "YAML parse error at line 2, column 7: mapping values are not allowed in this context",
            ),
            (
                "type: object\ntype: array\n",
                "type: duplicate key \"type\" at line 2, column 1",
            ),
            (
                "# only a comment\n",
                "the YAML text holds no schema, only comments",
            ),
        ] {
            assert_eq!(read_schema_text(text).unwrap_err(), expected, "{text:?}");
        }
    }

    #[test]
    fn deep_yaml_says_how_deep_the_editor_reads() {
        let mut deep = String::new();
        for level in 0..200 {
            deep.push_str(&format!("{}a{level}:\n", "  ".repeat(level)));
        }
        assert_eq!(read_schema_text(&deep).unwrap_err(), TOO_DEEP);
        assert_eq!(build_schema(&deep).err().as_deref(), Some(TOO_DEEP));
    }
}

#[cfg(not(feature = "yaml"))]
#[test]
fn without_yaml_a_yaml_schema_says_it_needs_the_feature() {
    assert_eq!(
        read_schema_text("properties: {}\n").unwrap_err(),
        "it is no JSON (expected value at line 1 column 1), and a schema written in YAML needs the editor's `yaml` feature"
    );
    // JSON still reads.
    assert_eq!(
        read_schema_text("{\"type\": \"object\"}"),
        Ok(json!({"type": "object"}))
    );
}
