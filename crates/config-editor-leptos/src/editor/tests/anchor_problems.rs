//! A schema problem inside a YAML anchor that a map reads through a merge
//! (`<<: *base`, `<<: [*a, *b]`) or an alias (`a: *base`) is placed on the
//! text the anchor writes, through every step (anchors inside anchors),
//! for every kind of problem, in the editor and in `validate`.

use std::sync::Arc;

use leptos::prelude::*;

use crate::editor::diagnostics::{Schema, build_schema};
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "svc": {
      "type": "object",
      "additionalProperties": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "port": {"type": "integer"},
          "host": {"type": "string"},
          "limits": {
            "type": "object",
            "required": ["name"],
            "additionalProperties": false,
            "properties": {"name": {"type": "string"}, "cpu": {"type": "integer"}}
          }
        }
      }
    }
  }
}"#;

/// Each case: the text, and where its one problem is (line, column, and
/// what the line holds there).
const CASES: &[(&str, &str, (usize, usize))] = &[
    // An unknown key, through a merge.
    (
        "merge, unknown key",
        "base: &b\n  prt: 1\nsvc:\n  a:\n    <<: *b\n    host: h\n",
        (2, 3),
    ),
    // A wrong type, through a merge.
    (
        "merge, wrong type",
        "base: &b\n  port: x\nsvc:\n  a:\n    <<: *b\n    host: h\n",
        (2, 9),
    ),
    // Of several merges, the one that writes the key.
    (
        "merges, unknown key",
        "one: &a\n  host: h\ntwo: &b\n  prt: 1\nsvc:\n  a:\n    <<: [*a, *b]\n",
        (4, 3),
    ),
    (
        "merges, wrong type",
        "one: &a\n  host: h\ntwo: &b\n  port: x\nsvc:\n  a:\n    <<: [*a, *b]\n",
        (4, 9),
    ),
    // A map read through an alias.
    (
        "alias, unknown key",
        "base: &b\n  prt: 1\nsvc:\n  a: *b\n",
        (2, 3),
    ),
    (
        "alias, wrong type",
        "base: &b\n  port: x\nsvc:\n  a: *b\n",
        (2, 9),
    ),
    // An anchor merging another one.
    (
        "nested merges, unknown key",
        "inner: &i\n  prt: 1\nbase: &b\n  <<: *i\n  host: h\nsvc:\n  a:\n    <<: *b\n",
        (2, 3),
    ),
    (
        "alias of a merge, wrong type",
        "inner: &i\n  port: x\nbase: &b\n  <<: *i\nsvc:\n  a: *b\n",
        (2, 9),
    ),
    // Deeper inside the anchor.
    (
        "alias, nested wrong type",
        "base: &b\n  limits:\n    name: n\n    cpu: x\nsvc:\n  a: *b\n",
        (4, 10),
    ),
    (
        "merge, nested unknown key",
        "base: &b\n  limits:\n    name: n\n    cpus: 1\nsvc:\n  a:\n    <<: *b\n",
        (4, 5),
    ),
    // A missing property: on the key of the object missing it.
    (
        "merge, missing required",
        "base: &b\n  limits:\n    cpu: 1\nsvc:\n  a:\n    <<: *b\n",
        (2, 3),
    ),
    (
        "alias, missing required",
        "base: &b\n  limits:\n    cpu: 1\nsvc:\n  a: *b\n",
        (2, 3),
    ),
    // An anchor under a list item and under a nested key.
    (
        "anchor in a nested key",
        "defs:\n  x: &x\n    prt: 1\nsvc:\n  a: *x\n",
        (3, 5),
    ),
    (
        "anchor on an item",
        "defs:\n  - &x\n    prt: 1\nsvc:\n  a: *x\n",
        (3, 5),
    ),
];

fn places(found: &[crate::editor::diagnostics::Diagnostic]) -> Vec<(Option<usize>, Option<usize>)> {
    found
        .iter()
        .map(|problem| (problem.line, problem.column))
        .collect()
}

#[test]
fn problems_inside_a_merged_or_aliased_anchor_are_placed_in_the_anchor() {
    let schema = Schema::compile(SCHEMA).unwrap();
    let mut failures = Vec::new();
    for (label, text, (line, column)) in CASES {
        let expected = vec![(Some(*line), Some(*column))];
        let found = schema.validate(Language::YAML, text);
        if places(&found) != expected {
            failures.push(format!("validate, {label}: {found:?}"));
        }
        let mut editor = TestEditor::new(Language::YAML, text);
        editor.schema = Some(Arc::new(build_schema(SCHEMA).unwrap()));
        let (_owner, state) = editor.mount();
        state.problems().diagnosed_text.set(text.to_string());
        let shown = state.problems().own.get_untracked();
        if places(&shown) != expected {
            failures.push(format!("editor, {label}: {shown:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn problems_inside_a_large_merged_anchor_read_the_anchors_once() {
    use crate::editor::work_count::{Work, counted};
    let keys: String = (0..2000)
        .map(|index| format!("  unknown_{index}: {index}\n"))
        .collect();
    let schema = Schema::compile(SCHEMA).unwrap();
    let reads = |text: &str| {
        let (problems, reads) =
            counted(Work::DocumentRead, || schema.validate(Language::YAML, text));
        let placed = problems
            .iter()
            .filter(|problem| problem.line.is_some_and(|line| line > 1))
            .count();
        (placed, reads)
    };
    // The same problems written in place, and in an anchor merged twice.
    let (_, direct) = reads(&format!(
        "svc:\n  a:\n{}",
        keys.replace("\n  ", "\n    ").replacen("  ", "    ", 1)
    ));
    let (placed, through) = reads(&format!(
        "base: &b\n{keys}svc:\n  a:\n    <<: *b\n  c: *b\n"
    ));
    // The anchors and the walk to find them read once more; read per
    // problem, dozens more.
    assert!(
        placed >= 20 && through <= direct + 3,
        "{placed} placed in {through} reads, {direct} written in place"
    );
}
