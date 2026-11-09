//! Map keys put in another order in an Edit-as view reach the host, each
//! entry moving with its own-line comments, its trailing comment and the
//! lines its value runs on (round 26: the view and the host disagreed on
//! the order, the data equal).

use super::Language;
use super::save_matrix::{ALL, report, save_value};
use serde_json::Value;

/// `value` with the keys of the map at `pointer` in `order` (keys left out
/// keep their place after them).
fn ordered(value: &mut Value, pointer: &str, order: &[&str]) {
    let map = value
        .pointer_mut(pointer)
        .and_then(Value::as_object_mut)
        .unwrap_or_else(|| panic!("no map at {pointer}"));
    let mut entries: Vec<(String, Value)> = std::mem::take(map).into_iter().collect();
    entries.sort_by_key(|(key, _)| {
        order
            .iter()
            .position(|other| other == key)
            .unwrap_or(order.len())
    });
    map.extend(entries);
}

/// `host_text` saved after the view puts the map at `pointer` in `order`,
/// from every view but `skip` (views that write that order otherwise:
/// TOML writes a table after the plain keys).
fn check(
    failures: &mut Vec<String>,
    label: &str,
    (host, host_text): (Language, &str),
    (pointer, order): (&str, &[&str]),
    skip: &[Language],
    expected: &str,
) {
    for edit in ALL
        .into_iter()
        .filter(|edit| *edit != host && !skip.contains(edit))
    {
        match save_value(host, edit, host_text, &|value| {
            ordered(value, pointer, order)
        }) {
            Ok(saved) if saved == expected => {}
            Ok(saved) => failures.push(format!(
                "{label}: {host} host via {edit}:\n--- expected\n{expected}--- saved\n{saved}"
            )),
            Err(error) => failures.push(format!("{label}: {host} host via {edit}: {error}")),
        }
    }
}

#[test]
fn keys_reordered_in_a_view_move_in_the_host_with_their_comments() {
    let mut failures = Vec::new();
    let toml_view = [Language::TOML];

    let yaml = "# head\n\na: 1 # one\n# about b\nb: 2\nm:\n  # x\n  x: 1\n  y: 2\nc: 3\n";
    check(
        &mut failures,
        "swapped",
        (Language::YAML, yaml),
        ("", &["b", "a"]),
        &[],
        "# head\n\n# about b\nb: 2\na: 1 # one\nm:\n  # x\n  x: 1\n  y: 2\nc: 3\n",
    );
    check(
        &mut failures,
        "swapped inside",
        (Language::YAML, yaml),
        ("/m", &["y", "x"]),
        &[],
        "# head\n\na: 1 # one\n# about b\nb: 2\nm:\n  y: 2\n  # x\n  x: 1\nc: 3\n",
    );
    check(
        &mut failures,
        "a map moved",
        (Language::YAML, yaml),
        ("", &["m", "a", "b", "c"]),
        &toml_view,
        "# head\n\nm:\n  # x\n  x: 1\n  y: 2\na: 1 # one\n# about b\nb: 2\nc: 3\n",
    );

    let json = "{\n  \"a\": 1,\n  \"b\": 2,\n  \"m\": {\n    \"x\": 1,\n    \"y\": 2\n  },\n  \"c\": 3\n}\n";
    check(
        &mut failures,
        "swapped",
        (Language::JSON, json),
        ("", &["b", "a"]),
        &[],
        "{\n  \"b\": 2,\n  \"a\": 1,\n  \"m\": {\n    \"x\": 1,\n    \"y\": 2\n  },\n  \"c\": 3\n}\n",
    );
    check(
        &mut failures,
        "the last key moved up",
        (Language::JSON, json),
        ("", &["a", "b", "c", "m"]),
        &toml_view,
        "{\n  \"a\": 1,\n  \"b\": 2,\n  \"c\": 3,\n  \"m\": {\n    \"x\": 1,\n    \"y\": 2\n  }\n}\n",
    );
    check(
        &mut failures,
        "swapped inside",
        (Language::JSON, json),
        ("/m", &["y", "x"]),
        &[],
        "{\n  \"a\": 1,\n  \"b\": 2,\n  \"m\": {\n    \"y\": 2,\n    \"x\": 1\n  },\n  \"c\": 3\n}\n",
    );

    let toml = "# head\n\na = 1 # one\n# about b\nb = 2\nc = 3\n\n[m]\n# x\nx = 1\ny = 2\n";
    check(
        &mut failures,
        "swapped",
        (Language::TOML, toml),
        ("", &["b", "a"]),
        &[],
        "# head\n\n# about b\nb = 2\na = 1 # one\nc = 3\n\n[m]\n# x\nx = 1\ny = 2\n",
    );
    check(
        &mut failures,
        "swapped inside",
        (Language::TOML, toml),
        ("/m", &["y", "x"]),
        &[],
        "# head\n\na = 1 # one\n# about b\nb = 2\nc = 3\n\n[m]\ny = 2\n# x\nx = 1\n",
    );

    let hcl = "# head\n\na = 1 # one\n# about b\nb = 2\nm {\n  # x\n  x = 1\n  y = 2\n}\nc = 3\n";
    check(
        &mut failures,
        "swapped",
        (Language::HCL, hcl),
        ("", &["b", "a"]),
        &[],
        "# head\n\n# about b\nb = 2\na = 1 # one\nm {\n  # x\n  x = 1\n  y = 2\n}\nc = 3\n",
    );
    check(
        &mut failures,
        "swapped inside",
        (Language::HCL, hcl),
        ("/m", &["y", "x"]),
        &[],
        "# head\n\na = 1 # one\n# about b\nb = 2\nm {\n  y = 2\n  # x\n  x = 1\n}\nc = 3\n",
    );
    check(
        &mut failures,
        "a block moved",
        (Language::HCL, hcl),
        ("", &["a", "b", "c", "m"]),
        &toml_view,
        "# head\n\na = 1 # one\n# about b\nb = 2\nc = 3\nm {\n  # x\n  x = 1\n  y = 2\n}\n",
    );

    let xml = "<r>\n  <!-- head -->\n\n  <a>1</a> <!-- one -->\n  <!-- about b -->\n  <b>2</b>\n  <m>\n    <x>1</x>\n    <y>2</y>\n  </m>\n  <c>3</c>\n</r>\n";
    check(
        &mut failures,
        "swapped",
        (Language::XML, xml),
        ("/r", &["b", "a"]),
        &[],
        "<r>\n  <!-- head -->\n\n  <!-- about b -->\n  <b>2</b>\n  <a>1</a> <!-- one -->\n  <m>\n    <x>1</x>\n    <y>2</y>\n  </m>\n  <c>3</c>\n</r>\n",
    );
    check(
        &mut failures,
        "an element moved",
        (Language::XML, xml),
        ("/r", &["a", "b", "c", "m"]),
        &toml_view,
        "<r>\n  <!-- head -->\n\n  <a>1</a> <!-- one -->\n  <!-- about b -->\n  <b>2</b>\n  <c>3</c>\n  <m>\n    <x>1</x>\n    <y>2</y>\n  </m>\n</r>\n",
    );
    report(failures);
}

/// A reorder beside a value edit reaches the host too.
#[test]
fn keys_reordered_while_a_value_is_edited_move_in_the_host() {
    let mut failures = Vec::new();
    let yaml = "a: 1 # one\nb: 2\nc: 3\n";
    for edit in ALL.into_iter().filter(|edit| *edit != Language::YAML) {
        let change = |value: &mut Value| {
            ordered(value, "", &["b", "a", "c"]);
            value["c"] = serde_json::json!(4);
        };
        match save_value(Language::YAML, edit, yaml, &change) {
            Ok(saved) if saved == "b: 2\na: 1 # one\nc: 4\n" => {}
            other => failures.push(format!("via {edit}: {other:?}")),
        }
    }
    report(failures);
}
