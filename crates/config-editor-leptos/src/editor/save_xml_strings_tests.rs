//! M23-E1: strings an XML host would read back as other values (`""` as
//! an empty element, `"null"`, `"true"`, `"1"`): from every view the save
//! waits with the reason; strings XML keeps are saved.

use super::Language;
use super::save_matrix::{ALL, save_value};
use serde_json::json;

const HOST: &str = "<r k=\"v\">\n  <s>abc</s> <!-- c -->\n  <n>1</n>\n</r>\n";

#[test]
fn strings_xml_reads_as_other_values_wait_from_every_view() {
    let mut failures = Vec::new();
    for edit in ALL.into_iter().filter(|edit| *edit != Language::XML) {
        for (pointer, string) in [
            ("/r/s", ""),
            ("/r/s", "null"),
            ("/r/s", "true"),
            ("/r/s", " false "),
            ("/r/s", "12"),
            ("/r/k", "true"),
            ("/r/k", "7"),
        ] {
            let change = |value: &mut serde_json::Value| {
                *value.pointer_mut(pointer).unwrap() = json!(string);
            };
            match save_value(Language::XML, edit, HOST, &change) {
                Err(error) if error.contains(&format!("(at {pointer})")) => {}
                other => failures.push(format!("{edit} view, {pointer} = {string:?}: {other:?}")),
            }
        }
        // What XML reads back as the same string is saved.
        for (pointer, string, expected) in [
            ("/r/s", "1.50", "<s>1.50</s>"),
            ("/r/s", "true!", "<s>true!</s>"),
            ("/r/k", "", "k=\"\""),
        ] {
            let change = |value: &mut serde_json::Value| {
                *value.pointer_mut(pointer).unwrap() = json!(string);
            };
            match save_value(Language::XML, edit, HOST, &change) {
                Ok(saved) if saved.contains(expected) => {}
                other => failures.push(format!("{edit} view, {pointer} = {string:?}: {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
