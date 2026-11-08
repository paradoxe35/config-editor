//! Edit-as saves of lists whose items move while others are added or
//! removed, checked by property: every list of every commented host, edited
//! from every view by random permutations with inserts and deletes, saves
//! to a host that shows the edit again, with each comment still on its
//! item (or gone with it), or the save waits.

use super::Language;
use super::bridge::Bridge;
use super::codec;
use super::save_properties::{ALL, Random, comment_pairs, hosts, pointers, schema};
use serde_json::Value;

/// Hosts with lists the round 21 testers moved: block and flow items with
/// comment lines above them, repeated XML elements with children and
/// attributes, TOML and HCL lists of inline tables.
fn moving_hosts() -> Vec<(Language, &'static str)> {
    vec![
        (
            Language::YAML,
            "tags:\n  # first tag\n  - web\n  - 'api'   # quoted\nn: 1\n",
        ),
        (
            Language::XML,
            "<r>\n  <!-- first -->\n  <t>web</t>\n  <t>api</t> <!-- q -->\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <!-- about a -->\n  <s name=\"a\">\n    <p>1</p>\n  </s>\n  <!-- about b -->\n  <s name=\"b\">\n    <p>2</p>\n  </s>\n  <s name=\"c\"/> <!-- tc -->\n</r>\n",
        ),
        (
            Language::TOML,
            "s = [\n  # about a\n  { name = \"a\", p = 1 },\n  # about b\n  { name = \"b\", p = 2 }, # tb\n  { name = \"c\", p = 3 },\n]\n",
        ),
        (
            Language::HCL,
            "s = [\n  # about a\n  { name = \"a\", p = 1 },\n  # about b\n  { name = \"b\", p = 2 }, # tb\n  { name = \"c\", p = 3 },\n]\n",
        ),
        (
            Language::YAML,
            "s:\n  # about a\n  - name: a\n    p: 1\n  # about b\n  - name: b # nb\n    p: 2\n  - name: c\n    p: 3\n",
        ),
        (
            Language::JSON,
            "{\n  \"s\": [\n    \"a\",\n    \"b\",\n    \"c\"\n  ]\n}\n",
        ),
    ]
}

/// A new item like the list's `items`, sharing no value with them: text,
/// a number, or a copy of the first item with each scalar changed.
fn new_item(items: &[Value], serial: usize) -> Option<Value> {
    match &items[0] {
        Value::String(_) => Some(Value::String(format!("n{serial}"))),
        Value::Number(_) => Some(Value::from(900 + serial)),
        Value::Object(map) => Some(Value::Object(
            map.iter()
                .map(|(key, value)| {
                    let value = match value {
                        Value::String(_) => Value::String(format!("n{serial}")),
                        Value::Number(_) => Value::from(900 + serial),
                        other => other.clone(),
                    };
                    (key.clone(), value)
                })
                .collect(),
        )),
        _ => None,
    }
}

/// Whether `moved` holds some items of `items` in another order.
fn reorders(items: &[Value], moved: &[Value]) -> bool {
    let mut used = vec![false; items.len()];
    let mut order = Vec::new();
    for item in moved {
        if let Some(at) = (0..items.len()).find(|at| !used[*at] && items[*at] == *item) {
            used[at] = true;
            order.push(at);
        }
    }
    order.windows(2).any(|pair| pair[0] > pair[1])
}

/// The list `items` moved and resized: reversed with an item added at the
/// end or the start, or its first removed, then random permutations with
/// an item added and one removed. Each moves some items past others.
fn variants(items: &[Value], random: &mut Random) -> Vec<Vec<Value>> {
    let mut out = Vec::new();
    let Some(added) = new_item(items, 1) else {
        return out;
    };
    let mut reversed = items.to_vec();
    reversed.reverse();
    let mut appended = reversed.clone();
    appended.push(added.clone());
    out.push(appended);
    let mut prepended = reversed.clone();
    prepended.insert(0, added.clone());
    out.push(prepended);
    out.push(reversed[..reversed.len() - 1].to_vec());
    for serial in 2..8 {
        let mut moved = items.to_vec();
        for at in (1..moved.len()).rev() {
            moved.swap(at, random.below(at + 1));
        }
        if random.below(2) == 0 {
            moved.remove(random.below(moved.len()));
        }
        if let Some(item) = new_item(items, serial).filter(|_| random.below(3) > 0) {
            moved.insert(random.below(moved.len() + 1), item);
        }
        out.push(moved);
    }
    out.retain(|moved| moved.len() >= 2 && reorders(items, moved));
    out
}

/// What `saved` did to the comments of `host_text`: each stays with its
/// code (after it on its line, or right above it), or goes with it.
fn misplaced_comments(host: Language, host_text: &str, saved: &str) -> Vec<String> {
    let kept = comment_pairs(host, saved);
    let has_comment = |comment: &str| saved.lines().any(|line| line.contains(comment));
    let has_code = |code: &str| {
        saved
            .lines()
            .any(|line| line.trim().trim_end_matches(',').starts_with(code))
    };
    comment_pairs(host, host_text)
        .into_iter()
        .filter(|pair| !kept.contains(pair))
        .filter(|(_, code, comment)| has_comment(comment) || has_code(code))
        .map(|(kind, code, comment)| format!("{comment} no longer {kind} {code}"))
        .collect()
}

#[test]
fn items_moved_added_and_removed_from_every_view_keep_their_comments() {
    let schema = schema();
    let shape = Some(&schema.schema);
    let mut random = Random(0x0b21);
    let mut failures = Vec::new();
    let (mut tried, mut saved_count) = (0, 0);
    let mut waits = Vec::new();
    for (host, host_text) in moving_hosts().into_iter().chain(hosts()) {
        for edit in ALL.into_iter().filter(|edit| edit.label() != host.label()) {
            let Some(bridge) = Bridge::new(host, edit, Some(schema.clone())) else {
                continue;
            };
            let Ok(shown) = bridge.to_edit(host_text) else {
                continue;
            };
            let Ok(value) = codec::read(edit.behavior(), &shown.text, shape) else {
                continue;
            };
            let mut all = Vec::new();
            pointers(&value, String::new(), &mut all);
            for (pointer, item) in all {
                let Some(items) = item.as_array() else {
                    continue;
                };
                if items.len() < 2 || items.iter().all(|other| *other == items[0]) {
                    continue;
                }
                for moved in variants(items, &mut random) {
                    let mut edited = value.clone();
                    *edited.pointer_mut(&pointer).unwrap() = Value::Array(moved.clone());
                    let Ok(edited_text) =
                        codec::serialize(edit.behavior(), &edited, Some(&shown.text))
                    else {
                        continue;
                    };
                    let Ok(edited) = codec::read(edit.behavior(), &edited_text, shape) else {
                        continue;
                    };
                    tried += 1;
                    let case = format!(
                        "{} host via {}: {pointer} -> {}",
                        host.label(),
                        edit.label(),
                        Value::Array(moved)
                    );
                    let saved = match bridge.to_host(&edited_text, host_text) {
                        Ok(saved) => saved,
                        Err(error) => {
                            waits.push(format!("{case}: {error:?}"));
                            continue;
                        }
                    };
                    saved_count += 1;
                    let mut problems = misplaced_comments(host, host_text, &saved.text);
                    let seen = bridge
                        .to_edit(&saved.text)
                        .ok()
                        .and_then(|shown| codec::read(edit.behavior(), &shown.text, shape).ok());
                    if seen.as_ref() != Some(&edited) {
                        problems.push("does not show the edit when shown again".to_string());
                    }
                    if !problems.is_empty() {
                        failures.push(format!(
                            "{case}: {}\n--- host\n{host_text}--- saved\n{}",
                            problems.join("; "),
                            saved.text
                        ));
                    }
                }
            }
        }
    }
    // Only what the view cannot show (XML lists inside lists) waits.
    waits.retain(|wait| !wait.contains("cannot show the value"));
    assert!(
        waits.is_empty() && saved_count * 10 > tried * 9,
        "{saved_count} of {tried} saved; waited:\n{}",
        waits.join("\n")
    );
    assert!(
        failures.is_empty(),
        "{} of {tried} failed:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
