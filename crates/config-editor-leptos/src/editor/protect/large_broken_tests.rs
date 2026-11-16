//! Large texts that do not parse, broken in more places than a stand-in
//! mends and longer than a text read line by line, in every language and
//! arrival order: an edit changing a locked value never reaches a text that
//! parses (I1), and repairing each break, which keeps every locked value,
//! always goes through (I2).

use super::super::Language;
use super::super::codec;
use super::super::work_count::{Work, counted, whole_reads};
use super::arrival_tests::{From, GHOST, LOCKED, allowed, locks};
use super::guard::check;
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
                        let (reached, _) =
                            repair_all(language, edited, carried.clone(), &pairs, from);
                        let Ok(read) = codec::read(language.behavior(), &reached, Some(&schema))
                        else {
                            continue;
                        };
                        // A field reading otherwise is held: never saved so.
                        let held = locks(language, &reached, from, true, Some(&carried)).held;
                        for pointer in LOCKED.iter().chain(&[GHOST]) {
                            if read.pointer(pointer) != original.pointer(pointer)
                                && !held.iter().any(|known| known == pointer)
                            {
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

/// The most whole reads (parses, path lookups from the top, stand-ins) a
/// repair's check makes beyond finding the locks of the repaired text,
/// which the check does for the editor (it then holds those locks): a read
/// per line of the 3000-line text is thousands.
const REPAIR_READS: usize = 64;

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
                let (ok, reads) = whole_reads(&repaired, || {
                    allowed(language, &shown, &repaired, &protected)
                });
                let (_, placing) = whole_reads(&repaired, || {
                    locks(language, &repaired, from, true, Some(&protected))
                });
                if !ok || reads > placing + REPAIR_READS {
                    slow.push(format!(
                        "{language} ({from:?}) repairing {bad:?}: {reads} whole reads ({placing} placing the locks), allowed {ok}"
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

/// `text` without `count` of its lines, from the one starting with `from`.
fn without_lines(text: &str, from: &str, count: usize) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let start = lines
        .iter()
        .position(|line| line.trim_start().starts_with(from))
        .unwrap();
    let end = start.saturating_add(count).min(lines.len());
    lines[..start].concat() + &lines[end..].concat()
}

/// Lines a deletion of unrelated lines, or the repair after it, reads for
/// their paths (each a read from the top of the text) at most, however
/// many were deleted: those near a locked field and those naming one of
/// its keys (reading every deleted line took a read per line).
const LINES_READ: usize = 50;

/// Many lines deleted from a large broken text, none holding a locked
/// field, then the break repaired: the deletion and the repair go through,
/// each reading only the lines that may hold a locked field (I2). Deleted
/// with the locked fields among them, it is refused naming one (I1).
#[test]
fn deleting_many_unrelated_lines_then_repairing_goes_through() {
    let strings = crate::EditorStrings::default();
    let schema = schema();
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let (good, pairs) = large(language, text, 3000, 1);
        let (fixed, bad) = (&pairs[0].0, &pairs[0].1);
        let first = match language.label() {
            "JSON" => "\"k100\"",
            "XML" => "<k100>",
            "YAML" => "k100:",
            _ => "k100 ",
        };
        let check = |protected: &Protected, before: &str, after: &str| {
            check(language.behavior(), Some(&schema), protected, before, after)
        };
        for from in [From::Schema, From::Paths] {
            let shown = broken(&good, &pairs);
            let protected = locks(language, &shown, from, true, None);
            for count in [200, 500, 1000, 2500] {
                let tag = format!("{language} ({from:?}) {count} lines deleted");
                let deleted = without_lines(&shown, first, count);
                let (deletion, lookups) = counted(Work::PathLookup, || {
                    allowed(language, &shown, &deleted, &protected)
                });
                if !deletion {
                    failures.push(format!("{tag}: the deletion refused"));
                    continue;
                }
                if lookups > LINES_READ {
                    failures.push(format!("{tag}: the deletion read {lookups} lines"));
                }
                let carried = locks(language, &deleted, from, true, Some(&protected));
                let repaired = deleted.replacen(bad, fixed, 1);
                assert!(
                    codec::read(language.behavior(), &repaired, None).is_ok(),
                    "{tag}"
                );
                let (result, lookups) =
                    counted(Work::PathLookup, || check(&carried, &deleted, &repaired));
                if let Err(blocked) = result {
                    let message = blocked.message(&strings);
                    failures.push(format!("{tag}: the repair refused: {message}"));
                }
                if lookups > LINES_READ {
                    failures.push(format!("{tag}: the repair read {lookups} lines"));
                }
            }
            // The locked fields deleted with the lines before them: the
            // deletion changes their text, refused at once.
            let tag = format!("{language} ({from:?}) deleted through the locked fields");
            let deleted = without_lines(&shown, first, usize::MAX);
            if allowed(language, &shown, &deleted, &protected) {
                failures.push(format!("{tag}: the deletion went through"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}

/// `filler` lines whose values name the locked keys (`id`, `meta`): a
/// large text's line scan reads each of them as it reads the locked
/// fields' own lines.
fn naming_filler(language: Language, index: usize) -> (String, String) {
    let (good, bad) = filler(language, index);
    let name = |line: String| match language.label() {
        "YAML" | "JSON" => line.replacen(&format!(" {index}"), " \"idmeta\"", 1),
        "XML" => line.replacen(&format!(">{index}<"), ">idmeta<", 1),
        _ => line.replacen(&format!("= {index}"), "= \"idmeta\"", 1),
    };
    (name(good), name(bad))
}

/// A copy of the locked `id` line with another value: the text parses to
/// it where the language lets a later duplicate win (or makes the two a
/// list).
fn copied_id(language: Language) -> (&'static str, &'static str) {
    match language.label() {
        "YAML" => ("id: abc\n", "id: abc\nid: evil\n"),
        "JSON" => (
            "  \"id\": \"abc\",\n",
            "  \"id\": \"abc\",\n  \"id\": \"evil\",\n",
        ),
        "XML" => ("<id>abc</id>\n", "<id>abc</id>\n<id>evil</id>\n"),
        _ => ("id = \"abc\"\n", "id = \"abc\"\nid = \"evil\"\n"),
    }
}

/// P1 in a large text broken in more places than a stand-in mends, with
/// hundreds of lines naming a locked key before the locked fields: a
/// locked value changed, or its line copied with another value, while
/// broken never reaches a text that parses unless that field is held.
#[test]
fn a_locked_key_copied_below_many_lines_naming_it_holds_the_save() {
    let schema = schema();
    let mut failures = Vec::new();
    let mut tampered = 0;
    for (language, text) in documents() {
        let original = codec::read(language.behavior(), text, Some(&schema)).unwrap();
        let (size, breaks) = (800, 5);
        let lines: String = (0..size)
            .map(|index| format!("{}\n", naming_filler(language, index).0))
            .collect();
        let good = match language.label() {
            "JSON" => text.replacen("{\n", &format!("{{\n{lines}"), 1),
            _ => format!("{lines}{text}"),
        };
        assert!(codec::read(language.behavior(), &good, Some(&schema)).is_ok());
        let pairs: Vec<(String, String)> = (0..breaks)
            .map(|at| naming_filler(language, 1 + at * (size - 2) / breaks))
            .map(|(good, bad)| (format!("{good}\n"), format!("{bad}\n")))
            .collect();
        // The line scan reaches the locked fields below every line naming
        // their keys.
        let broken_text = broken(&good, &pairs);
        let scanned = super::scan::written(
            language.behavior(),
            &broken_text,
            &super::locate::LineIndex::new(&broken_text),
            None,
            &["id".to_string(), "meta".to_string()],
        );
        for locked in LOCKED {
            assert!(
                scanned.iter().any(|field| field.pointer == locked),
                "{language}: the scan missed {locked}"
            );
        }
        let (id_line, copied) = copied_id(language);
        for from in [From::Schema, From::Paths] {
            for arrival in [Arrival::Broken, Arrival::PathsAfter, Arrival::UserBreaks] {
                if matches!(arrival, Arrival::PathsAfter) && matches!(from, From::Schema) {
                    continue;
                }
                let tag = format!("{language} ({from:?}, {arrival:?})");
                let (shown, protected) =
                    arrive(language, &good, &pairs, from, arrival, &mut failures);
                let copy = shown.replacen(id_line, copied, 1);
                let tampers = edits(language, &shown)
                    .into_iter()
                    .filter(|(what, _, _)| TAMPERS.contains(what))
                    .filter_map(|(what, edited, _)| Some((what, edited?)))
                    .chain([("copy the locked line with another value", copy)]);
                for (what, edited) in tampers {
                    if !allowed(language, &shown, &edited, &protected) {
                        continue;
                    }
                    tampered += 1;
                    let carried = locks(language, &edited, from, true, Some(&protected));
                    let (reached, _) = repair_all(language, edited, carried.clone(), &pairs, from);
                    let Ok(read) = codec::read(language.behavior(), &reached, Some(&schema)) else {
                        continue;
                    };
                    let held = locks(language, &reached, from, true, Some(&carried)).held;
                    for pointer in LOCKED {
                        if read.pointer(pointer) != original.pointer(pointer)
                            && !held.iter().any(|known| known == pointer)
                        {
                            failures.push(format!(
                                "{tag}: {what}, then repaired: {pointer} reads {:?}",
                                read.pointer(pointer)
                            ));
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
