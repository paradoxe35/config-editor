//! Locks while the document does not parse: a syntax error anywhere (before
//! or after a locked block) keeps every locked stretch refused, the block's
//! contents included, for schema marks and host paths alike, and an edit
//! repairing the syntax outside the lock goes through.

use serde_json::Value;

use super::super::Language;
use super::super::codec;
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use super::tests::{documents, edits, schema};

/// Ways to break each document outside its locks: an unclosed quote above
/// the locked fields, and a bracket left open below them.
fn breaks(language: Language) -> Vec<(&'static str, &'static str)> {
    match language.label() {
        "YAML" => vec![("name: x", "name: \"x"), ("port: 80", "port: [80")],
        "JSON" => vec![("\"name\": \"x\"", "\"name\": \"x"), ("}\n", "}\n}\n")],
        "TOML" => vec![("name = \"x\"", "name = \"x"), ("port = 80", "port = [80")],
        "HCL" => vec![("name = \"x\"", "name = \"x"), ("port = 80", "port = [80")],
        _ => vec![
            ("<name>x</name>", "<name>x</nam"),
            ("<port>80</port>", "<port>80</port>\n<"),
        ],
    }
}

/// How the locks come: from the schema's marks, or from the host's paths
/// (no schema).
#[derive(Clone, Copy, Debug)]
enum From {
    Schema,
    Paths,
}

fn locks(language: Language, text: &str, from: From, previous: Option<&Protected>) -> Protected {
    let schema = schema();
    let paths = [FieldPath::from("/id"), FieldPath::from("/meta")];
    let (marking, paths): (Option<&Value>, &[FieldPath]) = match from {
        From::Schema => (Some(&schema), &[]),
        From::Paths => (None, &paths),
    };
    protect(
        language.behavior(),
        text,
        Sources {
            marking_schema: marking,
            reading_schema: Some(&schema),
            paths,
            previous,
            ..Sources::default()
        },
    )
}

fn allowed(language: Language, before: &str, after: &str, protected: &Protected) -> bool {
    let schema = schema();
    check(language.behavior(), Some(&schema), protected, before, after).is_ok()
}

#[test]
fn locks_hold_while_the_document_does_not_parse() {
    let schema = schema();
    let mut failures = Vec::new();
    for from in [From::Schema, From::Paths] {
        for (language, text) in documents() {
            let fresh = locks(language, text, from, None);
            for (good, bad) in breaks(language) {
                let broken = text.replacen(good, bad, 1);
                let tag = format!("{language} ({from:?}, {bad:?})");
                if codec::read(language.behavior(), &broken, Some(&schema)).is_ok() {
                    failures.push(format!("{tag}: the break still parses"));
                    continue;
                }
                if !allowed(language, text, &broken, &fresh) {
                    failures.push(format!("{tag}: breaking the syntax was refused"));
                    continue;
                }
                let carried = locks(language, &broken, from, Some(&fresh));
                // Twice broken: the locks carried over two texts.
                let again = locks(language, &broken, from, Some(&carried));
                for (protected, round) in [(&carried, "once"), (&again, "twice")] {
                    for (what, edited, refused) in edits(language, &broken) {
                        let Some(edited) = edited else { continue };
                        if allowed(language, &broken, &edited, protected) == refused {
                            failures.push(format!(
                                "{tag}, carried {round}: {what}: {} {edited:?}",
                                if refused { "allowed" } else { "refused" }
                            ));
                        }
                    }
                    if !allowed(language, &broken, text, protected) {
                        failures.push(format!("{tag}, carried {round}: the repair was refused"));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}

/// Random edits on a broken document, the locks carried as the editor
/// carries them: whenever the text parses, or would once the break is
/// repaired, the locked values are the ones the document had.
#[test]
fn random_edits_on_a_broken_document_never_change_a_locked_value() {
    use super::tests::{PIECES, Random, WORDS, words};
    let schema = schema();
    let mut failures = Vec::new();
    let mut checked = 0;
    let mut allowed_count = 0;
    let mut random = Random(0x2545_f491_4f6c_dd1d);
    for from in [From::Schema, From::Paths] {
        for (language, text) in documents() {
            let original = codec::read(language.behavior(), text, Some(&schema)).unwrap();
            let fresh = locks(language, text, from, None);
            for (good, bad) in breaks(language) {
                let broken = text.replacen(good, bad, 1);
                let mut shown = broken.clone();
                let mut protected = locks(language, &shown, from, Some(&fresh));
                for step in 0..2000 {
                    if step % 20 == 0 {
                        shown = broken.clone();
                        protected = locks(language, &shown, from, Some(&fresh));
                    }
                    let units: Vec<u16> = shown.encode_utf16().collect();
                    // Mostly a word replaced by another, which keeps the
                    // document repairable, so the values are compared.
                    let (start, end, piece) = if random.below(4) != 0 {
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
                    if !allowed(language, &shown, &edited, &protected) {
                        continue;
                    }
                    allowed_count += 1;
                    let repaired =
                        (edited.matches(bad).count() == 1).then(|| edited.replacen(bad, good, 1));
                    for candidate in [Some(&edited), repaired.as_ref()].into_iter().flatten() {
                        let Ok(read) = codec::read(language.behavior(), candidate, Some(&schema))
                        else {
                            continue;
                        };
                        checked += 1;
                        for pointer in ["/id", "/meta"] {
                            if read.pointer(pointer) != original.pointer(pointer) {
                                failures.push(format!(
                                    "{language} ({from:?}, {bad:?}), step {step}: {pointer} changed by {piece:?} at {start}..{end}:\n{shown}\n→\n{edited}"
                                ));
                            }
                        }
                    }
                    let previous = std::mem::take(&mut protected);
                    protected = locks(language, &edited, from, Some(&previous));
                    shown = edited;
                }
                if failures.len() > 20 {
                    break;
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert!(checked > 1000, "checked {checked}, allowed {allowed_count}");
}
