//! M25-E1: the blank line that parts a header comment from the first
//! entry's own comment stays when that entry is removed from another view,
//! in every language with comments, for a map's first key and a list's
//! first item, at the top of the file and under an opener.

use crate::editor::Language;
use crate::editor::save_matrix::{from_every_view, report};
use serde_json::Value;

fn removed_first(value: &mut Value, pointer: &str) {
    match value.pointer_mut(pointer) {
        Some(Value::Object(map)) => {
            let first = map.keys().next().cloned().unwrap();
            map.shift_remove(&first);
        }
        Some(Value::Array(items)) => {
            items.remove(0);
        }
        _ => panic!("no container at {pointer}"),
    }
}

#[test]
fn the_blank_line_under_a_header_stays_when_the_first_entry_goes() {
    let cases: [(Language, &str, &str, &str); 8] = [
        (
            Language::YAML,
            "# h\n\n# A\na: 1\nb: 2\n",
            "",
            "# h\n\nb: 2\n",
        ),
        (
            Language::YAML,
            "l:\n  # h\n\n  # A\n  - a\n  - b\n",
            "/l",
            "l:\n  # h\n\n  - b\n",
        ),
        (
            Language::YAML,
            "s:\n  # h\n\n  # A\n  a: 1\n  b: 2\n",
            "/s",
            "s:\n  # h\n\n  b: 2\n",
        ),
        (
            Language::TOML,
            "# h\n\n# A\na = 1\nb = 2\n",
            "",
            "# h\n\nb = 2\n",
        ),
        (
            Language::HCL,
            "# h\n\n# A\na = 1\nb = 2\n",
            "",
            "# h\n\nb = 2\n",
        ),
        (
            Language::HCL,
            "s {\n  # h\n\n  # A\n  a = 1\n  b = 2\n}\n",
            "/s",
            "s {\n  # h\n\n  b = 2\n}\n",
        ),
        (
            Language::XML,
            "<r>\n  <!-- h -->\n\n  <!-- A -->\n  <a>1</a>\n  <b>2</b>\n</r>\n",
            "/r",
            "<r>\n  <!-- h -->\n\n  <b>2</b>\n</r>\n",
        ),
        (
            Language::YAML,
            "# h\n\n# A\na: 1\n\nb: 2\n",
            "",
            "# h\n\nb: 2\n",
        ),
    ];
    let mut failures = Vec::new();
    for (host, text, pointer, expected) in cases {
        from_every_view(
            &mut failures,
            text,
            host,
            text,
            &|value| removed_first(value, pointer),
            expected,
        );
    }
    report(failures);
}

/// Between entries, a commented entry removed takes the blank line above
/// its comment, as before: no gap is left.
#[test]
fn a_commented_entry_between_others_leaves_no_gap() {
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "middle",
        Language::YAML,
        "a: 1\n\n# B\nb: 2\nc: 3\n",
        &|value| {
            value.as_object_mut().unwrap().shift_remove("b");
        },
        "a: 1\nc: 3\n",
    );
    report(failures);
}
