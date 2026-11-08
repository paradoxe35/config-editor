//! Whitespace around and inside a locked field while the text does not
//! parse: free as it is while the text parses (a space after `:` or before
//! `=`, re-indenting where indentation carries no structure, a whole
//! document pasted reformatted), as long as it falls between tokens. Space
//! inside a string or a YAML plain value, and a YAML locked line's indent,
//! stay refused; repairing the break afterwards goes through.

use serde_json::Value;

use super::super::Language;
use super::super::codec;
use super::super::paths::FieldPath;
use super::broken_tests::breaks;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use super::tests::{documents, schema};

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

/// `from` → `to`, the first occurrence.
type Replace = (&'static str, &'static str);

/// Whitespace edits on the locked `/id` and `/meta` (`from` → `to`, first
/// occurrence), and the whole document reformatted.
fn spacing(language: Language) -> (Vec<Replace>, fn(&str) -> String) {
    match language.label() {
        "YAML" => (
            vec![
                ("id: abc", "id:  abc"),
                ("  a: 1", "  a:   1"),
                ("meta:\n", "meta:  \n"),
                ("  b: 2\n", "  b: 2\n\n"),
            ],
            |text| {
                text.split_inclusive('\n')
                    .map(|line| match line.starts_with("@@@") {
                        true => line.to_string(),
                        false => line.replace(": ", ":  "),
                    })
                    .collect()
            },
        ),
        "JSON" => (
            vec![
                ("\"id\": \"abc\"", "\"id\":  \"abc\""),
                ("  \"id\"", "    \"id\""),
                ("    \"a\": 1", "        \"a\" : 1"),
                ("\"meta\": {", "\"meta\":{"),
            ],
            |text| text.replace("\n  ", "\n    "),
        ),
        "TOML" => (
            vec![
                ("id = \"abc\"", "id  =  \"abc\""),
                ("a = 1", "a =1"),
                ("b = 2", "b=2"),
            ],
            |text| text.replace(" = ", "="),
        ),
        "HCL" => (
            vec![
                ("id = \"abc\"", "id   = \"abc\""),
                ("  a = 1", "      a = 1"),
                ("meta {", "meta  {"),
            ],
            |text| text.replace("\n  ", "\n    ").replace(" = ", "  =  "),
        ),
        _ => (
            vec![
                ("<id>abc</id>", "  <id>abc</id>"),
                ("  <a>1</a>", "      <a>1</a>"),
                ("<meta>", "<meta >"),
            ],
            |text| {
                let mut previous = '>';
                text.split_inclusive('\n')
                    .map(|line| {
                        let indented = match previous == '>' && line.starts_with('<') {
                            true => format!("  {line}"),
                            false => line.to_string(),
                        };
                        previous = line.trim_end().chars().last().unwrap_or('>');
                        indented
                    })
                    .collect()
            },
        ),
    }
}

/// Whitespace edits that change a locked value (or, in YAML, its indent).
fn changing(language: Language) -> Vec<Replace> {
    match language.label() {
        "YAML" => vec![
            ("id: abc", "id: a bc"),
            ("  a: 1", "   a: 1"),
            ("  b: 2", " b: 2"),
        ],
        "JSON" => vec![("\"abc\"", "\"ab c\""), ("\"id\"", "\"i d\"")],
        "TOML" | "HCL" => vec![("\"abc\"", "\" abc\""), ("id =", "i d =")],
        _ => vec![
            ("<id>abc</id>", "<id>a bc</id>"),
            ("<a>1</a>", "<a>1 1</a>"),
        ],
    }
}

/// `text` broken by `bad` replacing `good`, with a stray line too.
fn broken_texts(language: Language, text: &str) -> Vec<String> {
    let mut texts: Vec<String> = breaks(language)
        .into_iter()
        .map(|(good, bad)| text.replacen(good, bad, 1))
        .collect();
    let stray = match language.label() {
        "JSON" => text.replacen("  \"port\"", "  @@@\n  \"port\"", 1),
        "XML" => text.replacen("<port>", "<<<\n<port>", 1),
        _ => text.replacen("port", "@@@ : : x\nport", 1),
    };
    texts.push(stray);
    texts
}

#[test]
fn whitespace_between_tokens_of_a_locked_field_is_free_while_broken() {
    let schema = schema();
    let mut failures = Vec::new();
    for from in [From::Schema, From::Paths] {
        for (language, text) in documents() {
            let behavior = language.behavior();
            let original = codec::read(behavior, text, Some(&schema)).unwrap();
            let fresh = locks(language, text, from, None);
            let (edits, reformat) = spacing(language);
            for broken in broken_texts(language, text) {
                let tag = format!("{language} ({from:?}) {broken:?}");
                assert!(
                    codec::read(behavior, &broken, Some(&schema)).is_err(),
                    "{tag}"
                );
                let protected = locks(language, &broken, from, Some(&fresh));
                let mut spaced: Vec<(String, String)> = edits
                    .iter()
                    .map(|(from, to)| (format!("{from:?} → {to:?}"), broken.replacen(from, to, 1)))
                    .collect();
                spaced.push(("the document reformatted".into(), reformat(&broken)));
                for (what, edited) in spaced {
                    assert_ne!(edited, broken, "{tag}: {what}");
                    if !allowed(language, &broken, &edited, &protected) {
                        failures.push(format!("{tag}: {what} refused"));
                        continue;
                    }
                    // The break repaired afterwards goes through, keeping
                    // every locked value.
                    let after = locks(language, &edited, from, Some(&protected));
                    let repaired = reformat_back(text, &broken, &edited);
                    let Some(repaired) = repaired else { continue };
                    match codec::read(behavior, &repaired, Some(&schema)) {
                        Ok(read) => {
                            for pointer in ["/id", "/meta"] {
                                assert_eq!(read.pointer(pointer), original.pointer(pointer));
                            }
                        }
                        Err(_) => continue,
                    }
                    if !allowed(language, &edited, &repaired, &after) {
                        failures.push(format!("{tag}: {what}, then the repair refused"));
                    }
                }
                for (from, to) in changing(language) {
                    let edited = broken.replacen(from, to, 1);
                    if allowed(language, &broken, &edited, &protected) {
                        failures.push(format!("{tag}: {from:?} → {to:?} allowed"));
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

/// `edited` (whitespace changed in `broken`) with the break undone: the
/// lines that differ between `text` and `broken` put back, when the break
/// is still there as it was.
fn reformat_back(text: &str, broken: &str, edited: &str) -> Option<String> {
    let old: Vec<&str> = text.lines().collect();
    let bad: Vec<&str> = broken.lines().collect();
    // The break: the first line where they differ, and how many lines of
    // `broken` stand for how many of `text` there.
    let first = old
        .iter()
        .zip(&bad)
        .position(|(a, b)| a != b)
        .unwrap_or(old.len().min(bad.len()));
    let tail = old
        .iter()
        .rev()
        .zip(bad.iter().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(old.len() - first)
        .min(bad.len() - first);
    let bad_part = bad[first..bad.len() - tail].join("\n");
    let good_part = old[first..old.len() - tail].join("\n");
    if bad_part.is_empty() || edited.matches(bad_part.as_str()).count() != 1 {
        return None;
    }
    Some(edited.replacen(bad_part.as_str(), &good_part, 1))
}

/// A space, a tab or a line break put at every place of the text, or a
/// space taken out at every place: whenever the broken text takes it, and
/// then the repair of its break, every locked value reads as before.
#[test]
fn whitespace_allowed_while_broken_never_changes_a_locked_value() {
    let schema = schema();
    let mut failures = Vec::new();
    let mut allowed_count = 0;
    for from in [From::Schema, From::Paths] {
        for (language, text) in documents() {
            let behavior = language.behavior();
            let original = codec::read(behavior, text, Some(&schema)).unwrap();
            let fresh = locks(language, text, from, None);
            for broken in broken_texts(language, text) {
                let protected = locks(language, &broken, from, Some(&fresh));
                let places: Vec<usize> = broken
                    .char_indices()
                    .map(|(at, _)| at)
                    .chain([broken.len()])
                    .collect();
                for at in places {
                    let mut edits: Vec<String> = [" ", "\t", "\n", "\n  "]
                        .iter()
                        .map(|piece| format!("{}{piece}{}", &broken[..at], &broken[at..]))
                        .collect();
                    if broken[at..].starts_with(char::is_whitespace) {
                        edits.push(format!("{}{}", &broken[..at], &broken[at + 1..]));
                    }
                    for edited in edits {
                        if !allowed(language, &broken, &edited, &protected) {
                            continue;
                        }
                        allowed_count += 1;
                        let Some(repaired) = reformat_back(text, &broken, &edited) else {
                            continue;
                        };
                        let Ok(read) = codec::read(behavior, &repaired, Some(&schema)) else {
                            continue;
                        };
                        let after = locks(language, &edited, from, Some(&protected));
                        if !allowed(language, &edited, &repaired, &after) {
                            continue;
                        }
                        for pointer in ["/id", "/meta"] {
                            if read.pointer(pointer) != original.pointer(pointer) {
                                failures.push(format!(
                                    "{language} ({from:?}): {pointer} changed by\n{edited}"
                                ));
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(allowed_count > 500, "allowed {allowed_count}");
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
