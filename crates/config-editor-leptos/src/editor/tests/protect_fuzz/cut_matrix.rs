//! A list's key line copied, then one of the two copies cut (the one
//! above or the one below), in every list case and lock: the cut leaves
//! every value as it was, so it is taken wherever both texts parse, and
//! every protection holds (`replay`'s checks). Also with a copy of the
//! locked item's line pasted below the list, its line made another field
//! (a `:` typed before its item marker) and an item typed at the list's
//! end, so the copy is held on text its list's key does not write.

#![allow(clippy::string_slice, reason = "the test cuts its own texts")]

use super::acts::{Act, make};
use super::cases::{CASES, LOCKS, NAMES, value};
use super::checks::ordinary;
use super::mount::{Setup, case_text, mount};
use super::replay;
use crate::editor::state::EditorState;

/// The list cases: their pointer names an item.
fn list_cases() -> impl Iterator<Item = usize> {
    (0..CASES.len()).filter(|&case| CASES[case].pointer.ends_with("/0"))
}

/// The acts for `setup`: the key line copied; when `held`, a copy of the
/// locked item's line pasted below the list, a `:` typed before its item
/// marker and an item typed at the list's end (the copy's index another
/// item's now: held); then the key's copy above (`upper`) or below cut.
fn acts(setup: &Setup, held: bool, upper: bool) -> Vec<Act> {
    let text = case_text(setup);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let item = lines
        .iter()
        .position(|line| line.contains(value(setup.lock)))
        .unwrap_or_default();
    // The list's key (or its opening line) is the line above its first
    // item.
    let key = item.saturating_sub(1);
    let mut acts = vec![Act::Line {
        line: key as u16,
        command: 2,
    }];
    if held {
        // Below the list's second item, a line down for the key's copy.
        let below = item + 3;
        let start = lines[key].len()
            + lines[..item + 2]
                .iter()
                .map(|line| line.len())
                .sum::<usize>();
        let indent = lines[item].len() - lines[item].trim_start().len();
        let anchors = setup.case().anchors;
        let last = anchors
            .iter()
            .position(|anchor| anchor.after.contains("other"))
            .unwrap_or_default();
        let entry = (anchors[last].entry)(NAMES[2]);
        acts.extend([
            Act::PasteCopy { line: below as u16 },
            Act::Entry {
                anchor: last as u8,
                name: 2,
                paste: true,
            },
            // `:` (see `acts::TYPED`), on the copy's line.
            Act::Type {
                at: (start + entry.len() + indent) as u16,
                text: 4,
            },
        ]);
    }
    acts.push(Act::Cut {
        line: (key + usize::from(!upper)) as u16,
    });
    acts
}

/// The text on screen.
fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

#[test]
fn a_copied_key_line_cut_above_or_below_its_list_is_taken() {
    let mut failures = Vec::new();
    // The cuts judged ordinary (both texts parse, values as they were):
    // the matrix must reach some, held ones included.
    let mut judged = (0, 0);
    for case in list_cases() {
        for lock in LOCKS {
            for held in [false, true] {
                for upper in [true, false] {
                    let setup = Setup {
                        case,
                        lock,
                        host: None,
                        pairs: false,
                    };
                    let acts = acts(&setup, held, upper);
                    if let Err(failure) = replay(&setup, &acts) {
                        failures.push(format!(
                            "{} {lock:?} held {held} upper {upper}: {:?}: {}",
                            CASES[case].label, failure.class.check, failure.detail
                        ));
                        continue;
                    }
                    if reaches_ordinary_cut(&setup, &acts) {
                        if held {
                            judged.1 += 1;
                        } else {
                            judged.0 += 1;
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        judged.0 > 0 && judged.1 > 0,
        "ordinary cuts judged (plain, held): {judged:?}"
    );
}

/// Whether the last act of `acts` (a cut) is an ordinary edit once the
/// others are made.
fn reaches_ordinary_cut(setup: &Setup, acts: &[Act]) -> bool {
    let Some(mounted) = mount(setup) else {
        return false;
    };
    let Some((Act::Cut { line }, made)) = acts.split_last() else {
        return false;
    };
    for act in made {
        // Judged by `replay`; only the text it leaves is read here.
        let _ = make(setup, &mounted, act);
    }
    let before = shown(mounted.state);
    let lines: Vec<&str> = before.split_inclusive('\n').collect();
    let start: usize = lines
        .iter()
        .take(*line as usize)
        .map(|line| line.len())
        .sum();
    let length = lines.get(*line as usize).map_or(0, |line| line.len());
    let after = format!("{}{}", &before[..start], &before[start + length..]);
    ordinary(
        setup,
        setup.view_language(),
        value(setup.lock),
        &before,
        &after,
        true,
    )
}
