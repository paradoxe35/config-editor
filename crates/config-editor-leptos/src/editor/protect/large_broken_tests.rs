//! Large texts that do not parse, broken in more places than a stand-in
//! mends and longer than a text read line by line, in every language and
//! arrival order: an edit changing a locked value never reaches a text that
//! parses (I1), and repairing each break, which keeps every locked value,
//! always goes through (I2).

use super::super::Language;
use super::super::codec;
use super::arrival_tests::{From, GHOST, LOCKED, allowed, locks};
use super::locks::Protected;
use super::tests::{documents, edits, schema};

/// One filler line, and the same line broken.
fn filler(language: Language, index: usize) -> (String, String) {
    match language.label() {
        "YAML" => (
            format!("k{index}: {index}"),
            format!("k{index}: \"{index}\" y"),
        ),
        "JSON" => (
            format!("  \"k{index}\": {index},"),
            format!("  \"k{index}\" {index},"),
        ),
        "XML" => (
            format!("<k{index}>{index}</k{index}>"),
            format!("<k{index}>{index}</k{index}"),
        ),
        _ => (format!("k{index} = {index}"), format!("k{index} = ")),
    }
}

/// `text` after `size` filler lines (inside the object in JSON), and the
/// lines broken: `breaks` of them, spread over the filler.
fn large(
    language: Language,
    text: &str,
    size: usize,
    breaks: usize,
) -> (String, Vec<(String, String)>) {
    let lines: String = (0..size)
        .map(|index| format!("{}\n", filler(language, index).0))
        .collect();
    let large = match language.label() {
        "JSON" => text.replacen("{\n", &format!("{{\n{lines}"), 1),
        _ => format!("{lines}{text}"),
    };
    let pairs = (0..breaks)
        .map(|at| filler(language, 1 + at * (size - 2) / breaks.max(1)))
        .map(|(good, bad)| (format!("{good}\n"), format!("{bad}\n")))
        .collect();
    (large, pairs)
}

fn broken(text: &str, pairs: &[(String, String)]) -> String {
    pairs.iter().fold(text.to_string(), |text, (good, bad)| {
        text.replacen(good, bad, 1)
    })
}

/// The edits changing a locked value tried here.
const TAMPERS: [&str; 5] = [
    "change the value",
    "rename the key",
    "change a value inside the block",
    "delete a child of the block",
    "add a child to the block",
];

/// How the text and the locks arrive, ending on the broken text and its
/// locks.
#[derive(Clone, Copy, Debug)]
enum Arrival {
    /// The host gives the broken text, the locks with it.
    Broken,
    /// The host gives the broken text, and names the paths after.
    PathsAfter,
    /// The host gives the text whole; the user breaks it.
    UserBreaks,
}

fn arrive(
    language: Language,
    good: &str,
    pairs: &[(String, String)],
    from: From,
    arrival: Arrival,
    failures: &mut Vec<String>,
) -> (String, Protected) {
    let bad = broken(good, pairs);
    match arrival {
        Arrival::Broken => {
            let protected = locks(language, &bad, from, true, None);
            (bad, protected)
        }
        Arrival::PathsAfter => {
            let before = locks(language, &bad, from, false, None);
            let protected = locks(language, &bad, from, true, Some(&before));
            (bad, protected)
        }
        Arrival::UserBreaks => {
            let mut shown = good.to_string();
            let mut protected = locks(language, &shown, from, true, None);
            for (good, bad) in pairs {
                let edited = shown.replacen(good, bad, 1);
                if !allowed(language, &shown, &edited, &protected) {
                    failures.push(format!("{language} ({from:?}): breaking {good:?} refused"));
                }
                let previous = std::mem::take(&mut protected);
                protected = locks(language, &edited, from, true, Some(&previous));
                shown = edited;
            }
            (shown, protected)
        }
    }
}

/// Repairs each break in turn while they go through; the text reached.
fn repair_all(
    language: Language,
    mut shown: String,
    mut protected: Protected,
    pairs: &[(String, String)],
    from: From,
) -> (String, Option<String>) {
    for (good, bad) in pairs {
        let repaired = shown.replacen(bad, good, 1);
        if !allowed(language, &shown, &repaired, &protected) {
            return (shown, Some(format!("repairing {bad:?} refused")));
        }
        let previous = std::mem::take(&mut protected);
        protected = locks(language, &repaired, from, true, Some(&previous));
        shown = repaired;
    }
    (shown, None)
}

#[test]
fn large_texts_broken_in_many_places_keep_their_locks_both_ways() {
    let schema = schema();
    let mut failures = Vec::new();
    let mut tampered = 0;
    for (language, text) in documents() {
        let original = codec::read(language.behavior(), text, Some(&schema)).unwrap();
        for (size, breaks, arrivals) in [
            (
                700,
                5,
                &[Arrival::Broken, Arrival::PathsAfter, Arrival::UserBreaks][..],
            ),
            (2500, 6, &[Arrival::Broken][..]),
        ] {
            let (good, pairs) = large(language, text, size, breaks);
            for from in [From::Schema, From::Paths] {
                for &arrival in arrivals {
                    if matches!(arrival, Arrival::PathsAfter) && matches!(from, From::Schema) {
                        continue;
                    }
                    let tag =
                        format!("{language} {size} lines, {breaks} breaks ({from:?}, {arrival:?})");
                    let (shown, protected) =
                        arrive(language, &good, &pairs, from, arrival, &mut failures);
                    // I2: the repairs go through, back to the host's text.
                    match repair_all(language, shown.clone(), protected.clone(), &pairs, from) {
                        (_, Some(refused)) => failures.push(format!("{tag}: {refused}")),
                        (repaired, None) if repaired != good => {
                            failures.push(format!("{tag}: repaired to another text"));
                        }
                        _ => {}
                    }
                    // I1: a locked value changed while broken never parses.
                    for (what, edited, _) in edits(language, &shown) {
                        let Some(edited) = edited.filter(|_| TAMPERS.contains(&what)) else {
                            continue;
                        };
                        if !allowed(language, &shown, &edited, &protected) {
                            continue;
                        }
                        tampered += 1;
                        let carried = locks(language, &edited, from, true, Some(&protected));
                        let (reached, _) = repair_all(language, edited, carried, &pairs, from);
                        let Ok(read) = codec::read(language.behavior(), &reached, Some(&schema))
                        else {
                            continue;
                        };
                        for pointer in LOCKED.iter().chain(&[GHOST]) {
                            if read.pointer(pointer) != original.pointer(pointer) {
                                failures.push(format!(
                                    "{tag}: {what}, then repaired: {pointer} changed"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}\n({tampered} tampering edits went through while broken)",
        failures.len()
    );
}

/// The repair that makes a large text parse is checked in a moment, and so
/// is each edit before it, in every language.
#[test]
fn repairs_of_a_large_text_are_checked_quickly() {
    let mut slow = Vec::new();
    for (language, text) in documents() {
        let (good, pairs) = large(language, text, 3000, 6);
        for from in [From::Schema, From::Paths] {
            let mut shown = broken(&good, &pairs);
            let mut protected = locks(language, &shown, from, true, None);
            for (good, bad) in &pairs {
                let repaired = shown.replacen(bad, good, 1);
                let (took, ok) = crate::editor::test_timing::best_of(2, || {
                    allowed(language, &shown, &repaired, &protected)
                });
                if !ok || took > std::time::Duration::from_millis(400) {
                    slow.push(format!(
                        "{language} ({from:?}) repairing {bad:?}: {took:?}, allowed {ok}"
                    ));
                }
                let previous = std::mem::take(&mut protected);
                protected = locks(language, &repaired, from, true, Some(&previous));
                shown = repaired;
            }
        }
    }
    assert!(slow.is_empty(), "{slow:#?}");
}
