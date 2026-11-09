//! YAML merge keys (`<<: *defaults`) through saves from other views: the
//! merge stays, and a merged value the edit sets apart from its anchor's
//! becomes the map's own key; only a merged key the edit removes writes
//! the merge out.

use super::Language;
use super::save_matrix::{from_every_view, report, slot};
use serde_json::{Value, json};

type Change = fn(&mut Value);

const HOST: &str = "# defaults\ndefaults: &D\n  a: 1 # da\n  b: 2\n  c: 0x3\nsvc:\n  # merged\n  <<: *D\n  b: 20 # own\n  d: 4\n";

#[test]
fn a_merge_stays_through_edits_of_merged_values_and_of_its_anchor() {
    let cases: [(&str, Change, String); 7] = [
        (
            "a merged value edited",
            |value| value["svc"]["a"] = json!(9),
            HOST.replace("  <<: *D\n", "  a: 9\n  <<: *D\n"),
        ),
        (
            "two merged values edited",
            |value| {
                value["svc"]["a"] = json!(9);
                value["svc"]["c"] = json!(8);
            },
            HOST.replace("  <<: *D\n", "  a: 9\n  c: 8\n  <<: *D\n"),
        ),
        (
            "the anchor's value edited: the map keeps it, as written",
            |value| value["defaults"]["c"] = json!(30),
            HOST.replace("  c: 0x3\n", "  c: 30\n")
                .replace("  <<: *D\n", "  <<: *D\n  c: 0x3\n"),
        ),
        (
            "the anchor's value and the merged one edited alike",
            |value| {
                value["defaults"]["c"] = json!(30);
                value["svc"]["c"] = json!(30);
            },
            HOST.replace("  c: 0x3\n", "  c: 30\n"),
        ),
        (
            "a key added to the anchor and the map alike: the map writes it too",
            |value| {
                value["defaults"]["e"] = json!(5);
                value["svc"]["e"] = json!(5);
            },
            HOST.replace("  c: 0x3\n", "  c: 0x3\n  e: 5\n")
                .replace("  d: 4\n", "  d: 4\n  e: 5\n"),
        ),
        (
            "an own key edited",
            |value| value["svc"]["b"] = json!(21),
            HOST.replace("b: 20", "b: 21"),
        ),
        (
            "a merged key removed: the merge is written out",
            |value| {
                value["svc"].as_object_mut().unwrap().shift_remove("a");
            },
            HOST.replace("  <<: *D\n", "  c: 0x3\n"),
        ),
    ];
    let mut failures = Vec::new();
    for (label, change, expected) in cases {
        from_every_view(
            &mut failures,
            label,
            Language::YAML,
            HOST,
            &change,
            &expected,
        );
    }
    report(failures);
}

/// A merge on a list item's line (`- <<: *p`) takes its overrides at the
/// item's column.
#[test]
fn a_merge_on_a_list_item_takes_its_overrides_at_the_items_column() {
    let host = "p: &p\n  a: 1\n  b: 2\nl:\n  - <<: *p # m\n    n: x\n";
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "merged value edited",
        Language::YAML,
        host,
        &|value| *slot(value, "/l/0/b") = json!(3),
        "p: &p\n  a: 1\n  b: 2\nl:\n  - <<: *p # m\n    b: 3\n    n: x\n",
    );
    report(failures);
}

/// Merged values on a list item: an override goes on the merge's side
/// where the item reads its keys in the same order (an inherited value
/// turned into a map too); one between keys still inherited writes the
/// merge out, its comment kept.
#[test]
fn merged_values_of_list_items_keep_their_order() {
    let host = "p: &p {a: 1, b: 2, c: 3}\nl:\n  - <<: *p # m\n    n: x # n\n";
    let cases: [(&str, Change, &str); 3] = [
        (
            "the first merged value a map",
            |value| *slot(value, "/l/0/a") = json!({"k": 1}),
            "p: &p {a: 1, b: 2, c: 3}\nl:\n  - a:\n      k: 1\n    <<: *p # m\n    n: x # n\n",
        ),
        (
            "the last merged value edited",
            |value| *slot(value, "/l/0/c") = json!(9),
            "p: &p {a: 1, b: 2, c: 3}\nl:\n  - <<: *p # m\n    c: 9\n    n: x # n\n",
        ),
        (
            "a merged value between inherited ones edited",
            |value| *slot(value, "/l/0/b") = json!(9),
            "p: &p {a: 1, b: 2, c: 3}\nl:\n  - a: 1 # m\n    b: 9\n    c: 3\n    n: x # n\n",
        ),
    ];
    let mut failures = Vec::new();
    for (label, change, expected) in cases {
        from_every_view(
            &mut failures,
            label,
            Language::YAML,
            host,
            &change,
            expected,
        );
    }
    report(failures);
}

/// An anchor emptied leaves its merge nothing to read: the merge is written
/// out, and the view's values stay.
#[test]
fn a_merge_whose_anchor_is_emptied_is_written_out() {
    let host = "# c\nbase: &b # b\n  min_idle: 0x1F\npool:\n  <<: *b\nlisten: 1 # l\n";
    let mut failures = Vec::new();
    from_every_view(
        &mut failures,
        "anchor emptied",
        Language::YAML,
        host,
        &|value| {
            value["base"]
                .as_object_mut()
                .unwrap()
                .shift_remove("min_idle");
        },
        "# c\nbase: {} # b\npool:\n  min_idle: 0x1F\nlisten: 1 # l\n",
    );
    report(failures);
}
