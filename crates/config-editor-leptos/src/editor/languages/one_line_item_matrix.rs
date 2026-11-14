//! A list item written on one line (`{…}`, `<item>…</item>`) stays its
//! own item while a key is typed in it: completion reads the document
//! without the typed text only, so the item keeps its place and its
//! keys, and a `oneOf` or an `if` is decided by this item, never by the
//! next one sliding into its slot. In each language, before, between and
//! after other items, and in the JSON view of each host.
//!
//! `|` marks the caret; `@` stands for the caret's item in a frame.

use super::super::bridge::Bridge;
use super::super::completion::completions;
use super::super::diagnostics::build_schema;
use super::Language;

/// Two options told apart by `kind`'s `const`.
const ONE_OF: &str = r#"{"type":"object","properties":{"outs":{"type":"array","items":{"oneOf":[
    {"type":"object","required":["kind","path"],"properties":{"kind":{"const":"file"},
        "path":{"type":"string"},"mode":{"type":"string"}},"additionalProperties":false},
    {"type":"object","required":["kind","url"],"properties":{"kind":{"const":"http"},
        "url":{"type":"string"},"timeout":{"type":"integer"}},"additionalProperties":false}]}}}}"#;

/// The same split by an `if` on `kind`.
const IF_THEN: &str = r#"{"type":"object","properties":{"outs":{"type":"array","items":{
    "type":"object","properties":{"kind":{"enum":["file","http"]},"path":{"type":"string"},
        "url":{"type":"string"},"retries":{"type":"integer"}},
    "if":{"properties":{"kind":{"const":"http"}},"required":["kind"]},
    "then":{"required":["url"],"properties":{"path":false}},
    "else":{"required":["path"],"properties":{"url":false}}}}}}"#;

/// An item's state: the `kind` written before the caret (if any), the
/// text typed at the caret, and the keys offered under each schema.
struct State {
    kind: Option<&'static str>,
    typed: &'static str,
    one_of: &'static [&'static str],
    if_then: &'static [&'static str],
}

const STATES: &[State] = &[
    State {
        kind: None,
        typed: "",
        one_of: &["kind", "mode", "path", "timeout", "url"],
        if_then: &["kind", "path", "retries"],
    },
    State {
        kind: Some("file"),
        typed: "",
        one_of: &["mode", "path"],
        if_then: &["path", "retries"],
    },
    State {
        kind: Some("http"),
        typed: "",
        one_of: &["timeout", "url"],
        if_then: &["retries", "url"],
    },
    State {
        kind: Some("file"),
        typed: "pa",
        one_of: &["path"],
        if_then: &["path"],
    },
    State {
        kind: Some("http"),
        typed: "ur",
        one_of: &["url"],
        if_then: &["url"],
    },
];

/// The kind of the items around the caret's: the other one, so an item
/// sliding into the caret's slot changes what is offered.
fn other(kind: Option<&str>) -> (&'static str, &'static str) {
    match kind {
        Some("http") => ("file", "path"),
        _ => ("http", "url"),
    }
}

/// How each language writes a list item on one line.
struct Writer {
    language: Language,
    /// The list `outs` holding the items (one per line), `@` the caret's.
    frame: fn(&[String]) -> String,
    /// An item holding `entries` (key, value) on one line.
    item: fn(&[(&str, &str)]) -> String,
    /// The item holding `entries`, then the caret with `typed` before it.
    caret_item: fn(&[(&str, &str)], &str) -> String,
}

fn flow(entries: &[(&str, &str)], pair: fn(&str, &str) -> String) -> Vec<String> {
    entries
        .iter()
        .map(|(key, value)| pair(key, value))
        .collect()
}

fn json_item(entries: &[(&str, &str)]) -> String {
    format!(
        "{{{}}}",
        flow(entries, |k, v| format!("\"{k}\": \"{v}\"")).join(", ")
    )
}

fn json_caret(entries: &[(&str, &str)], typed: &str) -> String {
    let mut parts = flow(entries, |k, v| format!("\"{k}\": \"{v}\""));
    if !(typed.is_empty() && entries.is_empty()) {
        parts.push(format!("\"{typed}|\""));
    } else {
        parts.push("|".into());
    }
    format!("{{{}}}", parts.join(", "))
}

fn toml_item(entries: &[(&str, &str)]) -> String {
    format!(
        "{{ {} }}",
        flow(entries, |k, v| format!("{k} = \"{v}\"")).join(", ")
    )
}

fn toml_caret(entries: &[(&str, &str)], typed: &str) -> String {
    let mut parts = flow(entries, |k, v| format!("{k} = \"{v}\""));
    parts.push(format!("{typed}|"));
    format!("{{ {} }}", parts.join(", "))
}

fn yaml_item(entries: &[(&str, &str)]) -> String {
    format!(
        "{{{}}}",
        flow(entries, |k, v| format!("{k}: {v}")).join(", ")
    )
}

fn yaml_caret(entries: &[(&str, &str)], typed: &str) -> String {
    let mut parts = flow(entries, |k, v| format!("{k}: {v}"));
    parts.push(format!("{typed}|"));
    format!("{{{}}}", parts.join(", "))
}

fn xml_item(entries: &[(&str, &str)]) -> String {
    format!(
        "<outs>{}</outs>",
        flow(entries, |k, v| format!("<{k}>{v}</{k}>")).concat()
    )
}

fn xml_caret(entries: &[(&str, &str)], typed: &str) -> String {
    let opened = if typed.is_empty() {
        String::new()
    } else {
        format!("<{typed}")
    };
    format!(
        "<outs>{}{opened}|</outs>",
        flow(entries, |k, v| format!("<{k}>{v}</{k}>")).concat()
    )
}

const WRITERS: &[Writer] = &[
    Writer {
        language: Language::JSON,
        frame: |items| format!("{{\"outs\": [\n  {}\n]}}\n", items.join(",\n  ")),
        item: json_item,
        caret_item: json_caret,
    },
    Writer {
        language: Language::TOML,
        frame: |items| format!("outs = [\n  {},\n]\n", items.join(",\n  ")),
        item: toml_item,
        caret_item: toml_caret,
    },
    Writer {
        language: Language::HCL,
        frame: |items| format!("outs = [\n  {},\n]\n", items.join(",\n  ")),
        item: toml_item,
        caret_item: toml_caret,
    },
    Writer {
        language: Language::YAML,
        frame: |items| format!("outs:\n  - {}\n", items.join("\n  - ")),
        item: yaml_item,
        caret_item: yaml_caret,
    },
    Writer {
        language: Language::YAML,
        frame: |items| format!("outs: [\n  {}\n]\n", items.join(",\n  ")),
        item: yaml_item,
        caret_item: yaml_caret,
    },
    Writer {
        language: Language::XML,
        frame: |items| format!("{}\n", items.join("\n")),
        item: xml_item,
        caret_item: xml_caret,
    },
];

/// Where the caret's item sits among two others.
const PLACES: &[(&str, usize)] = &[("before", 0), ("between", 1), ("after", 2)];

fn offered(schema: &str, source: &str, at: usize, language: Language) -> Vec<String> {
    let bundle = build_schema(schema).unwrap();
    let mut labels: Vec<String> = completions(
        source,
        source[..at].encode_utf16().count(),
        true,
        Some(&bundle),
        true,
        language.behavior(),
    )
    .into_iter()
    .map(|option| option.label)
    .collect();
    labels.sort();
    labels.dedup();
    labels
}

fn check(name: &str, marked: &str, language: Language, state: &State, failures: &mut Vec<String>) {
    let at = marked.find('|').unwrap();
    let source = marked.replacen('|', "", 1);
    for (schema_name, schema, wanted) in [
        ("oneOf", ONE_OF, state.one_of),
        ("if", IF_THEN, state.if_then),
    ] {
        let labels = offered(schema, &source, at, language);
        if labels != wanted {
            failures.push(format!(
                "{name} {schema_name} {marked:?}: offered {labels:?}, wanted {wanted:?}"
            ));
        }
    }
}

#[test]
fn a_one_line_item_is_its_own_in_every_place() {
    let mut failures = Vec::new();
    for writer in WRITERS {
        for state in STATES {
            let (other_kind, other_key) = other(state.kind);
            let neighbour = (writer.item)(&[("kind", other_kind), (other_key, "x")]);
            let entries: Vec<(&str, &str)> =
                state.kind.map(|kind| ("kind", kind)).into_iter().collect();
            let caret = (writer.caret_item)(&entries, state.typed);
            for (place, index) in PLACES {
                let mut items = vec![neighbour.clone(), neighbour.clone()];
                items.insert(*index, caret.clone());
                let marked = (writer.frame)(&items);
                check(
                    &format!("{} {place}", writer.language),
                    &marked,
                    writer.language,
                    state,
                    &mut failures,
                );
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The JSON view of each host: a one-line item added above the first.
#[test]
fn a_one_line_item_in_a_json_view_is_its_own() {
    let value = serde_json::json!({"outs": [
        {"kind": "http", "url": "a"}, {"kind": "http", "url": "b"}
    ]});
    let mut failures = Vec::new();
    for host in [Language::YAML, Language::TOML, Language::HCL, Language::XML] {
        let Some(bridge) = Bridge::new(host, Language::JSON, None) else {
            continue;
        };
        let host_text = host.serialize(&value).unwrap();
        let shown = bridge.to_edit(&host_text).expect("shows").text;
        let Some(first) = shown
            .find("{\n")
            .and_then(|root| shown[root + 1..].find('{').map(|at| root + 1 + at))
        else {
            failures.push(format!("{host}: shown {shown:?}"));
            continue;
        };
        let line_start = shown[..first].rfind('\n').map_or(0, |at| at + 1);
        let pad = &shown[line_start..first];
        for state in STATES.iter().filter(|state| state.kind != Some("http")) {
            let entries: Vec<(&str, &str)> =
                state.kind.map(|kind| ("kind", kind)).into_iter().collect();
            let marked = format!(
                "{}{pad}{},\n{}",
                &shown[..line_start],
                json_caret(&entries, state.typed),
                &shown[line_start..]
            );
            check(
                &format!("{host} as json"),
                &marked,
                Language::JSON,
                state,
                &mut failures,
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
