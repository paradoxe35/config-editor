//! The work a typing pause's Edit-as save costs on a large document, counted
//! (see `work_count`): the host's texts are each read the way the schema
//! expects once, however many of the save's steps and checks ask (QA
//! `P30-1`: the last check of every save, that the host's text reads back
//! as the view holds it, read the saved text again, and an XML host's
//! check that it still converts built its element tree twice more; a
//! 5,000-line HCL or XML host paused 1.4 to 1.7 times as long).

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::diagnostics::build_schema;
use crate::editor::save_matrix::ALL;
use crate::editor::work_count::{Work, counted};

/// The sample in `language`.
fn sample(language: Language) -> &'static str {
    [
        (
            Language::YAML,
            include_str!("../../../../tests/fixtures/samples/tunnel.large.yaml"),
        ),
        (
            Language::JSON,
            include_str!("../../../../tests/fixtures/samples/tunnel.large.json"),
        ),
        (
            Language::TOML,
            include_str!("../../../../tests/fixtures/samples/tunnel.large.toml"),
        ),
        (
            Language::HCL,
            include_str!("../../../../tests/fixtures/samples/tunnel.large.hcl"),
        ),
        (
            Language::XML,
            include_str!("../../../../tests/fixtures/samples/tunnel.large.xml"),
        ),
    ]
    .into_iter()
    .find(|(known, _)| *known == language)
    .map(|(_, text)| text)
    .unwrap()
}

/// `shown` with the middle one of its `udp` values typed on (`udpx`): a
/// value, never a key (`udp {`, `"udp":`, `udp:`).
fn typed_on(shown: &str) -> String {
    let values: Vec<usize> = shown
        .match_indices("udp")
        .map(|(at, _)| at + "udp".len())
        .filter(|&end| {
            let after = &shown[end..];
            ["\"\n", "\",", "\"\r", "<", "\n"]
                .iter()
                .any(|next| after.starts_with(next))
        })
        .collect();
    let at = values[values.len() / 2];
    format!("{}x{}", &shown[..at], &shown[at..])
}

/// What one save of a value typed in each view of every host costs, by
/// the host's length: (whole-text parses, whole texts read as the schema
/// expects).
fn costs() -> Vec<(String, f64, f64)> {
    let schema = std::sync::Arc::new(
        build_schema(include_str!(
            "../../../../tests/fixtures/tunnel.schema.json"
        ))
        .unwrap(),
    );
    let mut costs = Vec::new();
    for host in ALL {
        let host_text = sample(host);
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            // An editor mounted on the host's text (`sync::connect`): its
            // view shown, nothing else read yet.
            crate::editor::parse_cache::hold();
            let bridge = Bridge::new(host, edit, Some(schema.clone())).unwrap();
            let shown = bridge.to_edit(host_text).unwrap().text;
            let typed = typed_on(&shown);
            let ((saved, conformed), parsed) = counted(Work::ParsedBytes, || {
                counted(Work::ConformedBytes, || {
                    bridge.to_host_since(&typed, host_text, host_text)
                })
            });
            let label = format!("{} as {}", host.label(), edit.label());
            assert!(saved.is_ok_and(|saved| saved.contains("udpx")), "{label}");
            let per = |count: usize| count as f64 / host_text.len() as f64;
            costs.push((label, per(parsed), per(conformed)));
            crate::editor::parse_cache::release();
        }
    }
    costs
}

/// A save reads the host's text written whole (to read its values back)
/// and the one it saves, each once as the schema expects, however many
/// patches and checks read them; between HCL and XML the view's texts as
/// well. Each patch that reads its text back, and the last check reading
/// the saved text back, take that one reading.
#[test]
fn a_pause_s_save_reads_each_text_once() {
    let costs = costs();
    // In host lengths: the two host texts, and up to one and a half more
    // for an XML view's texts, which are longer than HCL's.
    let over: Vec<_> = costs
        .iter()
        .filter(|(_, _, conformed)| *conformed > 4.0)
        .collect();
    assert!(over.is_empty(), "read as the schema expects: {over:#?}");
    // Parsed besides: the edit's texts, and no host text a second time (an
    // XML host's check that the saved text still converts builds no value
    // of it either).
    let over: Vec<_> = costs
        .iter()
        .filter(|(label, parsed, _)| {
            (label.starts_with("HCL") || label.starts_with("XML")) && *parsed > 4.0
        })
        .collect();
    assert!(over.is_empty(), "parsed: {over:#?}");
}

/// How many values `value` holds, itself included.
fn values_in(value: &serde_json::Value) -> usize {
    1 + match value {
        serde_json::Value::Object(map) => map.values().map(values_in).sum(),
        serde_json::Value::Array(items) => items.iter().map(values_in).sum(),
        _ => 0,
    }
}

/// Reading a document the way the schema expects resolves the schema of
/// each of its values once, not again for each child it holds (which made
/// each such reading, and so mounting an HCL or XML view, cost about twice
/// as much once a schema's branches took conditionals and dependent
/// schemas too).
#[test]
fn reading_a_text_as_the_schema_expects_resolves_each_value_s_schema_once() {
    let bundle = build_schema(include_str!(
        "../../../../tests/fixtures/tunnel.schema.json"
    ))
    .unwrap();
    for language in ALL {
        let repetition = language.behavior().repetition();
        if repetition == crate::editor::language::Repetition::None {
            continue;
        }
        let mut value = language.parse(sample(language)).unwrap();
        let values = values_in(&value);
        let ((), walks) = counted(Work::BranchWalk, || {
            crate::editor::completion::conform_arrays(&mut value, &bundle.schema, repetition);
        });
        assert!(
            walks <= values,
            "{}: {walks} walks for {values} values",
            language.label()
        );
    }
}
