//! A line broken while it is typed (a value or key left unfinished, a
//! syntax error of the language) directly above the caret: the keys the
//! object has written, that line's own included, are not offered again,
//! with nothing after the caret and with entries below it, in every
//! language.

use crate::editor::languages::Language;
use crate::editor::schema::Schema;

const SCHEMA: &str = r#"{"type": "object", "properties": {
    "name": {"type": "string"}, "port": {"type": "integer"},
    "host": {"type": "string"}, "debug": {"type": "boolean"},
    "server": {"type": "object", "properties": {
        "name": {"type": "string"}, "port": {"type": "integer"},
        "host": {"type": "string"}, "debug": {"type": "boolean"}}}
}}"#;

/// Per case: the language, the document with the caret at `|` (on its own
/// line right below the broken one), what an entry below the caret adds,
/// and the keys offered then (with nothing after, `host` too). `BELOW` marks where that entry goes; the
/// document's closers follow it.
const CASES: &[(Language, &str, &str, &[&str])] = &[
    (
        Language::YAML,
        "name: app\nport: 80: x\n|BELOW",
        "\nhost: h",
        &["debug", "server"],
    ),
    (
        Language::YAML,
        "name: app\npor\n|BELOW",
        "\nhost: h",
        &["debug", "port", "server"],
    ),
    (
        Language::YAML,
        "server:\n  name: app\n  port: 80: x\n  |BELOW",
        "\n  host: h",
        &["debug"],
    ),
    (
        Language::JSON,
        "{\n  \"name\": \"app\",\n  \"port\": \"80\n  \"|BELOW\n}\n",
        ",\n  \"host\": \"h\"",
        &["debug", "server"],
    ),
    (
        Language::JSON,
        "{\n  \"server\": {\n    \"name\": \"app\",\n    \"port\": \"80\n    \"|BELOW\n  }\n}\n",
        ",\n    \"host\": \"h\"",
        &["debug"],
    ),
    (
        Language::TOML,
        "name = \"app\"\nport = \n|BELOW",
        "\nhost = \"h\"",
        &["debug", "server"],
    ),
    (
        Language::TOML,
        "name = \"app\"\nport = \"80\n|BELOW",
        "\nhost = \"h\"",
        &["debug", "server"],
    ),
    (
        Language::TOML,
        "name = \"app\"\npor\n|BELOW",
        "\nhost = \"h\"",
        &["debug", "port", "server"],
    ),
    (
        Language::TOML,
        "[server]\nname = \"app\"\nport = \n|BELOW",
        "\nhost = \"h\"",
        &["debug"],
    ),
    (
        Language::HCL,
        "name = \"app\"\nport = \"80\n|BELOW",
        "\nhost = \"h\"",
        &["debug", "server"],
    ),
    (
        Language::HCL,
        "server {\n  name = \"app\"\n  port = \"80\n  |BELOW\n}\n",
        "\n  host = \"h\"",
        &["debug"],
    ),
    (
        Language::XML,
        "<name>app</name>\n<port>8&0</port>\n<|BELOW",
        "\n<host>h</host>",
        &["debug", "server"],
    ),
    (
        Language::XML,
        "<name>app</name>\n<port>80</prt>\n<|BELOW",
        "\n<host>h</host>",
        &["debug", "server"],
    ),
    (
        Language::XML,
        "<server>\n  <name>app</name>\n  <port>8&0</port>\n  <|BELOW\n</server>\n",
        "\n  <host>h</host>",
        &["debug"],
    ),
];

#[test]
fn a_broken_line_above_the_caret_keeps_the_keys_written() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (language, template, below, expected) in CASES {
        for (shape, below) in [("nothing after", ""), ("an entry below", *below)] {
            let marked = template.replace("BELOW", below);
            let cursor = marked.find('|').unwrap();
            let text = marked.replace('|', "");
            let cursor = text[..cursor].encode_utf16().count();
            let mut labels: Vec<String> = schema
                .suggestions(*language, &text, cursor)
                .iter()
                .map(|suggestion| suggestion.label().to_string())
                .collect();
            labels.sort();
            let mut expected = expected.to_vec();
            if below.is_empty() {
                expected.push("host");
                expected.sort_unstable();
            }
            if labels != expected {
                failures.push(format!(
                    "{language} with {shape}: {marked:?} offers {labels:?}, not {expected:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
