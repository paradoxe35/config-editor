//! Edit-as saves checked by property: random structural edits, made in every
//! edit language on commented hosts in every language, must save to a host
//! that
//!
//! 1. shows the edited document when shown in the edit language again,
//! 2. changes no more lines than rewriting just that edit would, and
//! 3. keeps every comment when the edit adds, changes or renames.
//!
//! The edits are pseudo-random from a fixed seed, so a failure always
//! reproduces; each failure prints the host, the edit and the saved text.

use super::Language;
use super::bridge::Bridge;
use super::codec;
use super::completion::conform_arrays;
use super::diagnostics::{SchemaBundle, build_schema};
use super::diff::{DiffEdit, myers_edits};
use super::language::Repetition;
use serde_json::{Map, Value, json};
use std::sync::Arc;

/// Commented hosts with the shapes saves must keep: one-line containers,
/// dotted keys, labelled and one-line blocks, attributes-only elements.
fn hosts() -> Vec<(Language, &'static str)> {
    vec![
        (
            Language::YAML,
            "# head\nlisten: 1 # port\ntcp: {nodelay: true}\nself:\n  allow: [a, b, c] # cidrs\n# clients\nclients:\n  # acme\n  acme:\n    secret: s1\n    max_connections: 2\n  beta:\n    secret: s2\nlimits:\n  max_pending: 5 # mp\n",
        ),
        (
            Language::JSON,
            "{\n  \"listen\": 1,\n  \"tcp\": {\"nodelay\": true},\n  \"self\": {\n    \"allow\": [\"a\", \"b\", \"c\"]\n  },\n  \"clients\": {\n    \"acme\": {\n      \"secret\": \"s1\",\n      \"max_connections\": 2\n    },\n    \"beta\": {\n      \"secret\": \"s2\"\n    }\n  },\n  \"limits\": {\n    \"max_pending\": 5\n  }\n}\n",
        ),
        (
            Language::TOML,
            "# head\nlisten = 1 # port\ntcp = { nodelay = true }\nlimits.max_pending = 5 # mp\nudp.buffer = 3 # udp\n\n[self]\nallow = [\"a\", \"b\", \"c\"] # cidrs\n\n# acme\n[clients.acme]\nsecret = \"s1\"\nmax_connections = 2\n\n# beta\n[clients.beta]\nsecret = \"s2\"\nreach = {}\nadmin_from = [\"x\"]\n\n# pool section\n[pool]\nmin_idle = 1\n",
        ),
        (
            Language::HCL,
            "# head\nlisten = 1 # port\ntcp { nodelay = true }\n\nself {\n  allow = [\"a\", \"b\", \"c\"] # cidrs\n}\n\nclients {\n  # acme\n  acme {\n    secret = \"s1\"\n    max_connections = 2\n  }\n  beta {\n    secret = \"s2\"\n  }\n}\n\nlimits {\n  max_pending = 5 # mp\n}\n",
        ),
        (
            Language::XML,
            "<!-- head -->\n<listen>1</listen> <!-- port -->\n<tcp><nodelay>true</nodelay></tcp>\n<self>\n  <allow>a</allow>\n  <allow>b</allow>\n  <allow>c</allow>\n</self>\n<clients>\n  <!-- acme -->\n  <acme secret=\"s1\" max_connections=\"2\"/>\n  <beta>\n    <secret>s2</secret>\n  </beta>\n</clients>\n<limits>\n  <max_pending>5</max_pending> <!-- mp -->\n</limits>\n",
        ),
        // Round 8 shapes: siblings sharing a one-line container's value,
        // multi-line arrays, dotted keys sharing prefixes and holding
        // multi-line strings.
        (
            Language::YAML,
            "# c\nlisten: 1\ntcp: {nodelay: true}\nudp:\n  nodelay: true # u\nweb:\n  allow:\n    - a # w\n    - b\nself:\n  allow: [a, b]\n",
        ),
        (
            Language::JSON,
            "{\n  \"tcp\": {\"nodelay\": true},\n  \"udp\": {\n    \"nodelay\": true\n  },\n  \"listen\": 1\n}\n",
        ),
        (
            Language::TOML,
            "# c\nlisten = 1 # l\ntcp = { nodelay = true }\nallow = [\n  \"x\", # x\n  \"y\",\n]\nweb.http = 80 # h\nweb.https = 443 # s\nservices.web.health_check.interval_secs = 2 # hc\n\n[udp]\nnodelay = true # u\n",
        ),
        (
            Language::TOML,
            "# c\nweb.error_pages = \"\"\"\nmulti\nline\"\"\"\nlisten = 1 # l\n",
        ),
        (
            Language::HCL,
            "# top\nlisten = 1\n\n# tcp opts\ntcp { nodelay = true } # inline\n\n# udp\nudp {\n  nodelay = true # u\n}\n\nallow = [\n  \"x\", # x\n  \"y\",\n]\n",
        ),
        (
            Language::HCL,
            "x {\n  s {\n    secret = \"a\"\n  }\n  n {\n    v = [\"p\"]\n  }\n  m {\n    c = [\"e\"]\n  }\n  w {\n    c = [\"e\"]\n    # note\n    v = [\"p\"]\n  }\n}\n",
        ),
        // Round 9 shapes: own-line comments inside multi-line lists, dotted
        // groups followed by other keys, dotted multi-line strings, compact
        // JSON.
        (
            Language::TOML,
            "# c\nlisten = 1\n\n[self]\nallow = [\n  \"127.0.0.1/32\",\n  # private\n  \"10.0.0.0/8\"\n]\n# tail\n",
        ),
        (
            Language::HCL,
            "# head\nlisten = 1 # port\nself {\n  allow = [\n    \"a\", # lo\n    # private\n    \"b\",\n  ]\n}\n# tail\n",
        ),
        (
            Language::TOML,
            "# top\nweb.http = \"a\"\nweb.https = \"b\"\nlisten = 1 # l\nservices.api.target.address = \"x\"\nservices.api.target.port = 1\nservices.api.mode = \"tcp\" # m\n",
        ),
        (
            Language::TOML,
            "# top\nweb.hsts = true # s\nweb.error_pages = \"\"\"\n<h1>x</h1>\n\"\"\"\nlisten = 1 # l\n",
        ),
        (
            Language::JSON,
            "{\"listen\":1,\"tcp\":{\"nodelay\":true},\"udp\":{\"nodelay\":true}}\n",
        ),
        (
            Language::XML,
            "<!-- c -->\n<tcp><nodelay>true</nodelay></tcp>\n<udp>\n  <nodelay>true</nodelay> <!-- u -->\n</udp>\n<listen>1</listen>\n",
        ),
    ]
}

const ALL: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// A small deterministic generator (an LCG): no dependency, same cases on
/// every run.
struct Random(u64);

impl Random {
    fn below(&mut self, bound: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % bound.max(1)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    AddKey,
    ChangeScalar,
    DeleteKey,
    RenameKey,
    ContainerToScalar,
    ScalarToContainer,
    AddItem,
    DeleteItem,
}

const OPS: [Op; 8] = [
    Op::AddKey,
    Op::ChangeScalar,
    Op::DeleteKey,
    Op::RenameKey,
    Op::ContainerToScalar,
    Op::ScalarToContainer,
    Op::AddItem,
    Op::DeleteItem,
];

/// Every pointer in `value` with its value, parents first.
fn pointers(value: &Value, pointer: String, out: &mut Vec<(String, Value)>) {
    out.push((pointer.clone(), value.clone()));
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                pointers(child, format!("{pointer}/{key}"), out);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                pointers(child, format!("{pointer}/{index}"), out);
            }
        }
        _ => {}
    }
}

fn parent_and_key(pointer: &str) -> (&str, &str) {
    let at = pointer.rfind('/').unwrap_or(0);
    (&pointer[..at], &pointer[(at + 1).min(pointer.len())..])
}

/// `value` with `op` applied at a random place it applies to, and a label.
fn apply(value: &Value, op: Op, random: &mut Random, serial: usize) -> Option<(Value, String)> {
    let mut all = Vec::new();
    pointers(value, String::new(), &mut all);
    let is_key = |pointer: &str| {
        let (parent, _) = parent_and_key(pointer);
        !pointer.is_empty() && value.pointer(parent).is_some_and(Value::is_object)
    };
    let candidates: Vec<&(String, Value)> = all
        .iter()
        .filter(|(pointer, item)| match op {
            Op::AddKey => item.is_object(),
            Op::ChangeScalar => !item.is_object() && !item.is_array(),
            Op::DeleteKey | Op::RenameKey => is_key(pointer) && !pointer.is_empty(),
            Op::ContainerToScalar => is_key(pointer) && (item.is_object() || item.is_array()),
            Op::ScalarToContainer => is_key(pointer) && !item.is_object() && !item.is_array(),
            Op::AddItem => item.is_array(),
            Op::DeleteItem => item.as_array().is_some_and(|items| items.len() > 1),
        })
        .collect();
    if candidates.is_empty() {
        return None;
    }
    let (pointer, item) = candidates[random.below(candidates.len())].clone();
    let mut edited = value.clone();
    let label = format!("{op:?} at {pointer:?}");
    match op {
        Op::AddKey => {
            edited
                .pointer_mut(&pointer)?
                .as_object_mut()?
                .insert(format!("added_{serial}"), json!(serial));
        }
        Op::ChangeScalar => {
            *edited.pointer_mut(&pointer)? = match item {
                Value::Number(_) => json!(serial + 100),
                Value::Bool(flag) => json!(!flag),
                _ => json!(format!("changed{serial}")),
            };
        }
        Op::DeleteKey | Op::RenameKey => {
            let (parent, key) = parent_and_key(&pointer);
            let map = edited.pointer_mut(parent)?.as_object_mut()?;
            let mut rebuilt = Map::new();
            for (name, child) in std::mem::take(map) {
                if name == key {
                    if op == Op::RenameKey {
                        rebuilt.insert(format!("{name}_renamed"), child);
                    }
                } else {
                    rebuilt.insert(name, child);
                }
            }
            *map = rebuilt;
        }
        Op::ContainerToScalar => *edited.pointer_mut(&pointer)? = json!(7),
        Op::ScalarToContainer => *edited.pointer_mut(&pointer)? = json!({ "inner": 1 }),
        Op::AddItem => {
            let items = edited.pointer_mut(&pointer)?.as_array_mut()?;
            let item = items.last().cloned().unwrap_or(json!("x"));
            items.push(if item.is_string() {
                json!(format!("item{serial}"))
            } else {
                item
            });
        }
        Op::DeleteItem => {
            edited.pointer_mut(&pointer)?.as_array_mut()?.remove(0);
        }
    }
    (edited != *value).then_some((edited, label))
}

/// Lines changed between `before` and `after` (inserted plus deleted).
fn changed_lines(before: &str, after: &str) -> usize {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    myers_edits(&old, &new)
        .map(|edits| {
            edits
                .iter()
                .filter(|edit| **edit != DiffEdit::Equal)
                .count()
        })
        .unwrap_or(old.len() + new.len())
}

fn comments(language: Language, text: &str) -> Vec<String> {
    let marker = if language.label() == "XML" {
        "<!--"
    } else {
        "#"
    };
    text.lines()
        .filter_map(|line| line.find(marker).map(|at| line[at..].trim().to_string()))
        .collect()
}

/// The demo's schema: XML reads a lone repeated element as a list by it.
fn schema() -> Arc<SchemaBundle> {
    Arc::new(build_schema(include_str!("../../tests/fixtures/burrow.schema.json")).unwrap())
}

/// `value` with every one-item list replaced by its item.
fn without_one_item_lists(value: &Value) -> Value {
    match value {
        Value::Array(items) if items.len() == 1 => without_one_item_lists(&items[0]),
        Value::Array(items) => Value::Array(items.iter().map(without_one_item_lists).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, child)| (key.clone(), without_one_item_lists(child)))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// One saved edit checked against the three properties, or why it failed.
fn check(
    (host, edit, schema): (Language, Language, &Arc<SchemaBundle>),
    host_text: &str,
    op: Op,
    random: &mut Random,
    serial: usize,
) -> Option<String> {
    let bridge = Bridge::new(host, edit, Some(schema.clone()))?;
    let shape = Some(&schema.schema);
    let shown = bridge.to_edit(host_text).ok()?.text;
    let value = codec::read(edit.behavior(), &shown, shape).ok()?;
    let (edited, label) = apply(&value, op, random, serial)?;
    // The edit as the user would have typed it, in the edit language.
    let edited_text = codec::serialize(edit.behavior(), &edited, Some(&shown)).ok()?;
    let Ok(edited) = codec::read(edit.behavior(), &edited_text, shape) else {
        return None;
    };
    let case = format!("{} host, edited as {}: {label}", host.label(), edit.label());
    let saved = match bridge.to_host(&edited_text, host_text) {
        Ok(saved) => saved.text,
        // A document the host cannot write is reported to the user instead.
        Err(_) => return None,
    };
    let old_host_value = codec::read(host.behavior(), host_text, shape).ok()?;
    // What a full rewrite of the host would hold and how much it changes.
    let full_old = codec::render(&old_host_value, host.behavior(), Some(host_text), shape)
        .ok()?
        .text;
    let edited_host_value = codec::read(
        host.behavior(),
        &codec::render(&edited, host.behavior(), Some(host_text), shape)
            .ok()?
            .text,
        shape,
    )
    .ok()?;
    let full_new = codec::render(&edited_host_value, host.behavior(), Some(host_text), shape)
        .ok()?
        .text;
    let mut problems = Vec::new();
    // Shown again in the edit language, the saved host is what was typed
    // (values the view cannot tell apart keep the host's own, as before).
    match codec::read(host.behavior(), &saved, shape) {
        Ok(_) => {
            let seen = bridge
                .to_edit(&saved)
                .ok()
                .and_then(|shown| codec::read(edit.behavior(), &shown.text, shape).ok());
            // The host reads a lone repeated element by the schema's shape, as
            // a list or a value; an edit off that shape is normalised so.
            let mut normalised = edited.clone();
            for language in [host, edit] {
                if language.behavior().repetition() != Repetition::None {
                    conform_arrays(
                        &mut normalised,
                        &schema.schema,
                        language.behavior().repetition(),
                    );
                }
            }
            // XML cannot tell a one-item list from its item where the schema
            // allows both (`secret`: a string or a list); a warning says so.
            let lone_items = (host.label() == "XML").then(|| without_one_item_lists(&normalised));
            if seen.as_ref() != Some(&edited)
                && seen.as_ref() != Some(&normalised)
                && seen != lone_items
            {
                problems.push("does not show the edit when shown again".to_string());
            }
        }
        Err(error) => problems.push(format!("does not parse: {}", error.message)),
    }
    // Slack for blank lines, an emptied table written inline (`self = {}`)
    // and a one-line container that must open over lines (an HCL block
    // given a second attribute). A rewrite of the whole host is far over.
    let budget = changed_lines(&full_old, &full_new) + 5;
    let changed = changed_lines(host_text, &saved);
    if changed > budget {
        problems.push(format!(
            "changes {changed} lines, a rewrite of the edit {budget}"
        ));
    }
    // A rename or a scalar becoming a container keeps every comment too.
    if matches!(
        op,
        Op::AddKey | Op::ChangeScalar | Op::AddItem | Op::RenameKey | Op::ScalarToContainer
    ) {
        let kept = comments(host, &saved);
        let lost: Vec<String> = comments(host, host_text)
            .into_iter()
            .filter(|comment| !kept.contains(comment))
            .collect();
        if !lost.is_empty() {
            problems.push(format!("loses comments {lost:?}"));
        }
    }
    (!problems.is_empty()).then(|| {
        format!(
            "{case}: {}\n--- host\n{host_text}--- saved\n{saved}",
            problems.join("; ")
        )
    })
}

#[test]
fn random_structural_edits_change_only_their_lines_and_keep_comments() {
    let schema = schema();
    let mut random = Random(0x5eed);
    let mut failures = Vec::new();
    let mut checked = 0;
    for (host, host_text) in hosts() {
        for edit in ALL.into_iter().filter(|edit| edit.label() != host.label()) {
            for (serial, op) in OPS.iter().cycle().take(OPS.len() * 8).enumerate() {
                checked += 1;
                if let Some(failure) =
                    check((host, edit, &schema), host_text, *op, &mut random, serial)
                {
                    failures.push(failure);
                }
            }
        }
    }
    let summary: Vec<String> = failures
        .iter()
        .map(|failure| failure.lines().next().unwrap_or("").to_string())
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {checked} edits failed:\n{}\n\nfirst failures:\n{}",
        failures.len(),
        summary.join("\n"),
        failures
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n"),
    );
}

/// The demo's large samples (about 2,000 lines each), with fewer edits.
#[test]
fn random_structural_edits_on_the_large_samples_change_only_their_lines() {
    let samples = [
        (
            Language::YAML,
            include_str!("../../tests/fixtures/samples/burrow.large.yaml"),
        ),
        (
            Language::JSON,
            include_str!("../../tests/fixtures/samples/burrow.large.json"),
        ),
        (
            Language::TOML,
            include_str!("../../tests/fixtures/samples/burrow.large.toml"),
        ),
        (
            Language::HCL,
            include_str!("../../tests/fixtures/samples/burrow.large.hcl"),
        ),
        (
            Language::XML,
            include_str!("../../tests/fixtures/samples/burrow.large.xml"),
        ),
    ];
    let schema = schema();
    let mut random = Random(0x1a26e);
    let mut failures = Vec::new();
    let mut checked = 0;
    for (host, host_text) in samples {
        for edit in ALL.into_iter().filter(|edit| edit.label() != host.label()) {
            for (serial, op) in OPS.iter().enumerate() {
                checked += 1;
                if let Some(failure) =
                    check((host, edit, &schema), host_text, *op, &mut random, serial)
                {
                    failures.push(failure.lines().next().unwrap_or("").to_string());
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {checked} edits failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
