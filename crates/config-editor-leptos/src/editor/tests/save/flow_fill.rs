//! Empty containers filled, and new containers added, from another view:
//! each is written in the form its place takes in the host. An empty `{}`
//! or `[]` stays on one line where that is the host's form for it (a TOML
//! inline table never becomes a `[table]` moved to the end, an HCL object
//! stays one line), and in YAML where the host writes others filled on one
//! line; a new object in a JSON object over lines is written over lines,
//! at the host's indent.

use crate::editor::Language;
use crate::editor::save_matrix::{from_every_view, report};
use serde_json::{Value, json};

type Change = fn(&mut Value);

const FILL_MAP: Change = |value| value["a"] = json!({"k": 1});
const FILL_LIST: Change = |value| value["b"] = json!([1, 2]);

#[test]
fn an_empty_container_filled_keeps_its_one_line_form_where_the_host_does() {
    let cases: [(Language, &str, Change, &str); 10] = [
        (
            Language::TOML,
            "a = {} # a\nb = []\nn = 1\n",
            FILL_MAP,
            "a = { k = 1 } # a\nb = []\nn = 1\n",
        ),
        (
            Language::TOML,
            "a = {}\nb = [] # b\nn = 1\n",
            FILL_LIST,
            "a = {}\nb = [1, 2] # b\nn = 1\n",
        ),
        (
            Language::HCL,
            "a = {} # a\nb = []\nn = 1\n",
            FILL_MAP,
            "a = { k = 1 } # a\nb = []\nn = 1\n",
        ),
        (
            Language::HCL,
            "a = {}\nb = [] # b\nn = 1\n",
            FILL_LIST,
            "a = {}\nb = [1, 2] # b\nn = 1\n",
        ),
        (
            Language::JSON,
            "{\n  \"a\": {},\n  \"b\": [],\n  \"n\": 1\n}\n",
            FILL_MAP,
            "{\n  \"a\": {\"k\": 1},\n  \"b\": [],\n  \"n\": 1\n}\n",
        ),
        (
            Language::JSON,
            "{\n  \"a\": {},\n  \"b\": [],\n  \"n\": 1\n}\n",
            FILL_LIST,
            "{\n  \"a\": {},\n  \"b\": [1, 2],\n  \"n\": 1\n}\n",
        ),
        // YAML beside others filled on one line: on one line too.
        (
            Language::YAML,
            "a: {} # a\nb: []\nt: [x, y]\nm: {p: 1}\n",
            FILL_MAP,
            "a: {k: 1} # a\nb: []\nt: [x, y]\nm: {p: 1}\n",
        ),
        (
            Language::YAML,
            "a: {}\nb: [] # b\nt: [x, y]\nm: {p: 1}\n",
            FILL_LIST,
            "a: {}\nb: [1, 2] # b\nt: [x, y]\nm: {p: 1}\n",
        ),
        // YAML in block style: `{}` and `[]` are only how it writes them
        // empty, filled they are blocks as the others.
        (
            Language::YAML,
            "a: {}\nb: []\nl:\n  - x\n",
            FILL_MAP,
            "a:\n  k: 1\nb: []\nl:\n  - x\n",
        ),
        (
            Language::YAML,
            "a: {}\nb: []\nl:\n  - x\n",
            FILL_LIST,
            "a: {}\nb:\n  - 1\n  - 2\nl:\n  - x\n",
        ),
    ];
    let mut failures = Vec::new();
    for (host, text, change, expected) in cases {
        from_every_view(&mut failures, text, host, text, &change, expected);
    }
    report(failures);
}

#[test]
fn a_new_object_in_a_json_object_over_lines_is_written_over_lines() {
    let cases: [(&str, Change, &str); 3] = [
        (
            "{\n  \"n\": 1\n}\n",
            |value| value["o"] = json!({"k": {"j": 1}}),
            "{\n  \"n\": 1,\n  \"o\": {\n    \"k\": {\n      \"j\": 1\n    }\n  }\n}\n",
        ),
        (
            "{\n    \"n\": 1,\n    \"m\": {\n        \"x\": 1\n    }\n}\n",
            |value| value["o"] = json!({"k": 1}),
            "{\n    \"n\": 1,\n    \"m\": {\n        \"x\": 1\n    },\n    \"o\": {\n        \"k\": 1\n    }\n}\n",
        ),
        (
            "{\n  \"l\": [\n    {\n      \"a\": 1\n    },\n    {\n      \"a\": 2\n    }\n  ]\n}\n",
            |value| value["l"].as_array_mut().unwrap().push(json!({"a": 3})),
            "{\n  \"l\": [\n    {\n      \"a\": 1\n    },\n    {\n      \"a\": 2\n    },\n    {\n      \"a\": 3\n    }\n  ]\n}\n",
        ),
    ];
    let mut failures = Vec::new();
    for (text, change, expected) in cases {
        from_every_view(&mut failures, text, Language::JSON, text, &change, expected);
    }
    report(failures);
}
