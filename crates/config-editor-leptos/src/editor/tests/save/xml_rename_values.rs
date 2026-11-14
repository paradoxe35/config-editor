//! An element renamed while its values change in the same edit (a Replace
//! All of `server` with `backend` over the key and the values holding it,
//! a paste over a key and its value) is the element it was: its attributes
//! stay attributes and it keeps its place (QA `E33-2`: they were saved as
//! child elements, and from a TOML view it moved after its siblings).

use crate::editor::Language;
use crate::editor::save_matrix::{from_every_view, report};
use serde_json::Value;

/// `value` with `from` replaced by `to` in every key and string, each key
/// in its place.
fn replace_all(value: &mut Value, from: &str, to: &str) {
    match value {
        Value::Object(map) => {
            *map = std::mem::take(map)
                .into_iter()
                .map(|(key, mut child)| {
                    replace_all(&mut child, from, to);
                    (key.replace(from, to), child)
                })
                .collect();
        }
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| replace_all(item, from, to)),
        Value::String(text) => *text = text.replace(from, to),
        _ => {}
    }
}

const XSI: &str = "xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";

#[test]
fn an_element_renamed_with_its_values_changed_keeps_its_attributes_and_place() {
    let mut failures = Vec::new();
    let nlog = |root: &str, file: &str, log: &str, console: &str| {
        format!(
            "<{root} xmlns=\"urn:nlog\" {XSI}>\n  <targets>\n    <target xsi:type=\"File\" name=\"{file}\" fileName=\"app.{log}\" />\n    <target xsi:type=\"Console\" name=\"{console}\" />\n  </targets>\n</{root}>\n"
        )
    };
    let cases: [(&str, String, &str, &str, String); 4] = [
        (
            "a lone element",
            "<config>\n  <server name=\"main-server\" port=\"80\"/>\n  <timeout>5</timeout>\n</config>\n".into(),
            "server",
            "backend",
            "<config>\n  <backend name=\"main-backend\" port=\"80\"/>\n  <timeout>5</timeout>\n</config>\n".into(),
        ),
        (
            "a list and its parent",
            "<config>\n  <servers>\n    <server name=\"server1\" port=\"80\"/>\n    <server name=\"server2\" port=\"81\"/>\n  </servers>\n  <timeout>5</timeout>\n</config>\n".into(),
            "server",
            "backend",
            "<config>\n  <backends>\n    <backend name=\"backend1\" port=\"80\"/>\n    <backend name=\"backend2\" port=\"81\"/>\n  </backends>\n  <timeout>5</timeout>\n</config>\n".into(),
        ),
        (
            "an element holding children and attributes",
            "<config>\n  <server name=\"main-server\">\n    <host>h</host>\n  </server>\n  <timeout>5</timeout>\n</config>\n".into(),
            "server",
            "srv",
            "<config>\n  <srv name=\"main-srv\">\n    <host>h</host>\n  </srv>\n  <timeout>5</timeout>\n</config>\n".into(),
        ),
        (
            "the root, items with markup",
            nlog("nlog", "logfile", "log", "console"),
            "o",
            "0",
            nlog("nl0g", "l0gfile", "l0g", "c0ns0le"),
        ),
    ];
    for (label, host, from, to, expected) in &cases {
        let change = |value: &mut Value| replace_all(value, from, to);
        from_every_view(&mut failures, label, Language::XML, host, &change, expected);
    }
    report(failures);
}
