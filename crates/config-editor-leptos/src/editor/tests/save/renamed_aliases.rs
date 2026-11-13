//! A key renamed in another view that holds YAML merges and aliases (a
//! Compose service `api` renamed `backend`): only the key changes, the
//! merges and aliases inside it stay.

use crate::editor::Language;
use crate::editor::save_matrix::{from_every_view, report};
use serde_json::{Map, Value};

const HOST: &str = "x-logging: &log\n  driver: json-file\nx-env: &env\n  TZ: UTC\nservices:\n  db:\n    environment:\n      <<: *env\n      DB: app # db\n    logging: *log\n  api:\n    environment:\n      <<: *env\n      URL: postgres://db\n    logging: *log\n";

type Change = fn(&mut Value);

fn renamed(map: &mut Map<String, Value>, from: &str, to: &str) {
    *map = std::mem::take(map)
        .into_iter()
        .map(|(key, held)| (if key == from { to.to_string() } else { key }, held))
        .collect();
}

#[test]
fn a_renamed_key_keeps_the_merges_and_aliases_inside_it() {
    let mut failures = Vec::new();
    let cases: [(&str, Change, String); 3] = [
        (
            "a service renamed",
            |value| renamed(value["services"].as_object_mut().unwrap(), "api", "backend"),
            HOST.replace("  api:\n", "  backend:\n"),
        ),
        (
            "the first service renamed",
            |value| renamed(value["services"].as_object_mut().unwrap(), "db", "database"),
            HOST.replace("  db:\n", "  database:\n"),
        ),
        (
            "the map holding the merge renamed",
            |value| {
                renamed(
                    value["services"]["api"].as_object_mut().unwrap(),
                    "environment",
                    "env",
                )
            },
            HOST.replacen(
                "    environment:\n      <<: *env\n      URL",
                "    env:\n      <<: *env\n      URL",
                1,
            ),
        ),
    ];
    for (label, change, expected) in cases {
        from_every_view(
            &mut failures,
            label,
            Language::YAML,
            HOST,
            &change,
            &expected,
        );
    }
    report(failures);
}
