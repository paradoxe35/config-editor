//! Objects of many entries written entry by entry are written as the
//! formatter writes them whole, in time growing with their entries.

use super::HclBehavior;
use super::convert::{HostForms, hcl_body, host_forms};
use crate::editor::language::ConfigLanguage;
use crate::editor::test_timing::best_of;
use serde_json::{Value, json};

/// An object of `entries` entries, each `entry(index)`.
fn many(entries: usize, entry: impl Fn(usize) -> Value) -> Value {
    Value::Object((0..entries).map(|index| (format!("k{index}"), entry(index))).collect())
}

fn values() -> Vec<Value> {
    vec![
        json!({ "hosts": many(300, |index| json!(format!("10.0.0.{index}"))) }),
        json!({ "vars": many(200, |index| json!({ "a": index, "b": [index, "x"], "c": { "d": null } })) }),
        json!({ "nested": many(150, |index| if index == 7 { many(400, |inner| json!(inner)) } else { json!(index) }) }),
        json!({ "list": [many(200, |index| json!(index)), many(200, |index| json!(format!("${{var.x}}-{index}")))] }),
        json!({ "keys": Value::Object((0..300).map(|index| (format!("key-{index} with space"), json!(index))).collect()) }),
        json!({ "text": many(200, |index| json!(format!("line one\nline {index}\t\"quoted\""))) }),
        json!({ "b": { "inner": many(300, |index| json!(index)) }, "c": many(130, |index| json!(index == 0)) }),
        json!({ "clash": many(200, |index| json!(format!("config_editor_object_{}_", index % 3))) }),
    ]
}

fn written(value: &Value, forms: &HostForms, entry_by_entry: bool) -> String {
    let map = value.as_object().expect("a body");
    let render = || hcl::format::to_string(&hcl_body(map, forms, &[])?).map_err(|error| error.to_string());
    if entry_by_entry { super::splice::writing(render) } else { render() }.expect("it writes")
}

/// `value`'s maps written as attributes (`name = { … }`), not blocks: a
/// host writing them so has them written as object expressions.
fn as_attributes(value: &Value) -> String {
    let map = value.as_object().expect("a body");
    map.iter().map(|(key, child)| format!("{key} = {}\n", hcl::format::to_string(&super::convert::hcl_expression(child)).expect("it writes"))).collect()
}

#[test]
fn objects_written_entry_by_entry_are_written_as_whole_ones() {
    for value in values() {
        // As blocks, as attributes, and with templates and bare
        // expressions written so again (the other writer).
        let host = as_attributes(&value);
        let templates = format!("{host}t = \"${{var.x}}-0\"\ne = var.y\n");
        for forms in [HostForms::default(), host_forms(&host), host_forms(&templates)] {
            let whole = written(&value, &forms, false);
            assert_eq!(written(&value, &forms, true), whole, "{value:.200}");
        }
    }
}

/// Writing a document of a map of `entries` entries back over its own
/// text (an edit made in another view) takes time in step with its length:
/// eight times the entries about eight times as long, not sixty-four.
#[test]
fn writing_grows_with_the_length_of_the_map() {
    let time = |entries: usize| {
        let value = json!({ "hosts": many(entries, |index| json!(format!("10.0.{}.{}", index >> 8, index & 255))) });
        let text = as_attributes(&value);
        best_of(3, || HclBehavior.serialize(&value, Some(&text)).expect("it writes")).0
    };
    let (small, large) = (time(2_000), time(16_000));
    let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-6);
    assert!(ratio < 20.0, "2000 entries {small:?}, 16000 entries {large:?} ({ratio:.1}x)");
}
