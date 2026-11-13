//! An Edit-as switch with folds closed costs what it costs with none: the
//! fields the folds start on, and the folds starting on them in the new
//! text, are found from one reading of each text (each fold, and each
//! foldable block of the new text, read the whole text again from its
//! top: 11.5 s for one switch with a single fold at 6,210 lines).

use leptos::prelude::*;

use super::super::keys::fold_scale_tests::blocks;
use super::super::languages::Language;
use super::super::sync::switch_to;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted};

/// The reads of a text from its top for one place in it, and its line
/// seeks, made by a switch from `host` to `edit` of a text of `count`
/// blocks with one block closed (or all of them); and the folds after.
fn switch_reads(host: Language, edit: Language, count: usize, all: bool) -> ([usize; 2], usize) {
    let text = blocks(host, count);
    let (_owner, state) = TestEditor::new(host, &text).mount();
    let ranges = state.fold().fold_ranges.get_untracked();
    let blocks: Vec<_> = ranges.into_iter().filter(|fold| fold.start > 2).collect();
    let closed = if all {
        blocks
    } else {
        blocks.into_iter().skip(1).take(1).collect()
    };
    assert!(!closed.is_empty(), "{host}: a block folds");
    state.set_folds(closed);
    let (((), lookups), seeks) = counted(Work::LineSeek, || {
        counted(Work::PathLookup, || switch_to(state, edit))
    });
    assert_eq!(
        state.sync().edit_language.get_untracked(),
        edit,
        "{host} as {edit}: switched"
    );
    let folds = state.fold().folded.with_untracked(Vec::len);
    ([lookups, seeks], folds)
}

#[test]
fn a_switch_reads_each_text_once_for_its_folds_whatever_their_number_and_the_size() {
    let mut failures = Vec::new();
    for host in Language::ALL.iter().copied() {
        for edit in Language::ALL.iter().copied().filter(|edit| *edit != host) {
            for all in [false, true] {
                let (small, small_folds) = switch_reads(host, edit, 20, all);
                let (large, large_folds) = switch_reads(host, edit, 200, all);
                if small_folds == 0 || large_folds == 0 {
                    failures.push(format!("{host} as {edit} (all {all}): no fold carried ({small_folds}, {large_folds})"));
                }
                if small != large {
                    failures.push(format!(
                        "{host} as {edit} (all {all}): [path reads, line seeks] {small:?} at 20 blocks, {large:?} at 200"
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
