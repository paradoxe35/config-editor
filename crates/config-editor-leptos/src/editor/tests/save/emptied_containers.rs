//! A map's or list's last entry deleted in a YAML view leaves `deps:`
//! with nothing under it, which reads as null: the save keeps the empty
//! map or list the host held, in every host (TOML and XML waited on "no
//! null value", JSON and HCL saved a null for it), and a null the user
//! writes stays one where the host can hold it.

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::save_matrix::report;
use serde_json::{Value, json};

/// `host_text` saved after each line of its YAML view holding the first
/// text of one of `edits` is replaced by its second (removed when empty).
fn saved_after(host: Language, host_text: &str, edits: &[(&str, &str)]) -> Result<String, String> {
    let bridge = Bridge::new(host, Language::YAML, None).ok_or("same language")?;
    let shown = bridge
        .to_edit(host_text)
        .map_err(|error| error.to_string())?
        .text;
    let edited: String = shown
        .split_inclusive('\n')
        .map(
            |shown_line| match edits.iter().find(|(line, _)| shown_line.contains(line)) {
                Some((_, with)) => with.to_string(),
                None => shown_line.to_string(),
            },
        )
        .collect();
    bridge
        .to_host(&edited, host_text)
        .map_err(|error| format!("{error:?}"))
}

/// Each host with a map `deps` holding one entry `x`, and the pointer of
/// `deps` in it.
const HOSTS: [(Language, &str, &str); 4] = [
    (
        Language::JSON,
        "{\n  \"a\": 1,\n  \"deps\": {\n    \"x\": 1\n  }\n}\n",
        "/deps",
    ),
    (Language::TOML, "a = 1\n\n[deps]\nx = 1\n", "/deps"),
    (Language::HCL, "a = 1\n\ndeps {\n  x = 1\n}\n", "/deps"),
    (
        Language::XML,
        "<r>\n  <a>1</a>\n  <deps>\n    <x>1</x>\n  </deps>\n</r>\n",
        "/r/deps",
    ),
];

#[test]
fn a_map_whose_last_entry_is_deleted_stays_an_empty_map() {
    let mut failures = Vec::new();
    for (host, text, pointer) in HOSTS {
        match saved_after(host, text, &[("x: 1", "")]) {
            Ok(saved) => {
                let read = host
                    .parse(&saved)
                    .map(|value| value.pointer(pointer).cloned());
                if !matches!(&read, Ok(Some(map)) if *map == json!({})) {
                    failures.push(format!("{host}: {read:?}\n{saved}"));
                }
            }
            Err(error) => failures.push(format!("{host}: {error}")),
        }
    }
    report(failures);
}

/// A null typed over the map stays a null where the host holds one, and
/// waits where it cannot.
#[test]
fn a_null_written_over_a_map_stays_null() {
    for written in ["null", "~"] {
        let line = format!("deps: {written}\n");
        let edits = [("deps:", line.as_str()), ("x: 1", "")];
        let saved = saved_after(Language::JSON, HOSTS[0].1, &edits);
        let read = saved
            .as_deref()
            .ok()
            .and_then(|saved| Language::JSON.parse(saved).ok());
        assert_eq!(
            read.as_ref().and_then(|value| value.pointer("/deps")),
            Some(&Value::Null),
            "{written}: {saved:?}"
        );
        let toml = saved_after(Language::TOML, HOSTS[1].1, &edits);
        assert!(
            toml.as_ref().is_err_and(|error| error.contains("no null")),
            "{written}: {toml:?}"
        );
    }
}

/// Each host with a list `arr` holding one item `1`, the line of the item
/// in the YAML view, and the pointer of `arr`.
const LIST_HOSTS: [(Language, &str, &str, &str); 3] = [
    (
        Language::JSON,
        "{\n  \"arr\": [1],\n  \"o\": {\"k\": 1}\n}\n",
        "- 1",
        "/arr",
    ),
    (
        Language::TOML,
        "name = \"x\"\narr = [1]\n\n[t]\nk = 1\n",
        "- 1",
        "/arr",
    ),
    (Language::HCL, "a {\n  arr = [1]\n}\n", "- 1", "/a/arr"),
];

/// A list whose last item is deleted stays a list, empty, in every host
/// that holds an empty list (JSON and HCL saved a null, TOML waited).
#[test]
fn a_list_whose_last_item_is_deleted_stays_an_empty_list() {
    let mut failures = Vec::new();
    for (host, text, item, pointer) in LIST_HOSTS {
        match saved_after(host, text, &[(item, "")]) {
            Ok(saved) => {
                let read = host
                    .parse(&saved)
                    .map(|value| value.pointer(pointer).cloned());
                if !matches!(&read, Ok(Some(list)) if *list == json!([])) {
                    failures.push(format!("{host}: {read:?}\n{saved}"));
                }
            }
            Err(error) => failures.push(format!("{host}: {error}")),
        }
    }
    report(failures);
}

/// An XML host writes an emptied list as its element left empty (the
/// save waited on a null the user never wrote).
#[test]
fn an_emptied_xml_list_is_its_empty_element() {
    let text = "<r>\n  <arr>\n    <i>1</i>\n    <i>2</i>\n  </arr>\n</r>\n";
    let saved = saved_after(Language::XML, text, &[("- 1", ""), ("- 2", "")]);
    assert_eq!(saved.as_deref(), Ok("<r>\n  <arr/>\n</r>\n"));
}

/// A null typed over the list stays a null where the host holds one.
#[test]
fn a_null_written_over_a_list_stays_null() {
    let (host, text, item, pointer) = LIST_HOSTS[0];
    let edits = [("arr:", "arr: null\n"), (item, "")];
    let saved = saved_after(host, text, &edits);
    let read = saved
        .as_deref()
        .ok()
        .and_then(|saved| host.parse(saved).ok());
    assert_eq!(
        read.as_ref().and_then(|value| value.pointer(pointer)),
        Some(&Value::Null),
        "{saved:?}"
    );
}
