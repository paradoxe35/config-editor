//! An HCL expression edited in another view into text that reads as no
//! expression waits, saying so (QA round 37: it said the value would be
//! written "in another form … though it reads the same"), in place or
//! after the attribute moved.

use crate::editor::Language;
use crate::editor::save_matrix::{ALL, report, save_steps, slot};
use serde_json::{Value, json};

const HOST: &str = "locals {\n  name = \"app\"\n  tags = merge(var.tags, { Name = \"x\" })\n}\n\nresource \"r\" \"w\" {\n  ami = \"ami-1\"\n}\n";

const BROKEN: &str = "${merge(var.tags, { Name = \"x\" }Z)}";

fn break_in_place(value: &mut Value) {
    *slot(value, "/locals/tags") = json!(BROKEN);
}

fn move_into_resource(value: &mut Value) {
    let tags = value["locals"]
        .as_object_mut()
        .unwrap()
        .shift_remove("tags")
        .unwrap();
    slot(value, "/resource/r/w")
        .as_object_mut()
        .unwrap()
        .shift_insert(0, "tags".to_string(), tags);
}

fn break_moved(value: &mut Value) {
    *slot(value, "/resource/r/w/tags") = json!(BROKEN);
}

#[test]
fn an_expression_edited_into_none_waits_saying_so() {
    type Steps = &'static [fn(&mut Value)];
    let cases: [(&str, Steps); 2] = [
        ("in place", &[break_in_place]),
        ("after a move", &[move_into_resource, break_moved]),
    ];
    let mut failures = Vec::new();
    for view in ALL.into_iter().filter(|view| *view != Language::HCL) {
        for (label, steps) in cases {
            let steps: Vec<&dyn Fn(&mut Value)> = steps
                .iter()
                .map(|step| step as &dyn Fn(&mut Value))
                .collect();
            match save_steps(Language::HCL, view, HOST, &steps) {
                Err(error) if error.contains("no longer reads as an HCL expression") => {}
                other => failures.push(format!("{label} via {view}: {other:?}")),
            }
        }
    }
    report(failures);
}
