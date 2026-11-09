//! L22 L11: keys YAML 1.1 readers take for booleans (`n`, `y`, `on`) are
//! quoted when the editor writes them, unless the host writes them bare
//! already: then its readers take them as text, and a new one added from
//! another view is bare beside them, in a new list item as in a map.

use super::Language;
use super::save_matrix::{from_every_view, report};
use serde_json::{Value, json};

type Change = fn(&mut Value);

#[test]
fn keys_the_host_writes_bare_are_written_bare_from_every_view() {
    let cases: [(&str, Change, &str); 3] = [
        (
            "l:\n  - n: a\n    y: b\n  - n: x\n    y: z\n",
            |value| {
                value["l"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"n": "c", "y": "d"}))
            },
            "l:\n  - n: a\n    y: b\n  - n: x\n    y: z\n  - n: c\n    y: d\n",
        ),
        (
            "on: 1\nm:\n  k: 2\n",
            |value| value["m"]["on"] = json!(3),
            "on: 1\nm:\n  k: 2\n  on: 3\n",
        ),
        // Not written bare by the host: quoted, as YAML 1.1 readers need.
        (
            "a: 1\nm:\n  k: 2\n",
            |value| value["m"]["y"] = json!(3),
            "a: 1\nm:\n  k: 2\n  \"y\": 3\n",
        ),
    ];
    let mut failures = Vec::new();
    for (host, change, expected) in cases {
        from_every_view(&mut failures, host, Language::YAML, host, &change, expected);
    }
    report(failures);
}
