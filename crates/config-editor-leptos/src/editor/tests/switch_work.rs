//! The work an Edit-as switch costs on a large document (the demo's
//! 2,000-line samples, under the tunnel schema), and the switch back: the
//! texts the switch reads are read once, and its time grows with the
//! document no faster than the document (P5). Every host and view.

use std::sync::Arc;

use leptos::prelude::*;
use serde_json::Value;

use crate::editor::languages::Language;
use crate::editor::schema::{SchemaBundle, build_schema};
use crate::editor::state::EditorState;
use crate::editor::sync::switch_to;
use crate::editor::test_state::TestEditor;
use crate::editor::work_count::{Work, counted};

/// The sample written in `language`.
fn sample(language: Language) -> &'static str {
    match language.label() {
        "YAML" => include_str!("../../../tests/fixtures/samples/tunnel.large.yaml"),
        "JSON" => include_str!("../../../tests/fixtures/samples/tunnel.large.json"),
        "TOML" => include_str!("../../../tests/fixtures/samples/tunnel.large.toml"),
        "HCL" => include_str!("../../../tests/fixtures/samples/tunnel.large.hcl"),
        "XML" => include_str!("../../../tests/fixtures/samples/tunnel.large.xml"),
        other => panic!("no sample for {other}"),
    }
}

fn schema() -> Arc<SchemaBundle> {
    Arc::new(build_schema(include_str!("../../../tests/fixtures/tunnel.schema.json")).unwrap())
}

/// An editor of `text` in `host` with the schema, its problems read as the
/// mounted view reads them.
fn mounted(host: Language, text: &str, schema: &Arc<SchemaBundle>) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(host, text);
    editor.schema = Some(Arc::clone(schema));
    let mounted = editor.mount();
    read_problems(mounted.1);
    mounted
}

/// What the view reads after a switch: the problems of the text on screen.
fn read_problems(state: EditorState) {
    state.problems().own.with_untracked(|_| ());
}

/// Switches to `next`, as the view sees it.
fn switch(state: EditorState, next: Language) {
    switch_to(state, next);
    read_problems(state);
}

/// Every host with every view.
fn pairs() -> impl Iterator<Item = (Language, Language)> {
    Language::ALL.iter().flat_map(|&host| {
        Language::ALL
            .iter()
            .filter(move |&&view| view != host)
            .map(move |&view| (host, view))
    })
}

/// A switch parses only the text it puts on screen, and the switch back
/// parses nothing: the host's text and the view's stay among the shared
/// parses (`parse_cache`) through the switch's own checks (a check that a
/// language can write, reading a small text of its own, pushed them out).
#[test]
fn a_switch_parses_only_the_text_it_shows_and_back_parses_nothing() {
    // As a mounted editor does (`sync::connect`).
    crate::editor::parse_cache::hold();
    let schema = schema();
    let mut failures = Vec::new();
    for (host, view) in pairs() {
        let text = sample(host);
        let (_owner, state) = mounted(host, text, &schema);
        let ((), parsed) = counted(Work::ParsedBytes, || switch(state, view));
        let shown = state.doc().text.get_untracked().len();
        if parsed != shown {
            failures.push(format!(
                "{host} as {view}: {parsed} bytes parsed, the view is {shown}"
            ));
        }
        let ((), parsed) = counted(Work::ParsedBytes, || switch(state, host));
        if parsed != 0 {
            failures.push(format!("{host} as {view}, back: {parsed} bytes parsed"));
        }
    }
    crate::editor::parse_cache::release();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The problems of the text a switch shows look for the host's secret
/// placeholders written in it in one pass over it, not one pass for each
/// placeholder (the samples write about 150).
#[cfg(feature = "protect")]
#[test]
fn a_switch_searches_its_text_for_the_placeholders_once() {
    let schema = schema();
    let mut failures = Vec::new();
    for (host, view) in pairs() {
        let (_owner, state) = mounted(host, sample(host), &schema);
        if untrack(|| state.protect().accepted_tokens().len()) < 100 {
            failures.push(format!("{host}: the sample's placeholders are not read"));
        }
        for next in [view, host] {
            let ((), searches) = counted(Work::TokenSearch, || switch(state, next));
            if searches > 1 {
                failures.push(format!(
                    "{host} as {view}, into {next}: {searches} searches"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The sample's document with its lists of clients, services, visitors
/// and sites each `copies` times over, the copies named apart (and their
/// placeholders told apart, each a placeholder of its own).
fn document(copies: usize) -> Value {
    let mut value: Value = serde_json::from_str(sample(Language::JSON)).unwrap();
    for name in ["clients", "services", "visitors", "web"] {
        let Some(Value::Object(entries)) = value.get_mut(name) else {
            panic!("the sample has no {name}");
        };
        let originals = std::mem::take(entries);
        for copy in 0..copies {
            for (key, entry) in &originals {
                let (key, entry) = if copy == 0 {
                    (key.clone(), entry.clone())
                } else {
                    let written = serde_json::to_string(entry)
                        .unwrap()
                        .replace("«secret ", &format!("«secret {copy:x}"));
                    (
                        format!("{key}-{copy}"),
                        serde_json::from_str(&written).unwrap(),
                    )
                };
                entries.insert(key, entry);
            }
        }
    }
    value
}

/// Timed: a switch and the switch back take time that grows with the
/// document no faster than the document. Four times the entries take
/// about four times as long; a read of the document for each entry would
/// take sixteen times.
#[test]
fn a_switch_and_back_take_time_linear_in_the_document() {
    crate::editor::parse_cache::hold();
    let schema = schema();
    let (small, large) = (document(1), document(4));
    let mut failures = Vec::new();
    for (host, view) in pairs() {
        let texts = [&small, &large].map(|value| host.serialize(value).unwrap());
        let editors = texts
            .each_ref()
            .map(|text| mounted(host, text.as_str(), &schema));
        let growth = crate::editor::test_timing::growth(3, (&texts[0], &texts[1]), |text| {
            let state = editors[usize::from(*text != texts[0])].1;
            switch(state, view);
            switch(state, host);
        });
        if growth.ratio() > 10.0 {
            failures.push(format!("{host} as {view} and back: {growth}"));
        }
    }
    crate::editor::parse_cache::release();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
