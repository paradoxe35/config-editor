//! The keys a YAML block list item already has are the ones written in
//! that item: its dash line and the lines below it, never the keys of the
//! items before or after it. Each caret form (an empty `- `, a key half
//! typed after the dash, a line below a key) in each place (before,
//! after, between items, past a blank line or a comment, in a document
//! that does not parse elsewhere), in a YAML document and in the YAML
//! view of each host.
//!
//! `|` marks the caret; `@` stands for the caret's lines in a frame.

use super::super::bridge::Bridge;
use super::super::completion::completions;
use super::super::diagnostics::build_schema;
use super::Language;

const SCHEMA: &str = r#"{"type":"object","properties":{"spec":{"type":"object","properties":{
    "containers":{"type":"array","items":{"type":"object",
        "properties":{"name":{"type":"string"},"image":{"type":"string"},"cmd":{"type":"string"}}}},
    "env":{"type":"array","items":{"type":"object","properties":{
        "name":{"type":"string"},"value":{"type":"string"},
        "valueFrom":{"type":"object","properties":{"key":{"type":"string"}}}}}}}}}}"#;

/// The caret's item, at the item column `-`: its text and the keys
/// completion offers there.
const FORMS: &[(&str, &[&str])] = &[
    ("- |", &["cmd", "image", "name"]),
    ("- na|", &["name"]),
    ("- image: x\n  |", &["cmd", "name"]),
    ("- |\n  image: x", &["cmd", "name"]),
];

/// The list around the caret's item (`@`), its items written at column 4.
const FRAMES: &[&str] = &[
    // Before an item.
    "spec:\n  containers:\n@\n    - name: a\n      cmd: c\n",
    // After one.
    "spec:\n  containers:\n    - name: a\n      cmd: c\n@\n",
    // Between two.
    "spec:\n  containers:\n    - name: a\n      cmd: c\n@\n    - name: b\n      cmd: d\n",
    // A blank line, then a comment, between it and the next.
    "spec:\n  containers:\n@\n\n    - name: a\n      cmd: c\n",
    "spec:\n  containers:\n@\n    # the next one\n    - name: a\n      cmd: c\n",
    // A key after the list.
    "spec:\n  containers:\n@\n    - name: a\n      cmd: c\nother: 1\n",
    // The rest of the document does not parse.
    "spec:\n  containers:\n@\n    - name: a\n      cmd: c\nbroken: [\n",
];

fn offered(source: &str, at: usize, language: Language) -> Vec<String> {
    let bundle = build_schema(SCHEMA).unwrap();
    let mut labels: Vec<String> =
        completions(source, at, true, Some(&bundle), true, language.behavior())
            .into_iter()
            .map(|option| option.label)
            .collect();
    labels.sort();
    labels
}

fn check(name: &str, marked: &str, wanted: &[&str], failures: &mut Vec<String>) {
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    let labels = offered(&source, at, Language::YAML);
    if labels != wanted {
        failures.push(format!(
            "{name} {marked:?}: offered {labels:?}, wanted {wanted:?}"
        ));
    }
}

fn indented(form: &str, by: &str) -> String {
    form.lines()
        .map(|line| format!("{by}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn an_items_keys_are_its_own_in_every_place() {
    let mut failures = Vec::new();
    for frame in FRAMES {
        for (form, wanted) in FORMS {
            let marked = frame.replacen('@', &indented(form, "    "), 1);
            check("yaml", &marked, wanted, &mut failures);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The shape a Kubernetes container's `env` list has: an entry added
/// above one with `valueFrom` offers every key.
#[test]
fn an_env_entry_added_above_another_offers_every_key() {
    let mut failures = Vec::new();
    let frame = "spec:\n  env:\n@\n    - name: B\n      valueFrom:\n        key: k\n";
    for (form, wanted) in [
        ("- |", &["name", "value", "valueFrom"][..]),
        ("- na|", &["name"]),
        ("- name: A\n  |", &["value", "valueFrom"]),
    ] {
        let marked = frame.replacen('@', &indented(form, "    "), 1);
        check("yaml env", &marked, wanted, &mut failures);
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The YAML view of each host: an item started above the first one.
#[test]
fn an_item_started_in_a_yaml_view_offers_its_own_keys() {
    let value = serde_json::json!({"spec": {"containers": [
        {"name": "a", "cmd": "c"}, {"name": "b", "cmd": "d"}
    ]}});
    let mut failures = Vec::new();
    for host in [Language::JSON, Language::TOML, Language::HCL, Language::XML] {
        let Some(bridge) = Bridge::new(host, Language::YAML, None) else {
            continue;
        };
        let host_text = host.serialize(&value).unwrap();
        let shown = bridge.to_edit(&host_text).expect("shows").text;
        let Some(first) = shown.find("- name: a") else {
            failures.push(format!("{host}: shown {shown:?}"));
            continue;
        };
        let line_start = shown[..first].rfind('\n').map_or(0, |at| at + 1);
        let pad = &shown[line_start..first];
        for (form, wanted) in FORMS {
            let marked = format!(
                "{}{}\n{}",
                &shown[..line_start],
                indented(form, pad),
                &shown[line_start..]
            );
            check(&format!("{host} as yaml"), &marked, wanted, &mut failures);
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
