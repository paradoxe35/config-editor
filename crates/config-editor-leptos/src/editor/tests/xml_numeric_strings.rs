//! XML holds text: an untyped `const` or `enum` of strings that look like
//! numbers (`"1.3"`) reads its element as the string XML holds, so the
//! value matches, a suggestion writes the const, and an Edit-as save of
//! an XML host keeps it (not stuck on a value it would read otherwise).

use std::sync::Arc;

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::completion::accept_marked;
use crate::editor::diagnostics::{Schema, build_schema};

const SCHEMA: &str = r#"{"type":"object","properties":{
    "c1":{"type":"object","required":["k","v"],"properties":{"k":{"type":"string"},"v":{"const":"1.3"}}},
    "c5":{"type":"object","required":["k","v"],"properties":{"k":{"type":"string"},"v":{"enum":["1.3","2","true"]}}}
}}"#;

#[test]
fn an_untyped_const_or_enum_of_strings_matches_the_xml_text() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let problems = |xml: &str| -> Vec<String> {
        schema
            .validate(Language::XML, xml)
            .into_iter()
            .map(|problem| problem.message)
            .collect()
    };
    let mut failures = Vec::new();
    for xml in [
        "<c1><k>x</k><v>1.3</v></c1>",
        "<c5><k>x</k><v>1.3</v></c5>",
        "<c5><k>x</k><v>2</v></c5>",
        "<c5><k>x</k><v>true</v></c5>",
    ] {
        let found = problems(xml);
        if !found.is_empty() {
            failures.push(format!("{xml}: {found:?}"));
        }
    }
    // A value not listed is the text it is, too.
    let found = problems("<c5><k>x</k><v>3</v></c5>");
    if found != [r#"/c5/v: "3" is not one of "1.3", "2", "true""#] {
        failures.push(format!("3: {found:?}"));
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn accepting_the_parent_writes_the_const() {
    let bundle = build_schema(SCHEMA).unwrap();
    let accepted = accept_marked("c|", "c1", &bundle, Language::XML.behavior());
    assert!(accepted.contains("<v>1.3</v>"), "{accepted}");
}

#[test]
fn an_xml_host_saves_from_every_view() {
    let bundle = Arc::new(build_schema(SCHEMA).unwrap());
    let host = "<c1>\n  <k>x</k>\n  <v>1.3</v>\n</c1>\n";
    let mut failures = Vec::new();
    for view in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
    ] {
        let bridge = Bridge::new(Language::XML, view, Some(bundle.clone())).unwrap();
        let shown = bridge.to_edit(host).unwrap().text;
        // The const is a string in the view.
        let read = view.parse(&shown).unwrap();
        if read.pointer("/c1/v") != Some(&"1.3".into()) {
            failures.push(format!("{view} shows {shown}"));
            continue;
        }
        let edited = shown.replacen('x', "y", 1);
        match bridge.to_host(&edited, host) {
            Ok(saved) if saved == host.replace("<k>x</k>", "<k>y</k>") => {}
            other => failures.push(format!("{view}: {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
