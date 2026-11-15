//! A map's or list's last entry deleted in a YAML view leaves `deps:`
//! with nothing under it, which reads as null: the save writes the view's
//! value (QA `FZ38-2`: the host read back as an empty block the view did
//! not hold), a null where the host's language writes one (JSON, HCL),
//! else it waits with the reason (TOML and XML hold no null).

use crate::editor::Language;
use crate::editor::bridge::Bridge;
use crate::editor::save_matrix::report;
use serde_json::Value;

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

/// What a host reads at `pointer` once the view holds a null there: the
/// null where it writes one, else the save waits.
fn null_kept(host: Language, saved: Result<String, String>, pointer: &str) -> Result<(), String> {
    if host == Language::TOML || host == Language::XML {
        return match saved {
            Err(_) => Ok(()),
            Ok(saved) => Err(format!(
                "{host}: saved what it cannot read as a null:\n{saved}"
            )),
        };
    }
    let saved = saved.map_err(|error| format!("{host}: {error}"))?;
    let read = host
        .parse(&saved)
        .map(|value| value.pointer(pointer).cloned());
    match read {
        Ok(Some(Value::Null)) => Ok(()),
        other => Err(format!("{host}: {other:?}\n{saved}")),
    }
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
fn a_map_whose_last_entry_is_deleted_saves_the_null_the_view_holds() {
    let mut failures = Vec::new();
    for (host, text, pointer) in HOSTS {
        if let Err(failure) = null_kept(host, saved_after(host, text, &[("x: 1", "")]), pointer) {
            failures.push(failure);
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
        assert_eq!(
            null_kept(Language::JSON, saved, "/deps"),
            Ok(()),
            "{written}"
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
const LIST_HOSTS: [(Language, &str, &str, &str); 4] = [
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
    (
        Language::XML,
        "<r>\n  <arr>\n    <i>1</i>\n    <i>2</i>\n  </arr>\n</r>\n",
        "- 1",
        "/r/arr",
    ),
];

/// A list whose last item is deleted saves the null the view holds, or
/// waits, in every host.
#[test]
fn a_list_whose_last_item_is_deleted_saves_the_null_the_view_holds() {
    let mut failures = Vec::new();
    for (host, text, item, pointer) in LIST_HOSTS {
        let edits = [(item, ""), ("- 2", "")];
        if let Err(failure) = null_kept(host, saved_after(host, text, &edits), pointer) {
            failures.push(failure);
        }
    }
    report(failures);
}
