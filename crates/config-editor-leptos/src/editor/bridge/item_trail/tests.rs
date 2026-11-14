use super::super::super::Language;
use super::super::super::item_sources::ItemSource;
use super::{Splice, Trail};
use std::collections::HashMap;

/// The UTF-16 offset of the `nth` `needle` in `text`, plus `delta`.
fn at(text: &str, needle: &str, nth: usize, delta: usize) -> usize {
    let mut found = text.match_indices(needle).nth(nth).expect("found").0;
    found += delta;
    text[..found].encode_utf16().count()
}

/// Each item's source after the edits `steps` (each text with the caret
/// it leaves), from `start`, in `language`.
fn traced(
    language: Language,
    start: &str,
    steps: &[(String, usize)],
) -> HashMap<String, ItemSource> {
    let mut trail = Trail::starting(start);
    let mut text = start.to_string();
    for (next, caret) in steps {
        trail.note(&text, next, *caret);
        text = next.clone();
    }
    trail
        .trace(language.behavior(), &text)
        .expect("traced")
        .items
}

fn previous(pairs: &[(&str, &str)]) -> HashMap<String, ItemSource> {
    pairs
        .iter()
        .map(|(new, old)| (new.to_string(), ItemSource::Previous(old.to_string())))
        .collect()
}

const SAME: &str = "shapes:\n  shape:\n    - size: 1\n    - size: 1\n";

#[test]
fn the_item_whose_lines_were_deleted_is_the_one_removed() {
    // Deleting the first of two equal lines leaves the caret on the second
    // (now the first), at the column it was on.
    let after = "shapes:\n  shape:\n    - size: 1\n".to_string();
    let caret = at(&after, "- size", 0, 2);
    assert_eq!(
        traced(Language::YAML, SAME, &[(after.clone(), caret)]),
        previous(&[("/shapes/shape/0", "/shapes/shape/1")])
    );
    // Deleting the second leaves it at the end.
    assert_eq!(
        traced(
            Language::YAML,
            SAME,
            &[(after.clone(), after.encode_utf16().count())]
        ),
        previous(&[("/shapes/shape/0", "/shapes/shape/0")])
    );
    // The same in a JSON view, the item's lines selected and deleted.
    let json = "{\n  \"l\": [\n    {\n      \"size\": 1\n    },\n    {\n      \"size\": 1\n    }\n  ]\n}\n";
    let after = json.replacen("    {\n      \"size\": 1\n    },\n", "", 1);
    let caret = at(json, "    {", 0, 0);
    assert_eq!(
        traced(Language::JSON, json, &[(after, caret)]),
        previous(&[("/l/0", "/l/1")])
    );
}

const TARGETS: &str = "targets:\n  - name: logfile\n  - name: console\n  - name: blackhole\n";

#[test]
fn an_item_cut_and_pasted_is_the_item_moved() {
    let cut = TARGETS.replace("  - name: console\n", "");
    let pasted = format!("{cut}  - name: console\n");
    let steps = [
        (cut.clone(), at(&cut, "  - name: blackhole", 0, 0)),
        (pasted.clone(), pasted.encode_utf16().count()),
    ];
    assert_eq!(
        traced(Language::YAML, TARGETS, &steps),
        previous(&[
            ("/targets/0", "/targets/0"),
            ("/targets/1", "/targets/2"),
            ("/targets/2", "/targets/1")
        ])
    );
    // Cut in one save, pasted in the next: the session's first item.
    let mut trail = Trail::starting(TARGETS);
    trail.note(TARGETS, &cut, steps[0].1);
    let saved = trail
        .trace(Language::YAML.behavior(), &cut)
        .expect("traced")
        .saved;
    let mut trail = saved;
    trail.note(&cut, &pasted, steps[1].1);
    let items = trail
        .trace(Language::YAML.behavior(), &pasted)
        .expect("traced")
        .items;
    assert_eq!(
        items.get("/targets/2"),
        Some(&ItemSource::Origin("/targets/1".into()))
    );
    assert_eq!(
        items.get("/targets/1"),
        Some(&ItemSource::Previous("/targets/1".into()))
    );
}

#[test]
fn items_swapped_in_one_edit_are_each_moved_and_an_edited_one_stays() {
    let shapes = "l:\n  - size: 1\n  - size: 2\n";
    // `Alt+Up` on the second: one edit, from the first digit to the second.
    let swapped = "l:\n  - size: 2\n  - size: 1\n".to_string();
    let caret = at(&swapped, "- size: 2", 0, 0);
    assert_eq!(
        traced(Language::YAML, shapes, &[(swapped, caret)]),
        previous(&[("/l/0", "/l/1"), ("/l/1", "/l/0")])
    );
    // A value typed over in place keeps its item.
    let typed = "l:\n  - size: 7\n  - size: 2\n".to_string();
    let caret = at(&typed, "7", 0, 1);
    assert_eq!(
        traced(Language::YAML, shapes, &[(typed, caret)]),
        previous(&[("/l/0", "/l/0"), ("/l/1", "/l/1")])
    );
}

#[test]
fn a_deletion_spread_over_whole_lines_is_read_where_the_caret_says() {
    let before = "a\nb\nb\nc\n";
    let after = "a\nb\nc\n";
    // The first `b` line deleted, the caret left on the second.
    let splice = Splice::of(before, after, 2).expect("an edit");
    assert_eq!(
        (splice.at, splice.removed.as_str(), splice.inserted.as_str()),
        (2, "b\n", "")
    );
    let splice = Splice::of(before, after, 4).expect("an edit");
    assert_eq!((splice.at, splice.removed.as_str()), (4, "b\n"));
    // An edit inside one line stays there.
    let splice = Splice::of("k: 1\n", "k: 12\n", 5).expect("an edit");
    assert_eq!(
        (splice.at, splice.removed.as_str(), splice.inserted.as_str()),
        (4, "", "2")
    );
}

/// A save's trace reads each of the two texts once, whatever their size:
/// one read of each for its spans, its parse once (a typing pause's
/// cost, at two sizes).
#[test]
fn a_trace_reads_each_text_once_at_any_size() {
    use super::super::super::work_count::{Work, counted};
    let counts = |items: usize| {
        let start: String = std::iter::once("l:\n".to_string())
            .chain((0..items).map(|item| format!("  - size: {item}\n")))
            .collect();
        let after = start.replacen("  - size: 1\n", "", 1);
        let caret = start.find("  - size: 1\n").expect("found");
        let mut trail = Trail::starting(&start);
        trail.note(&start, &after, caret);
        let ((((traced, lines), paths), reads), parsed) = counted(Work::ParsedBytes, || {
            counted(Work::DocumentRead, || {
                counted(Work::PathLookup, || {
                    counted(Work::LineSplit, || {
                        trail
                            .trace(Language::YAML.behavior(), &after)
                            .map(|trace| trace.items.len())
                    })
                })
            })
        });
        assert_eq!(traced, Some(items - 1));
        assert!(parsed <= start.len() + after.len(), "{parsed} bytes parsed");
        (lines, paths, reads)
    };
    assert_eq!(counts(50), counts(800));
}

/// Deleting from an item's first character to the next item's (a
/// selection, as a mouse makes it) removes the first: the edit is read
/// back to the start of the line the caret was on, not forward.
#[test]
fn a_selection_from_item_to_item_deletes_the_first() {
    let json = "{\n  \"l\": [\n    {\n      \"size\": 1\n    },\n    {\n      \"size\": 1\n    }\n  ]\n}\n";
    let selected = "{\n      \"size\": 1\n    },\n    ";
    let caret = at(json, selected, 0, 0);
    let after = json.replacen(selected, "", 1);
    assert_eq!(
        traced(Language::JSON, json, &[(after, caret)]),
        previous(&[("/l/0", "/l/1")])
    );
}

/// QA `E34-1`: an item cut, saved, then pasted after a switch where its
/// lines read otherwise (another indent) is the item cut, by its data.
#[test]
fn an_item_pasted_with_other_lines_is_the_item_cut_by_its_data() {
    let cut = TARGETS.replace("  - name: console\n", "");
    let caret = at(&cut, "  - name: blackhole", 0, 0);
    let saved = |text: &str| {
        let mut trail = Trail::starting(TARGETS);
        trail.note(TARGETS, &cut, caret);
        let mut trail = trail
            .trace(Language::YAML.behavior(), &cut)
            .expect("traced")
            .saved;
        // A switch and back: the same text, the session's items known.
        trail.restart(&cut);
        trail.note(&cut, text, text.encode_utf16().count());
        trail
            .trace(Language::YAML.behavior(), text)
            .expect("traced")
            .items
    };
    let pasted = format!("{cut}  -   name: console\n");
    assert_eq!(
        saved(&pasted).get("/targets/2"),
        Some(&ItemSource::Origin("/targets/1".into()))
    );
    // Another item's data is no item cut.
    let other = format!("{cut}  - name: other\n");
    assert_eq!(saved(&other).get("/targets/2"), None);
}

/// The items pasted are compared with the items cut, not with every item
/// of the view: a typing pause after a cut and a paste compares as many
/// at any size.
#[test]
fn pasted_items_are_compared_with_the_cut_ones_only_at_any_size() {
    use super::super::super::work_count::{Work, counted};
    let comparisons = |count: usize| {
        let start: String = std::iter::once("l:\n".to_string())
            .chain((0..count).map(|item| format!("  - size: {item}\n")))
            .collect();
        let cut = start.replacen("  - size: 1\n", "", 1);
        let caret = start.find("  - size: 1\n").expect("found");
        let mut trail = Trail::starting(&start);
        trail.note(&start, &cut, caret);
        let mut trail = trail
            .trace(Language::YAML.behavior(), &cut)
            .expect("traced")
            .saved;
        let pasted = format!("{cut}  -  size: 1\n");
        trail.note(&cut, &pasted, pasted.encode_utf16().count());
        let (items, compared) = counted(Work::ItemComparison, || {
            trail
                .trace(Language::YAML.behavior(), &pasted)
                .expect("traced")
                .items
        });
        assert_eq!(
            items.get(&format!("/l/{}", count - 1)),
            Some(&ItemSource::Origin("/l/1".into()))
        );
        compared
    };
    assert_eq!(comparisons(50), 1);
    assert_eq!(comparisons(800), 1);
}
