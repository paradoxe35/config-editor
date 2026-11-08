//! Locks whatever order the text and the locks arrive in: a text from the
//! host that never parsed (at mount, or a new value), locks the host names
//! while the text does not parse, the schema's marks and the host's paths,
//! blocks and single values, and a path no text has. Both ways: the locked
//! values are never changed in any text the editor sends (I1), and a repair
//! that keeps every locked value is never refused (I2).

use super::super::Language;
use super::super::codec;
use super::super::paths::FieldPath;
use super::super::work_count::{Work, counted};
use super::broken_tests::breaks;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use super::tests::{PIECES, Random, WORDS, documents, edits, schema, words};

/// Where the locks come from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum From {
    /// The schema's `readOnly` (`/id` a value, `/meta` a block).
    Schema,
    /// The host's paths, the same two fields.
    Paths,
}

pub(super) const LOCKED: [&str; 2] = ["/id", "/meta"];

/// A path the host names that no text here has.
pub(super) const GHOST: &str = "/ghost";

pub(super) fn locks(
    language: Language,
    text: &str,
    from: From,
    paths_given: bool,
    previous: Option<&Protected>,
) -> Protected {
    let schema = schema();
    let paths: Vec<FieldPath> = if from == From::Paths && paths_given {
        LOCKED
            .iter()
            .chain(&[GHOST])
            .map(|pointer| FieldPath::from(*pointer))
            .collect()
    } else {
        Vec::new()
    };
    protect(
        language.behavior(),
        text,
        Sources {
            marking_schema: (from == From::Schema).then_some(&schema),
            reading_schema: Some(&schema),
            paths: &paths,
            previous,
            ..Sources::default()
        },
    )
}

/// Every way the documents are broken here: [`breaks`], breaks right next
/// to the locked fields (a list opened on the line above one, a trailing
/// comma, an element left open), a stray line, and in JSON the object
/// holding them left unclosed (its repair closes it; see `repair_tests` for
/// blocks left open in every language).
fn all_breaks(language: Language) -> Vec<(&'static str, &'static str)> {
    let mut list = breaks(language);
    list.push(match language.label() {
        "XML" => ("<port>", "<<<\n<port>"),
        "YAML" => ("port:", "%%%\nport:"),
        _ => ("port", "%%%\nport"),
    });
    list.extend(match language.label() {
        "YAML" => vec![("meta:\n", "q: [\nmeta:\n"), ("port: 80", "port: \"80")],
        "JSON" => vec![
            ("\"port\": 80", "\"port\": 80,"),
            ("  \"id\"", "  \"q\": [\n  \"id\""),
            ("  \"port\": 80\n}\n", "  \"port\": 80\n"),
        ],
        "TOML" => vec![
            ("[meta]", "q = [\n[meta]"),
            ("port = 80", "port = 80\n[meta"),
        ],
        "HCL" => vec![("meta {", "q = [\nmeta {"), ("port = 80", "port = {")],
        _ => vec![
            ("<meta>", "<q>\n<meta>"),
            ("<port>80</port>", "<port>80</port>\n<q>"),
        ],
    });
    list
}

pub(super) fn allowed(
    language: Language,
    before: &str,
    after: &str,
    protected: &Protected,
) -> bool {
    let schema = schema();
    check(language.behavior(), Some(&schema), protected, before, after).is_ok()
}

/// One broken text and its locks.
struct Case<'a> {
    language: Language,
    from: From,
    /// The document, and the break made in it (`good` → `bad`).
    text: &'a str,
    good: &'a str,
    bad: &'a str,
    /// Whether every edit changing a locked field is refused at once (a
    /// break the locks are found around); else one may go through while
    /// the text does not parse, and the repair after it is refused.
    at_once: bool,
}

/// The edits changing a locked field are refused on the broken text with
/// the locks `protected` (at once, or when repaired); the repair goes
/// through.
fn refusals(case: &Case<'_>, protected: &Protected, tag: &str, failures: &mut Vec<String>) {
    let Case {
        language,
        from,
        text,
        good,
        bad,
        at_once,
    } = *case;
    let schema = schema();
    let original = codec::read(language.behavior(), text, Some(&schema)).unwrap();
    let broken = text.replacen(good, bad, 1);
    if codec::read(language.behavior(), &broken, Some(&schema)).is_ok() {
        failures.push(format!("{tag}: the break parses"));
        return;
    }
    for (what, edited, refused) in edits(language, &broken) {
        let Some(edited) = edited else { continue };
        if !refused || !allowed(language, &broken, &edited, protected) {
            continue;
        }
        if at_once {
            failures.push(format!("{tag}: {what} allowed: {edited:?}"));
            continue;
        }
        let carried = locks(language, &edited, from, true, Some(protected));
        let repaired = edited.replacen(bad, good, 1);
        let changed =
            codec::read(language.behavior(), &repaired, Some(&schema)).is_ok_and(|read| {
                LOCKED
                    .iter()
                    .any(|pointer| read.pointer(pointer) != original.pointer(pointer))
            });
        if changed && allowed(language, &edited, &repaired, &carried) {
            failures.push(format!(
                "{tag}: {what} allowed, and its repair: {repaired:?}"
            ));
        }
    }
    if !allowed(language, &broken, text, protected) {
        failures.push(format!("{tag}: the repair was refused"));
    }
}

/// Each document broken each way, and whether the locks hold at once.
fn cases() -> Vec<(Language, &'static str, &'static str, &'static str, bool)> {
    let mut list = Vec::new();
    for (language, text) in documents() {
        let found_around = breaks(language);
        for (good, bad) in all_breaks(language) {
            list.push((
                language,
                text,
                good,
                bad,
                found_around.contains(&(good, bad)),
            ));
        }
    }
    list
}

#[test]
fn a_text_that_never_parsed_is_locked() {
    let mut failures = Vec::new();
    for from in [From::Schema, From::Paths] {
        for (language, text, good, bad, at_once) in cases() {
            let case = Case {
                language,
                from,
                text,
                good,
                bad,
                at_once,
            };
            let broken = text.replacen(good, bad, 1);
            let protected = locks(language, &broken, from, true, None);
            let tag = format!("{language} ({from:?}, {bad:?}) from the host");
            refusals(&case, &protected, &tag, &mut failures);
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}

#[test]
fn paths_given_while_the_text_does_not_parse_lock() {
    let mut failures = Vec::new();
    for (language, text, good, bad, at_once) in cases() {
        let case = Case {
            language,
            from: From::Paths,
            text,
            good,
            bad,
            at_once,
        };
        let broken = text.replacen(good, bad, 1);
        let before = locks(language, text, From::Paths, false, None);
        let carried = locks(language, &broken, From::Paths, false, Some(&before));
        let given = locks(language, &broken, From::Paths, true, Some(&carried));
        let tag = format!("{language} ({bad:?}) paths given while broken");
        refusals(&case, &given, &tag, &mut failures);
        // Given on a text from the host that never parsed, after it.
        let fresh = locks(language, &broken, From::Paths, false, None);
        let given = locks(language, &broken, From::Paths, true, Some(&fresh));
        let tag = format!("{language} ({bad:?}) paths given on a host text that never parsed");
        refusals(&case, &given, &tag, &mut failures);
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}

/// I2 on one text of a session: each repair of a break it holds (once)
/// that leaves every locked value (and the path no text has) as the host's
/// document had them goes through.
fn repairs_go_through(
    language: Language,
    shown: &str,
    protected: &Protected,
    original: &serde_json::Value,
) -> Vec<String> {
    let schema = schema();
    all_breaks(language)
        .into_iter()
        .filter(|(_, bad)| shown.matches(bad).count() == 1)
        .map(|(good, bad)| shown.replacen(bad, good, 1))
        .filter(|repaired| {
            codec::read(language.behavior(), repaired, Some(&schema)).is_ok_and(|read| {
                LOCKED
                    .iter()
                    .chain(&[GHOST])
                    .all(|pointer| read.pointer(pointer) == original.pointer(pointer))
            })
        })
        .filter(|repaired| !allowed(language, shown, repaired, protected))
        .collect()
}

/// One step of a session.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// The host gives a text (at mount, or a new value): the original or
    /// one of its broken forms.
    Host(usize),
    /// The host names the read-only paths.
    Paths,
    /// The user breaks or repairs the syntax.
    Break,
    Repair,
    /// One of the listed edits.
    Listed,
    /// A word replaced, or a piece typed anywhere.
    Random,
}

/// Random sessions in every language, the text and the locks arriving in
/// any order: every text the editor sends that parses (and each repair of
/// its break it would send next) holds the locked values as the host had them
/// (I1), and every repair keeping them goes through (I2).
#[test]
fn locked_values_hold_whatever_order_text_and_locks_arrive_in() {
    let schema = schema();
    let mut failures = Vec::new();
    let mut sent_parsing = 0;
    let mut refused = Vec::new();
    let mut refused_repairs = 0;
    let mut random = Random(0x0bad_cafe_1234_4321);
    for from in [From::Schema, From::Paths] {
        for (language, text) in documents() {
            let original = codec::read(language.behavior(), text, Some(&schema)).unwrap();
            let pairs = all_breaks(language);
            let hosts: Vec<String> = std::iter::once(text.to_string())
                .chain(pairs.iter().map(|(good, bad)| text.replacen(good, bad, 1)))
                .collect();
            for session in 0..150 {
                let mut paths_given = false;
                let mut shown = hosts[random.below(hosts.len())].clone();
                let mut protected = locks(language, &shown, from, paths_given, None);
                let mut trail = vec![format!("host {shown:?}")];
                for _ in 0..40 {
                    let locked = from == From::Schema || paths_given;
                    for repaired in repairs_go_through(language, &shown, &protected, &original) {
                        refused_repairs += 1;
                        if refused.len() < 5 {
                            refused.push(format!(
                                "{language} ({from:?}) session {session}: refused {repaired:?}\n{}",
                                trail.join("\n")
                            ));
                        }
                    }
                    let step = match random.below(10) {
                        0 => Step::Host(random.below(hosts.len())),
                        1 | 2 => Step::Paths,
                        3 => Step::Break,
                        4 => Step::Repair,
                        5 | 6 => Step::Listed,
                        _ => Step::Random,
                    };
                    let edited = match step {
                        Step::Host(index) => {
                            shown = hosts[index].clone();
                            protected = locks(language, &shown, from, paths_given, None);
                            trail.push(format!("host {shown:?}"));
                            continue;
                        }
                        Step::Paths => {
                            if from == From::Paths && !paths_given {
                                paths_given = true;
                                let previous = std::mem::take(&mut protected);
                                protected = locks(language, &shown, from, true, Some(&previous));
                                trail.push("paths given".to_string());
                            }
                            continue;
                        }
                        Step::Break => {
                            let (good, bad) = pairs[random.below(pairs.len())];
                            shown.contains(good).then(|| shown.replacen(good, bad, 1))
                        }
                        Step::Repair => pairs
                            .iter()
                            .find(|(_, bad)| shown.contains(bad))
                            .map(|(good, bad)| shown.replacen(bad, good, 1)),
                        Step::Listed => {
                            let list: Vec<(&str, Option<String>, bool)> = edits(language, &shown)
                                .into_iter()
                                .filter(|(_, _, touches)| locked || !touches)
                                .collect();
                            list.get(random.below(list.len()))
                                .and_then(|(_, edited, _)| edited.clone())
                        }
                        // Before the locks come, only edits that keep the
                        // locked fields as they are.
                        Step::Random if !locked => None,
                        Step::Random => {
                            let units: Vec<u16> = shown.encode_utf16().collect();
                            let (start, end, piece) = if random.below(3) != 0 {
                                let words = words(&units);
                                let (start, end) = words[random.below(words.len())];
                                (start, end, WORDS[random.below(WORDS.len())])
                            } else {
                                let start = random.below(units.len() + 1);
                                let end = (start + random.below(7)).min(units.len());
                                (start, end, PIECES[random.below(PIECES.len())])
                            };
                            let mut edited = String::from_utf16_lossy(&units[..start]);
                            edited.push_str(piece);
                            edited.push_str(&String::from_utf16_lossy(&units[end..]));
                            Some(edited)
                        }
                    };
                    let Some(edited) = edited else { continue };
                    if edited == shown || !allowed(language, &shown, &edited, &protected) {
                        continue;
                    }
                    trail.push(format!("sent {edited:?}"));
                    let previous = std::mem::take(&mut protected);
                    protected = locks(language, &edited, from, paths_given, Some(&previous));
                    // And each repair the editor would send next.
                    let repaired: Vec<String> = pairs
                        .iter()
                        .filter(|(_, bad)| edited.matches(bad).count() == 1)
                        .map(|(good, bad)| edited.replacen(bad, good, 1))
                        .filter(|repaired| allowed(language, &edited, repaired, &protected))
                        .collect();
                    for candidate in std::iter::once(&edited).chain(&repaired) {
                        let Ok(read) = codec::read(language.behavior(), candidate, Some(&schema))
                        else {
                            continue;
                        };
                        sent_parsing += 1;
                        if !locked {
                            continue;
                        }
                        for pointer in LOCKED {
                            if read.pointer(pointer) != original.pointer(pointer) {
                                failures.push(format!(
                                    "{language} ({from:?}) session {session}: {pointer} changed in {candidate:?}\n{}",
                                    trail.join("\n")
                                ));
                            }
                        }
                    }
                    shown = edited;
                }
                if failures.len() > 10 {
                    break;
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures, the first:\n{}",
        failures.len(),
        failures[..failures.len().min(5)].join("\n\n")
    );
    assert!(sent_parsing > 1000, "only {sent_parsing} texts parsed");
    assert!(
        refused.is_empty(),
        "{refused_repairs} repairs keeping the locked values refused, the first:\n{}",
        refused.join("\n\n")
    );
}

/// A large text from the host that does not parse is read for its locks in
/// a moment, in every language (each break, and a second one near the top).
#[test]
fn a_large_text_that_never_parsed_is_locked_quickly() {
    let mut slow = Vec::new();
    // Past the lines read one by one, and just under them.
    for (language, text, size) in documents()
        .into_iter()
        .flat_map(|(language, text)| [(language, text, 3000), (language, text, 1400)])
    {
        let filler: String = (0..size)
            .map(|index| match language.label() {
                "YAML" => format!("k{index}: {index}\n"),
                "JSON" => format!("  \"k{index}\": {index},\n"),
                "XML" => format!("<k{index}>{index}</k{index}>\n"),
                _ => format!("k{index} = {index}\n"),
            })
            .collect();
        let large = match language.label() {
            "JSON" => text.replacen("{\n", &format!("{{\n{filler}"), 1),
            _ => format!("{filler}{text}"),
        };
        for (good, bad) in all_breaks(language) {
            let broken = large.replacen(good, bad, 1);
            let twice = broken
                .replacen("k1: 1\n", "k1: [\n", 1)
                .replacen("k1 = 1\n", "k1 = [\n", 1)
                .replacen("\"k1\": 1,", "\"k1\": [", 1)
                .replacen("<k1>1</k1>", "<k1>1</k", 1);
            for text in [&broken, &twice] {
                for from in [From::Schema, From::Paths] {
                    // A few lines' paths read (each a read from the top),
                    // not every line's: read line by line, thousands.
                    let (_, lookups) =
                        counted(Work::PathLookup, || locks(language, text, from, true, None));
                    if lookups > 50 {
                        slow.push(format!(
                            "{language} {size} ({from:?}, {bad:?}): {lookups} path lookups"
                        ));
                    }
                    // The stand-in's parses: up to about 150 ms here.
                    let (took, _) = crate::editor::test_timing::best_of(2, || {
                        locks(language, text, from, true, None)
                    });
                    if took > std::time::Duration::from_millis(700) {
                        slow.push(format!("{language} {size} ({from:?}, {bad:?}): {took:?}"));
                    }
                }
            }
        }
    }
    assert!(slow.is_empty(), "{slow:#?}");
}
