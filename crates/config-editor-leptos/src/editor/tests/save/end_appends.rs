//! A key added at the end of a host whose last value is a number in a
//! spelling the view's writer does not use (`3.10`, `0x1F`): the save
//! appends the key in place and keeps the number as the host wrote it
//! (P2), from every view that shows it.

use crate::editor::Language;
use crate::editor::save_matrix::{report, save_value};
use serde_json::json;

/// Numbers YAML reads in another written form, and two it writes as is.
const SPELLINGS: [&str; 7] = ["3.10", "1.50", "0x1F", "1e3", "0o17", "+7", "3.9"];

/// The last value as a list item, a map's value, and in a list of maps.
const SHAPES: [&str; 3] = [
    "jobs:\n  test:\n    python-version:\n      - 3.9\n      - {}\n",
    "jobs:\n  test:\n    python: 3.9\n    version: {}\n",
    "jobs:\n  test:\n    - name: a\n      version: {}\n",
];

#[test]
fn a_key_added_after_a_last_number_in_its_own_spelling_is_appended() {
    let mut failures = Vec::new();
    for (spelling, shape) in SPELLINGS
        .into_iter()
        .flat_map(|spelling| SHAPES.map(|shape| (spelling, shape)))
    {
        let host = shape.replace("{}", spelling);
        let expected = format!("{host}  lint:\n    run: 1\n");
        for edit in [Language::JSON, Language::TOML, Language::HCL] {
            let added = |value: &mut serde_json::Value| {
                value["jobs"]["lint"] = json!({"run": 1});
            };
            match save_value(Language::YAML, edit, &host, &added) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!(
                    "{host:?} via {edit}:\n--- expected\n{expected}--- saved\n{saved}"
                )),
                Err(error) => failures.push(format!("{host:?} via {edit}: {error}")),
            }
        }
    }
    report(failures);
}

/// The same inside the document: a key added to a map after its list
/// whose last item has its own spelling goes before the next key.
#[test]
fn a_key_added_after_a_list_ending_in_its_own_spelling_goes_before_the_next_key() {
    let mut failures = Vec::new();
    for spelling in SPELLINGS {
        let host = format!(
            "jobs:\n  test:\n    ratio:\n      - 1\n      - {spelling}\n  lint:\n    a: 1\n"
        );
        let expected = host.replace("  lint:", "    timeout: 5\n  lint:");
        for edit in [Language::JSON, Language::TOML, Language::HCL] {
            let added = |value: &mut serde_json::Value| {
                value["jobs"]["test"]["timeout"] = json!(5);
            };
            match save_value(Language::YAML, edit, &host, &added) {
                Ok(saved) if saved == expected => {}
                Ok(saved) => failures.push(format!(
                    "{host:?} via {edit}:\n--- expected\n{expected}--- saved\n{saved}"
                )),
                Err(error) => failures.push(format!("{host:?} via {edit}: {error}")),
            }
        }
    }
    report(failures);
}
