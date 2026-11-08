//! A locked value written over several lines while the text does not
//! parse: every line of it stays locked (not only its first), and the
//! whitespace inside it is content, never free formatting. Every way each
//! language writes a value over lines, the text broken from the start (the
//! host gave it so) and broken by an edit after it parsed.

use super::super::Language;
use super::super::codec;
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Protected, Sources, protect};

/// A text per way of writing `/b3` over lines, the piece of its last line
/// to edit, and a line that breaks the text after it.
fn cases() -> Vec<(Language, &'static str, &'static str, &'static str)> {
    vec![
        (
            Language::JSON,
            "{\n  \"b3\": [\n    1,\n    2\n  ],\n  \"c\": 1\n}\n",
            "2",
            "@@@\n",
        ),
        (
            Language::TOML,
            "b3 = \"\"\"\nl1\nl2\"\"\"\nc = 1\n",
            "l2",
            "@@@\n",
        ),
        (
            Language::TOML,
            "b3 = '''\nl1\nl2'''\nc = 1\n",
            "l2",
            "@@@\n",
        ),
        (
            Language::TOML,
            "b3 = \"\"\"l0\nl1 \\\n  l2\"\"\"\nc = 1\n",
            "l2",
            "@@@\n",
        ),
        (
            Language::YAML,
            "b3: \"l1\n  l2\"\nc: 1\n",
            "l2",
            "@@@ : : x\n",
        ),
        (
            Language::YAML,
            "b3: 'l1\n  l2'\nc: 1\n",
            "l2",
            "@@@ : : x\n",
        ),
        (Language::YAML, "b3: l1\n  l2\nc: 1\n", "l2", "@@@ : : x\n"),
        (
            Language::YAML,
            "b3: |\n  l1\n  l2\nc: 1\n",
            "l2",
            "@@@ : : x\n",
        ),
        (
            Language::YAML,
            "b3: >\n  l1\n  l2\nc: 1\n",
            "l2",
            "@@@ : : x\n",
        ),
        (Language::XML, "<b3>l1\nl2</b3>\n<c>1</c>\n", "l2", "<<\n"),
        (
            Language::XML,
            "<b3>\nl1\nl2\n</b3>\n<c>1</c>\n",
            "l2",
            "<<\n",
        ),
        (
            Language::HCL,
            "b3 = <<EOT\nl1\nl2\nEOT\nc = 1\n",
            "l2",
            "@@@ @@@\n",
        ),
        (
            Language::HCL,
            "b3 = <<-EOT\n  l1\n  l2\n  EOT\nc = 1\n",
            "l2",
            "@@@ @@@\n",
        ),
    ]
}

fn locks(language: Language, text: &str, previous: Option<&Protected>) -> Protected {
    let paths = [FieldPath::from("/b3")];
    protect(
        language.behavior(),
        text,
        Sources {
            paths: &paths,
            previous,
            ..Sources::default()
        },
    )
}

/// The ways `text` is broken: as given (locks read from it), and by
/// appending `junk` after it parsed (locks carried through the edit).
fn broken(language: Language, text: &str, junk: &str) -> Vec<(&'static str, String, Protected)> {
    let broken = format!("{text}{junk}");
    assert!(
        codec::read(language.behavior(), &broken, None).is_err(),
        "{language}: {broken:?} parses"
    );
    let fresh = locks(language, text, None);
    vec![
        (
            "given broken",
            broken.clone(),
            locks(language, &broken, None),
        ),
        (
            "broken by an edit",
            broken.clone(),
            locks(language, &broken, Some(&fresh)),
        ),
    ]
}

fn allowed(language: Language, before: &str, after: &str, protected: &Protected) -> bool {
    check(language.behavior(), None, protected, before, after).is_ok()
}

/// Edits on the value's last line (`piece`): a letter, a space and a line
/// break typed inside it, a space and a tab after it.
fn edits(text: &str, piece: &str) -> Vec<(String, String)> {
    let at = text.find(piece).expect("the piece");
    let end = at + piece.len();
    let mut edits: Vec<(String, String)> = ["Z", " ", "\n"]
        .iter()
        .map(|typed| {
            (
                format!("{typed:?} typed inside {piece:?}"),
                format!("{}{typed}{}", &text[..at + 1], &text[at + 1..]),
            )
        })
        .collect();
    for typed in [" ", "\t"] {
        edits.push((
            format!("{typed:?} typed after {piece:?}"),
            format!("{}{typed}{}", &text[..end], &text[end..]),
        ));
    }
    edits
}

#[test]
fn every_line_of_a_locked_multiline_value_stays_locked_while_broken() {
    let mut failures = Vec::new();
    for (language, text, piece, junk) in cases() {
        let original = codec::read(language.behavior(), text, None).unwrap();
        let locked = original.pointer("/b3").cloned();
        for (how, broken, protected) in broken(language, text, junk) {
            for (what, edited) in edits(&broken, piece) {
                // An edit leaving the value as it was (the text repaired
                // afterwards reads the same) may go through.
                let repaired = edited.replacen(junk, "", 1);
                let same = codec::read(language.behavior(), &repaired, None)
                    .is_ok_and(|read| read.pointer("/b3").cloned() == locked);
                if !same && allowed(language, &broken, &edited, &protected) {
                    failures.push(format!("{language} {text:?} ({how}): {what} allowed"));
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

#[test]
fn a_locked_multiline_value_is_locked_on_every_line_while_broken() {
    let mut failures = Vec::new();
    for (language, text, _, junk) in cases() {
        let lines = text.lines().count();
        for (how, broken, protected) in broken(language, text, junk) {
            let covered: Vec<usize> = (0..lines)
                .filter(|line| {
                    let start: usize = broken
                        .split_inclusive('\n')
                        .take(*line)
                        .map(|line| line.encode_utf16().count())
                        .sum();
                    let width = broken.lines().nth(*line).unwrap().encode_utf16().count();
                    (start..start + width).any(|at| protected.lock_at(at).is_some())
                })
                .collect();
            // Every line of the value: from its key's to the last one
            // before `c`'s (an XML end tag's line left out).
            let first = text.lines().position(|line| line.contains("b3")).unwrap();
            let last = text
                .lines()
                .position(|line| line.contains('c') || line.starts_with("</"))
                .unwrap();
            let expected: Vec<usize> = (first..last).collect();
            if !expected.iter().all(|line| covered.contains(line)) {
                failures.push(format!(
                    "{language} {text:?} ({how}): lines {covered:?} locked, not {expected:?}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{failures:#?}",
        failures.len()
    );
}
