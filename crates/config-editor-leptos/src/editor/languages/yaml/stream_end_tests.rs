//! Block scalars at the end of the stream, against the YAML 1.2 spec
//! (8.1.1.2 chomping: a final line break is kept only when written).

use serde_json::{Value, json};

use super::super::YamlBehavior;
use crate::editor::language::ConfigLanguage;

fn read(source: &str) -> Value {
    YamlBehavior
        .parse(source)
        .unwrap_or_else(|error| panic!("{source:?}: {error:?}"))
}

#[test]
fn a_block_scalar_ending_the_stream_without_a_line_break_holds_none() {
    let cases = [
        ("a: 1\nk: |\n  echo hi", json!({"a": 1, "k": "echo hi"})),
        ("a: 1\nk: |\n  echo hi\n", json!({"a": 1, "k": "echo hi\n"})),
        ("k: |+\n  echo hi", json!({"k": "echo hi"})),
        ("k: |+\n  echo hi\n", json!({"k": "echo hi\n"})),
        ("k: |-\n  echo hi", json!({"k": "echo hi"})),
        ("k: >\n  echo\n  hi", json!({"k": "echo hi"})),
        ("k: >\n  echo\n  hi\n", json!({"k": "echo hi\n"})),
        ("k: |\n  a\n\n  b", json!({"k": "a\n\nb"})),
        ("k: |\n  a\n  ", json!({"k": "a\n"})),
        ("l:\n  - |\n    x", json!({"l": ["x"]})),
        ("o:\n  i: |2\n     x", json!({"o": {"i": " x"}})),
        ("|\n  x", json!("x")),
        ("--- >\n  x\n  y", json!("x y")),
        ("|\n  x\n# done", json!("x\n")),
        ("k: |\n  x\n# done", json!({"k": "x\n"})),
        ("k: \"a\\n\"", json!({"k": "a\n"})),
        ("k: |\n  x\nm: \"y\\n\"", json!({"k": "x\n", "m": "y\n"})),
        ("k: |\n  x\r\n  y", json!({"k": "x\ny"})),
    ];
    let wrong: Vec<String> = cases
        .iter()
        .filter(|(source, expected)| read(source) != *expected)
        .map(|(source, expected)| format!("{source:?}: {} not {expected}", read(source)))
        .collect();
    assert!(wrong.is_empty(), "{wrong:#?}");
}
