//! A large edit of an Edit-as view (Replace all, or the whole text pasted
//! over itself with a few changes) is one edit spanning the document: the
//! typing pause saving it follows the view's items through it with work in
//! proportion to them, with items written alike or not, and reads the
//! texts as often at any size (QA `SP37-1`: each cut item looked for over
//! the whole insert against every place taken, and each item among all
//! those followed, froze the page for seconds; and items cut by the edit
//! and written back as they were, taken for the next one written alike,
//! counted as moved, each read off the whole document again).

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::test_state::TestEditor;
use super::moved_forms::{pause, units};
use crate::editor::work_count::{Work, counted};
use leptos::prelude::*;

/// How the host's items are laid out.
#[derive(Clone, Copy, Debug)]
enum Items {
    /// Maps under keys of their own, holding the same.
    SameMaps,
    /// List items holding the same.
    SameList,
    /// List items each holding its own.
    OwnList,
}

/// The large edit made to the view.
#[derive(Clone, Copy, Debug)]
enum Large {
    /// The whole text pasted over itself, two values changed.
    Paste,
    /// Every `true` replaced with `false`.
    ReplaceAll,
    /// Every key `on` renamed `at`.
    RenameAll,
}

impl Large {
    /// The text it replaces, and with what.
    fn replaced(self) -> (&'static str, &'static str) {
        match self {
            Large::Paste | Large::ReplaceAll => ("true", "false"),
            Large::RenameAll => ("on", "at"),
        }
    }
}

/// A host of `count` items laid out as `items`, in `host`.
fn host_text(host: Language, items: Items, count: usize) -> String {
    let value = |at: usize| match items {
        Items::OwnList => at,
        Items::SameMaps | Items::SameList => 7,
    };
    let json = host == Language::JSON;
    let entries: Vec<String> = (0..count)
        .map(|at| match (items, json) {
            (Items::SameMaps, true) => {
                format!(
                    "  \"m{at}\": {{\n    \"vv\": {},\n    \"on\": true\n  }}",
                    value(at)
                )
            }
            (Items::SameMaps, false) => format!("m{at}:\n  vv: {}\n  on: true\n", value(at)),
            (_, true) => format!(
                "    {{\n      \"vv\": {},\n      \"on\": true\n    }}",
                value(at)
            ),
            (_, false) => format!("  - vv: {}\n    on: true\n", value(at)),
        })
        .collect();
    match (items, json) {
        (Items::SameMaps, true) => format!("{{\n{}\n}}\n", entries.join(",\n")),
        (_, true) => format!("{{\n  \"l\": [\n{}\n  ]\n}}\n", entries.join(",\n")),
        (Items::SameMaps, false) => entries.concat(),
        (_, false) => format!("l:\n{}", entries.concat()),
    }
}

/// `text` with `large` made: its text, the caret it leaves, and how many
/// places it changed.
fn edited(text: &str, large: Large) -> (String, usize, usize) {
    let (from, to) = large.replaced();
    let places: Vec<usize> = text.match_indices(from).map(|(at, _)| at).collect();
    let changed: Vec<usize> = match large {
        Large::ReplaceAll | Large::RenameAll => places.clone(),
        Large::Paste => vec![places[3], places[places.len() - 4]],
    };
    let mut out = text.to_string();
    for at in changed.iter().rev() {
        out.replace_range(*at..at + from.len(), to);
    }
    let caret = match large {
        Large::Paste => units(&out),
        Large::ReplaceAll | Large::RenameAll => {
            let grown = to.len() - from.len();
            let last = changed[changed.len() - 1] + (changed.len() - 1) * grown + to.len();
            units(&out[..last])
        }
    };
    (out, caret, changed.len())
}

/// The steps following the items of the pause saving `large`, in a
/// `host` of `count` items laid out as `items`, viewed as `view`, and the
/// whole documents read.
fn work(
    host: Language,
    view: Language,
    items: Items,
    large: Large,
    count: usize,
) -> (usize, usize) {
    let mut editor = TestEditor::new(host, &host_text(host, items, count));
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let before = state.doc().text.get_untracked();
    let (text, caret, changed) = edited(&before, large);
    let end = units(&before);
    let selection = match large {
        Large::Paste => (0, end),
        Large::ReplaceAll | Large::RenameAll => (caret, caret),
    };
    state.commit(Edit::new(before, selection, text, caret));
    let (((), steps), reads) = counted(Work::DocumentRead, || {
        counted(Work::LinkStep, || pause(state))
    });
    let saved = state.sync().host_text.get_value();
    assert_eq!(
        saved.matches(large.replaced().1).count(),
        changed,
        "{host:?} as {view:?}, {items:?}, {large:?}, {count}:\n{saved}"
    );
    (steps, reads)
}

#[test]
fn a_large_edit_follows_the_items_with_work_in_proportion_to_them() {
    let mut failures = Vec::new();
    for (host, view) in [
        (Language::JSON, Language::YAML),
        (Language::YAML, Language::JSON),
    ] {
        for items in [Items::SameMaps, Items::SameList, Items::OwnList] {
            for large in [Large::Paste, Large::ReplaceAll, Large::RenameAll] {
                let small = work(host, view, items, large, 40);
                let big = work(host, view, items, large, 160);
                // Four times the items: at most about four times the steps,
                // and as many reads.
                if big.0 > small.0 * 5 || big.1 != small.1 {
                    failures.push(format!(
                        "{host:?} as {view:?}, {items:?}, {large:?}: {small:?} steps and reads at 40 items, {big:?} at 160"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
