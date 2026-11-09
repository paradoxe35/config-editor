//! A YAML document whose root is a flow collection (`[a, b]`, `{a: 1}`)
//! keeps its form through saves from another view, as a nested flow
//! collection does: items edited, added, removed and moved inside it, its
//! comments kept.

use super::Language;
use super::paths::field_range;
use super::save_matrix::{report, save_value};
use serde_json::{Value, json};

type Change = fn(&mut Value);

/// Views that can show a root list, and a root map.
const LIST_VIEWS: [Language; 1] = [Language::JSON];
const MAP_VIEWS: [Language; 3] = [Language::JSON, Language::TOML, Language::HCL];

fn check(
    failures: &mut Vec<String>,
    views: &[Language],
    host: &str,
    cases: &[(&str, Change, &str)],
) {
    for (label, change, expected) in cases {
        for view in views {
            match save_value(Language::YAML, *view, host, change) {
                Ok(saved) if saved == *expected => {}
                Ok(saved) => failures.push(format!(
                    "{label} via {view}:\n--- host\n{host}--- expected\n{expected}--- saved\n{saved}"
                )),
                Err(error) => failures.push(format!("{label} via {view}: {error}")),
            }
        }
    }
}

#[test]
fn a_flow_list_at_the_root_keeps_its_form_and_comments() {
    let mut failures = Vec::new();
    let host = "# c\n[a, b, c] # t\n";
    let cases: [(&str, Change, &str); 5] = [
        (
            "edit",
            |value| value[1] = json!("bx"),
            "# c\n[a, bx, c] # t\n",
        ),
        (
            "append",
            |value| value.as_array_mut().unwrap().push(json!("d")),
            "# c\n[a, b, c, d] # t\n",
        ),
        (
            "delete",
            |value| {
                value.as_array_mut().unwrap().remove(0);
            },
            "# c\n[b, c] # t\n",
        ),
        (
            "reorder",
            |value| value.as_array_mut().unwrap().reverse(),
            "# c\n[c, b, a] # t\n",
        ),
        (
            "add a map",
            |value| value.as_array_mut().unwrap().push(json!({"k": 1})),
            "# c\n[a, b, c, {k: 1}] # t\n",
        ),
    ];
    check(&mut failures, &LIST_VIEWS, host, &cases);
    report(failures);
}

#[test]
fn a_flow_map_at_the_root_keeps_its_form_and_comments() {
    let mut failures = Vec::new();
    let host = "# c\n{a: 1, b: 2, c: 3} # t\n";
    let cases: [(&str, Change, &str); 4] = [
        (
            "edit",
            |value| value["b"] = json!(20),
            "# c\n{a: 1, b: 20, c: 3} # t\n",
        ),
        (
            "add",
            |value| {
                value.as_object_mut().unwrap().insert("d".into(), json!(4));
            },
            "# c\n{a: 1, b: 2, c: 3, d: 4} # t\n",
        ),
        (
            "delete",
            |value| {
                value.as_object_mut().unwrap().shift_remove("a");
            },
            "# c\n{b: 2, c: 3} # t\n",
        ),
        (
            "rename",
            |value| {
                let object = value.as_object_mut().unwrap();
                *object = std::mem::take(object)
                    .into_iter()
                    .map(|(key, held)| (if key == "b" { "z".into() } else { key }, held))
                    .collect();
            },
            "# c\n{a: 1, z: 2, c: 3} # t\n",
        ),
    ];
    check(&mut failures, &MAP_VIEWS, host, &cases);
    // A root map holding one map: XML shows it too.
    let host = "# c\n{r: {a: 1, b: 2}} # t\n";
    let cases: [(&str, Change, &str); 2] = [
        (
            "nested edit",
            |value| value["r"]["b"] = json!(20),
            "# c\n{r: {a: 1, b: 20}} # t\n",
        ),
        (
            "nested add",
            |value| {
                value["r"]
                    .as_object_mut()
                    .unwrap()
                    .insert("c".into(), json!(3));
            },
            "# c\n{r: {a: 1, b: 2, c: 3}} # t\n",
        ),
    ];
    let views = [Language::JSON, Language::TOML, Language::HCL, Language::XML];
    check(&mut failures, &views, host, &cases);
    report(failures);
}

/// Over lines, with a comment inside: entries kept on their lines.
#[test]
fn a_flow_collection_over_lines_at_the_root_keeps_its_lines() {
    let mut failures = Vec::new();
    let host = "# c\n[\n  a, # first\n  b,\n] # t\n";
    let cases: [(&str, Change, &str); 2] = [
        (
            "edit",
            |value| value[1] = json!("bx"),
            "# c\n[\n  a, # first\n  bx,\n] # t\n",
        ),
        (
            "append",
            |value| value.as_array_mut().unwrap().push(json!("c")),
            "# c\n[\n  a, # first\n  b,\n  c,\n] # t\n",
        ),
    ];
    check(&mut failures, &LIST_VIEWS, host, &cases);
    let host = "# c\n{\n  a: 1, # first\n  b: 2\n} # t\n";
    let cases: [(&str, Change, &str); 2] = [
        (
            "edit",
            |value| value["b"] = json!(20),
            "# c\n{\n  a: 1, # first\n  b: 20\n} # t\n",
        ),
        (
            "add",
            |value| {
                value.as_object_mut().unwrap().insert("c".into(), json!(3));
            },
            "# c\n{\n  a: 1, # first\n  b: 2,\n  c: 3\n} # t\n",
        ),
    ];
    check(&mut failures, &MAP_VIEWS, host, &cases);
    report(failures);
}

/// Every path in a flow root is found where it is written (so its
/// problems and locks are placed there).
#[test]
fn every_path_in_a_flow_root_is_found() {
    let mut failures = Vec::new();
    let yaml = Language::YAML.behavior();
    let documents = [
        (
            "# c\n[a, {k: v}, [x]] # t\n",
            vec![("/0", "a"), ("/1/k", "k: v"), ("/2/0", "x")],
        ),
        (
            "--- {a: 1, b: [2, 3]} # t\n",
            vec![("/a", "a: 1"), ("/b/1", "3")],
        ),
        (
            "# c\n{\n  a: 1, # first\n  b: 2\n}\n",
            vec![("/a", "a: 1"), ("/b", "b: 2")],
        ),
    ];
    for (host, paths) in documents {
        for (pointer, written) in paths {
            let found = field_range(yaml, host, pointer).and_then(|range| {
                let line = host.lines().nth(range.start.line - 1)?;
                (range.start.line == range.end.line).then(|| {
                    line.chars()
                        .skip(range.start.column - 1)
                        .take(range.end.column - range.start.column)
                        .collect::<String>()
                })
            });
            if found.as_deref() != Some(written) {
                failures.push(format!(
                    "{host:?} {pointer}: {found:?}, expected {written:?}"
                ));
            }
        }
    }
    report(failures);
}
