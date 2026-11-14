//! A key renamed together with keys inside it (a Replace All of `port`
//! over `ports` and the `port` keys it holds) is renamed where each key is
//! written: the host's forms stay (QA `E32-5`: a TOML inline table so
//! renamed was rewritten as `[tool.x.ps]` table sections after the rest).

use crate::editor::Language;
use crate::editor::save_matrix::{from_every_view, report};
use serde_json::{Map, Value};

/// `value` with every key holding `from` spelled with `to` instead.
fn replaced(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::Object(map) => {
            *map = std::mem::take(map)
                .into_iter()
                .map(|(key, mut child)| {
                    replaced(&mut child, from, to);
                    (key.replace(from, to), child)
                })
                .collect::<Map<String, Value>>();
        }
        Value::Array(items) => items.iter_mut().for_each(|item| replaced(item, from, to)),
        _ => {}
    }
}

#[test]
fn a_key_renamed_with_the_keys_inside_it_keeps_the_hosts_forms() {
    let mut failures = Vec::new();
    let cases = [
        (
            Language::TOML,
            "[tool.x]\nname = \"n\"\nports = { port = 1, other = { port = 2 } }\nlist = [{ port = 1 }, { port = 2 }]\n",
        ),
        (
            Language::YAML,
            "tool:\n  name: n\n  ports: {port: 1, other: {port: 2}}  # inline\n  list:\n    - port: 1\n",
        ),
        (
            Language::JSON,
            "{\n  \"name\": \"n\",\n  \"ports\": { \"port\": 1, \"other\": { \"port\": 2 } }\n}\n",
        ),
        (
            Language::HCL,
            "name = \"n\"\nports = { port = 1, other = { port = 2 } }\n",
        ),
    ];
    for (host, text) in cases {
        let expected = text.replace("port", "p");
        from_every_view(
            &mut failures,
            "rename a key and the keys inside it",
            host,
            text,
            &|value| replaced(value, "port", "p"),
            &expected,
        );
    }
    report(failures);
}
