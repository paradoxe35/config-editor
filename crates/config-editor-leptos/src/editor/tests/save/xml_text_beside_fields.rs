//! A new XML key holding text beside other fields (`dimen: {name: pad,
//! "#text": 4dp}` typed in a view): the only XML that reads so is the
//! fields as attributes of the element holding the text
//! (`<dimen name="pad">4dp</dimen>`), which a save writes, at any depth,
//! as it writes an item added to a list of such elements; the text beside
//! child elements no view could show again never is.

use crate::editor::Language;
use crate::editor::save_matrix::{from_every_view, report, slot};
use serde_json::{Value, json};

const HOST: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<resources>\n  <string name=\"app_name\">My App</string>\n  <color name=\"red\">#f00</color>\n</resources>\n";

#[test]
fn fields_beside_a_new_keys_text_are_its_attributes() {
    let mut failures = Vec::new();
    let cases: [(&str, &str, Value, &str); 3] = [
        (
            "a new key",
            "/resources",
            json!({"dimen": {"name": "pad", "#text": "4dp"}}),
            "  <dimen name=\"pad\">4dp</dimen>\n",
        ),
        (
            "two fields, text first",
            "/resources",
            json!({"dimen": {"#text": "4dp", "name": "pad", "unit": "dp"}}),
            "  <dimen name=\"pad\" unit=\"dp\">4dp</dimen>\n",
        ),
        (
            "deeper",
            "/resources",
            json!({"style": {"item": {"name": "x", "#text": "1"}}}),
            "  <style>\n    <item name=\"x\">1</item>\n  </style>\n",
        ),
    ];
    for (label, pointer, added, written) in cases {
        let change = |value: &mut Value| {
            if let (Some(map), Value::Object(added)) =
                (slot(value, pointer).as_object_mut(), &added)
            {
                map.extend(added.clone());
            }
        };
        let expected = HOST.replace("</resources>", &format!("{written}</resources>"));
        from_every_view(
            &mut failures,
            label,
            Language::XML,
            HOST,
            &change,
            &expected,
        );
    }
    report(failures);
}

/// A field added beside an element's text in the host: an attribute too.
#[test]
fn a_field_added_beside_a_host_elements_text_is_an_attribute() {
    let mut failures = Vec::new();
    let change = |value: &mut Value| {
        if let Some(map) = slot(value, "/resources/color").as_object_mut() {
            map.insert("alpha".into(), json!(1));
        }
    };
    let expected = HOST.replace("<color name=\"red\">", "<color name=\"red\" alpha=\"1\">");
    from_every_view(
        &mut failures,
        "a field beside text",
        Language::XML,
        HOST,
        &change,
        &expected,
    );
    report(failures);
}
