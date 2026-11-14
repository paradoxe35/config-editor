//! A schema problem inside the one item of a list (an HCL block written
//! once, a TOML `[[table]]` written once, a lone XML element, a one-item
//! YAML or JSON list), inside a block nested in it, inside a labelled
//! block, and inside an object written where a list is expected, is placed
//! on the line that writes it, for every kind of problem and language.

use crate::editor::diagnostics::Schema;
use crate::editor::languages::Language;

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "name": {"type": "string"},
    "server": {
      "type": "object",
      "properties": {
        "listen": {
          "type": "array",
          "items": {
            "type": "object",
            "required": ["port"],
            "additionalProperties": false,
            "properties": {
              "port": {"type": "integer"},
              "host": {"type": "string"},
              "tls": {
                "type": "object",
                "required": ["on"],
                "additionalProperties": false,
                "properties": {"on": {"type": "boolean"}, "ca": {"type": "string"}}
              }
            }
          }
        }
      }
    }
  }
}"#;

/// Each case: label, language, text; the line of each of its problems.
fn cases() -> Vec<(&'static str, Language, &'static str, &'static [usize])> {
    vec![
        // HCL: one block.
        (
            "hcl block, refused",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen {\n    port = 80\n    hots = \"x\"\n  }\n}\n",
            &[5],
        ),
        (
            "hcl block, type",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen {\n    port = \"x\"\n  }\n}\n",
            &[4],
        ),
        (
            "hcl block, required",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen {\n    host = \"x\"\n  }\n}\n",
            &[3],
        ),
        // HCL: a block nested in the one block.
        (
            "hcl nested, refused",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen {\n    port = 80\n    tls {\n      on = true\n      onn = true\n    }\n  }\n}\n",
            &[7],
        ),
        (
            "hcl nested, type",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen {\n    port = 80\n    tls {\n      on = 1\n    }\n  }\n}\n",
            &[6],
        ),
        (
            "hcl nested, required",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen {\n    port = 80\n    tls {\n      ca = \"c\"\n    }\n  }\n}\n",
            &[5],
        ),
        // HCL: one labelled block, read as an item holding its label: the
        // label is refused and the item misses its required key.
        (
            "hcl labelled",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen \"a\" {\n    port = 80\n  }\n}\n",
            &[3, 3],
        ),
        // HCL: an object written where the list is expected.
        (
            "hcl object, refused",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen = {\n    port = 80\n    hots = \"x\"\n  }\n}\n",
            &[5],
        ),
        (
            "hcl object, type",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen = {\n    port = \"x\"\n  }\n}\n",
            &[4],
        ),
        (
            "hcl one-line object, refused",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen = { port = 80, hots = \"x\" }\n}\n",
            &[3],
        ),
        (
            "hcl object in a list, refused",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen = [\n    { port = 80 },\n    { hots = \"x\", port = 1 },\n  ]\n}\n",
            &[5],
        ),
        // HCL: two blocks, the problem in the second.
        (
            "hcl blocks, refused",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen {\n    port = 80\n  }\n  listen {\n    port = 81\n    hots = \"x\"\n  }\n}\n",
            &[8],
        ),
        (
            "hcl blocks, type",
            Language::HCL,
            "name = \"app\"\nserver {\n  listen {\n    port = 80\n  }\n  listen {\n    port = \"x\"\n  }\n}\n",
            &[7],
        ),
        // TOML: one `[[table]]`, nested table, two.
        (
            "toml table, refused",
            Language::TOML,
            "name = \"app\"\n\n[[server.listen]]\nport = 80\nhots = \"x\"\n",
            &[5],
        ),
        (
            "toml table, type",
            Language::TOML,
            "name = \"app\"\n\n[[server.listen]]\nport = \"x\"\n",
            &[4],
        ),
        (
            "toml table, required",
            Language::TOML,
            "name = \"app\"\n\n[[server.listen]]\nhost = \"x\"\n",
            &[3],
        ),
        (
            "toml nested, refused",
            Language::TOML,
            "name = \"app\"\n\n[[server.listen]]\nport = 80\n\n[server.listen.tls]\non = true\nonn = true\n",
            &[8],
        ),
        (
            "toml nested, type",
            Language::TOML,
            "name = \"app\"\n\n[[server.listen]]\nport = 80\n\n[server.listen.tls]\non = 1\n",
            &[7],
        ),
        (
            "toml tables, refused",
            Language::TOML,
            "[[server.listen]]\nport = 80\n\n[[server.listen]]\nport = 81\nhots = \"x\"\n",
            &[6],
        ),
        // XML: one element, nested, two.
        (
            "xml element, refused",
            Language::XML,
            "<server>\n  <listen>\n    <port>80</port>\n    <hots>x</hots>\n  </listen>\n</server>\n",
            &[4],
        ),
        (
            "xml element, type",
            Language::XML,
            "<server>\n  <listen>\n    <port>x</port>\n  </listen>\n</server>\n",
            &[3],
        ),
        (
            "xml element, required",
            Language::XML,
            "<server>\n  <listen>\n    <host>x</host>\n  </listen>\n</server>\n",
            &[2],
        ),
        (
            "xml nested, refused",
            Language::XML,
            "<server>\n  <listen>\n    <port>80</port>\n    <tls>\n      <on>true</on>\n      <onn>true</onn>\n    </tls>\n  </listen>\n</server>\n",
            &[6],
        ),
        (
            "xml nested, type",
            Language::XML,
            "<server>\n  <listen>\n    <port>80</port>\n    <tls>\n      <on>1</on>\n    </tls>\n  </listen>\n</server>\n",
            &[5],
        ),
        (
            "xml elements, refused",
            Language::XML,
            "<server>\n  <listen>\n    <port>80</port>\n  </listen>\n  <listen>\n    <port>81</port>\n    <hots>x</hots>\n  </listen>\n</server>\n",
            &[7],
        ),
        // YAML: one item, nested, two.
        (
            "yaml item, refused",
            Language::YAML,
            "name: app\nserver:\n  listen:\n    - port: 80\n      hots: x\n",
            &[5],
        ),
        (
            "yaml item, type",
            Language::YAML,
            "name: app\nserver:\n  listen:\n    - port: x\n",
            &[4],
        ),
        (
            "yaml nested, refused",
            Language::YAML,
            "name: app\nserver:\n  listen:\n    - port: 80\n      tls:\n        on: true\n        onn: true\n",
            &[7],
        ),
        (
            "yaml nested, required",
            Language::YAML,
            "name: app\nserver:\n  listen:\n    - port: 80\n      tls:\n        ca: c\n",
            &[5],
        ),
        (
            "yaml items, refused",
            Language::YAML,
            "server:\n  listen:\n    - port: 80\n    - port: 81\n      hots: x\n",
            &[5],
        ),
        // JSON: one item, nested.
        (
            "json item, refused",
            Language::JSON,
            "{\n  \"server\": {\n    \"listen\": [\n      {\n        \"port\": 80,\n        \"hots\": \"x\"\n      }\n    ]\n  }\n}\n",
            &[6],
        ),
        (
            "json nested, refused",
            Language::JSON,
            "{\n  \"server\": {\n    \"listen\": [\n      {\n        \"port\": 80,\n        \"tls\": {\n          \"on\": true,\n          \"onn\": true\n        }\n      }\n    ]\n  }\n}\n",
            &[8],
        ),
    ]
}

#[test]
fn problems_inside_a_lone_list_item_are_placed_on_their_line() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (label, language, text, lines) in cases() {
        let found = schema.validate(language, text);
        let placed: Vec<_> = found.iter().filter_map(|problem| problem.line).collect();
        if placed != lines {
            failures.push(format!("{label}: expected lines {lines:?}, got {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
