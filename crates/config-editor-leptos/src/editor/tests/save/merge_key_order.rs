//! Keys a YAML merge (`<<: *x`) brings in are not written in the map: a
//! key the view adds after them is written after the merge line, and later
//! edits in the map save (QA `E37-3`: the new key went above the merge, and
//! every later save waited on a merged key's place).

use crate::editor::Language;
use crate::editor::save_matrix::{ALL, report, save_steps};
use serde_json::{Value, json};

type Change = fn(&mut Value);

const ONE: &str = "c: &c\n  r: 1\n  l: 2\ns:\n  <<: *c\n  i: 16\n  e: 5\n";
const TWO: &str = "c: &c\n  r: 1\n  l: 2\nd: &d\n  m: 3\ns:\n  <<: [*c, *d]\n  i: 16\n  e: 5\n";
const OVERRIDE: &str = "c: &c\n  r: 1\n  l: 2\ns:\n  <<: *c\n  l: 20\n  i: 16\n  e: 5\n";

fn map(value: &mut Value) -> &mut serde_json::Map<String, Value> {
    value["s"].as_object_mut().unwrap()
}

fn index_of(value: &Value, key: &str) -> usize {
    value["s"]
        .as_object()
        .unwrap()
        .keys()
        .position(|k| k == key)
        .unwrap()
}

/// `h` put right before `i`.
fn add_before_i(value: &mut Value) {
    let at = index_of(value, "i");
    map(value).shift_insert(at, "h".to_string(), json!("x"));
}

fn edit_i(value: &mut Value) {
    value["s"]["i"] = json!(17);
}

#[test]
fn edits_around_merged_keys_save_in_every_view() {
    let cases: [(&str, &str, Change, &str); 6] = [
        (
            "a key added after the merged ones",
            ONE,
            add_before_i,
            "c: &c\n  r: 1\n  l: 2\ns:\n  <<: *c\n  h: x\n  i: 17\n  e: 5\n",
        ),
        (
            "a key added after the keys of two merges",
            TWO,
            add_before_i,
            "c: &c\n  r: 1\n  l: 2\nd: &d\n  m: 3\ns:\n  <<: [*c, *d]\n  h: x\n  i: 17\n  e: 5\n",
        ),
        (
            "a key added at the end",
            ONE,
            |value| {
                map(value).insert("h".to_string(), json!("x"));
            },
            "c: &c\n  r: 1\n  l: 2\ns:\n  <<: *c\n  i: 17\n  e: 5\n  h: x\n",
        ),
        (
            "a merged value overridden",
            ONE,
            |value| value["s"]["r"] = json!(9),
            "c: &c\n  r: 1\n  l: 2\ns:\n  r: 9\n  <<: *c\n  i: 17\n  e: 5\n",
        ),
        (
            "an override removed: the merge is written out",
            OVERRIDE,
            |value| {
                map(value).shift_remove("l");
            },
            "c: &c\n  r: 1\n  l: 2\ns:\n  r: 1\n  i: 17\n  e: 5\n",
        ),
        (
            "an override renamed: the merge is written out",
            OVERRIDE,
            |value| {
                let at = index_of(value, "l");
                let held = map(value).shift_remove("l").unwrap();
                map(value).shift_insert(at, "k".to_string(), held);
            },
            "c: &c\n  r: 1\n  l: 2\ns:\n  r: 1\n  k: 20\n  i: 17\n  e: 5\n",
        ),
    ];
    let mut failures = Vec::new();
    for (label, host, change, expected) in cases {
        for view in ALL.into_iter().filter(|view| *view != Language::YAML) {
            match save_steps(Language::YAML, view, host, &[&change, &edit_i]) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!(
                    "{label} via {view}:\n--- expected\n{expected}--- saved\n{saved}"
                )),
                Err(error) => failures.push(format!("{label} via {view}: {error}")),
            }
        }
    }
    report(failures);
}
