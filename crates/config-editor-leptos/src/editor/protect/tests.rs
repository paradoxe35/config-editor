//! Protected fields across every language: locating read-only values,
//! refusing each kind of edit, the same locks while editing as another
//! language, and the save check, plus a property test that random edits in
//! every language pair never change a read-only value.

#![allow(
    clippy::single_range_in_vec_init,
    reason = "a lock range is a range, not a list of numbers"
)]

use std::ops::Range;

use serde_json::{Value, json};

use super::super::Language;
use super::super::bridge::Bridge;
use super::super::codec;
use super::super::diagnostics::build_schema;
use super::super::paths::FieldPath;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use super::save::{check_save, locked_pointers};

const SCHEMA: &str = r#"{
  "type": "object",
  "properties": {
    "name": {"type": "string"},
    "id": {"type": "string", "readOnly": true, "x-readonly-reason": "Set by the server"},
    "meta": {"type": "object", "readOnly": true, "properties": {"a": {"type": "integer"}, "b": {"type": "integer"}}},
    "port": {"type": "integer"}
  }
}"#;

/// The same document in every language: `id` and `meta` are read-only.
pub(super) fn documents() -> Vec<(Language, &'static str)> {
    vec![
        (
            Language::YAML,
            "name: x\nid: abc\nmeta:\n  a: 1\n  b: 2\nport: 80\n",
        ),
        (
            Language::JSON,
            "{\n  \"name\": \"x\",\n  \"id\": \"abc\",\n  \"meta\": {\n    \"a\": 1,\n    \"b\": 2\n  },\n  \"port\": 80\n}\n",
        ),
        (
            Language::TOML,
            "name = \"x\"\nid = \"abc\"\nport = 80\n\n[meta]\na = 1\nb = 2\n",
        ),
        (
            Language::HCL,
            "name = \"x\"\nid = \"abc\"\nmeta {\n  a = 1\n  b = 2\n}\nport = 80\n",
        ),
        (
            Language::XML,
            "<name>x</name>\n<id>abc</id>\n<meta>\n  <a>1</a>\n  <b>2</b>\n</meta>\n<port>80</port>\n",
        ),
    ]
}

pub(super) fn schema() -> Value {
    serde_json::from_str(SCHEMA).unwrap()
}

fn locks_of(
    language: Language,
    text: &str,
    paths: &[FieldPath],
    ranges: &[Range<usize>],
) -> Protected {
    let schema = schema();
    protect(
        language.behavior(),
        text,
        Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            paths,
            ranges,
            previous: None,
        },
    )
}

/// The locks of `text` after an edit from the text `previous` protected.
fn locks_after(language: Language, text: &str, previous: &Protected) -> Protected {
    let schema = schema();
    protect(
        language.behavior(),
        text,
        Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            previous: Some(previous),
            ..Sources::default()
        },
    )
}

fn covered(text: &str, range: &Range<usize>) -> String {
    String::from_utf16_lossy(&text.encode_utf16().collect::<Vec<_>>()[range.clone()])
}

fn allowed(language: Language, before: &str, after: &str, protected: &Protected) -> bool {
    let schema = schema();
    check(language.behavior(), Some(&schema), protected, before, after).is_ok()
}

#[test]
fn read_only_fields_are_located_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let protected = locks_of(language, text, &[], &[]);
        let pointers: Vec<_> = protected
            .locks
            .iter()
            .map(|lock| lock.pointer.clone().unwrap_or_default())
            .collect();
        if pointers != ["/id", "/meta"] {
            failures.push(format!("{language}: locks {pointers:?}"));
            continue;
        }
        let id = covered(text, &protected.locks[0].ranges[0]);
        if !id.contains("id") || !id.contains("abc") {
            failures.push(format!("{language}: /id covers {id:?}"));
        }
        if protected.locks[0].reason != super::texts::Reason::Given("Set by the server".into()) {
            failures.push(format!(
                "{language}: reason {:?}",
                protected.locks[0].reason
            ));
        }
        let meta: String = protected.locks[1]
            .ranges
            .iter()
            .map(|range| covered(text, range))
            .collect::<Vec<_>>()
            .join("|");
        let closes = match language.label() {
            "JSON" | "HCL" => meta.trim_end().ends_with('}'),
            "XML" => meta.trim_end().ends_with("</meta>"),
            _ => true,
        };
        if !meta.contains("meta")
            || !meta.contains('1')
            || !meta.contains('2')
            || !closes
            || meta.contains("port")
        {
            failures.push(format!("{language}: /meta covers {meta:?}"));
        }
        if protected.locks[1].ranges.len() != 1 {
            failures.push(format!("{language}: /meta in pieces {meta:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// The edits of the matrix, written for each language: what they do, the
/// edited text, and whether it must be refused.
pub(super) fn edits(language: Language, text: &str) -> Vec<(&'static str, Option<String>, bool)> {
    let replace = |from: &str, to: &str| text.contains(from).then(|| text.replacen(from, to, 1));
    let label = language.label();
    let (id_line, a_line, comment, new_in_meta, new_line_before_id, new_at_end) = match label {
        "YAML" => (
            "id: abc\n",
            "  a: 1\n",
            "# ",
            ("  b: 2\n", "  b: 2\n  c: 3\n"),
            "name: x\n",
            "z: 1\n",
        ),
        "JSON" => (
            "  \"id\": \"abc\",\n",
            "    \"a\": 1,\n",
            "",
            ("\"b\": 2\n", "\"b\": 2,\n    \"c\": 3\n"),
            "  \"name\": \"x\",\n",
            "",
        ),
        "TOML" => (
            "id = \"abc\"\n",
            "a = 1\n",
            "# ",
            ("b = 2\n", "b = 2\nc = 3\n"),
            "name = \"x\"\n",
            "",
        ),
        "HCL" => (
            "id = \"abc\"\n",
            "  a = 1\n",
            "# ",
            ("  b = 2\n", "  b = 2\n  c = 3\n"),
            "name = \"x\"\n",
            "z = 1\n",
        ),
        _ => (
            "<id>abc</id>\n",
            "  <a>1</a>\n",
            "",
            ("  <b>2</b>\n", "  <b>2</b>\n  <c>3</c>\n"),
            "<name>x</name>\n",
            "<z>1</z>\n",
        ),
    };
    let (key_from, key_to) = match label {
        "JSON" => ("\"id\"", "\"idx\""),
        "XML" => ("<id>abc</id>", "<idx>abc</idx>"),
        _ => ("id", "idx"),
    };
    let one = |text: &str| text.replacen('1', "9", 1);
    let mut list = vec![
        ("change the value", replace("abc", "abd"), true),
        ("append to the value", replace("abc", "abcZ"), true),
        ("prepend to the value", replace("abc", "Zabc"), true),
        ("delete the value", replace("abc", ""), true),
        ("delete the line", replace(id_line, ""), true),
        ("rename the key", replace(key_from, key_to), true),
        (
            "change a value inside the block",
            replace(a_line, &one(a_line)),
            true,
        ),
        ("delete a child of the block", replace(a_line, ""), true),
        (
            "add a child to the block",
            replace(new_in_meta.0, new_in_meta.1),
            true,
        ),
        (
            "delete a selection covering it",
            replace(&format!("{new_line_before_id}{id_line}"), ""),
            true,
        ),
        ("change another value", replace("80", "81"), false),
        (
            "add a line before it",
            replace(new_line_before_id, &format!("{new_line_before_id}\n")),
            false,
        ),
        ("change the name", replace("x", "y"), false),
        (
            "change values on both sides of it at once (Replace All)",
            replace("x", "y").map(|text| text.replacen("80", "81", 1)),
            false,
        ),
        (
            "change it and another value at once",
            replace("x", "y").map(|text| text.replacen("abc", "abd", 1)),
            true,
        ),
    ];
    if !comment.is_empty() {
        list.push((
            "comment it out",
            replace(id_line, &format!("{comment}{id_line}")),
            true,
        ));
    }
    if !new_at_end.is_empty() {
        list.push((
            "add a field at the end",
            Some(format!("{text}{new_at_end}")),
            false,
        ));
    }
    list
}

#[test]
fn edits_changing_a_read_only_field_are_refused_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let protected = locks_of(language, text, &[], &[]);
        for (what, edited, refused) in edits(language, text) {
            let Some(edited) = edited else {
                failures.push(format!("{language}: {what}: the edit does not apply"));
                continue;
            };
            if allowed(language, text, &edited, &protected) == refused {
                failures.push(format!(
                    "{language}: {what}: {} {edited:?}",
                    if refused { "allowed" } else { "refused" }
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn host_paths_and_ranges_lock_too() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let paths = [FieldPath::from("/port")];
        let protected = locks_of(language, text, &paths, &[]);
        if !protected
            .locks
            .iter()
            .any(|lock| lock.pointer.as_deref() == Some("/port"))
        {
            failures.push(format!("{language}: /port not locked"));
        }
        if allowed(language, text, &text.replacen("80", "81", 1), &protected) {
            failures.push(format!("{language}: /port changed"));
        }
        // A range: the name's line.
        let start = text.find('x').unwrap();
        let protected = protect(
            language.behavior(),
            text,
            Sources {
                ranges: &[start..start + 1],
                ..Sources::default()
            },
        );
        if allowed(language, text, &text.replacen('x', "y", 1), &protected) {
            failures.push(format!("{language}: the range changed"));
        }
        if !allowed(language, text, &text.replacen("80", "81", 1), &protected) {
            failures.push(format!("{language}: an edit outside the range was refused"));
        }
        // A whole first line, its newline included: the next line's start
        // is free.
        let line_end = text.find('\n').unwrap() + 1;
        let protected = protect(
            language.behavior(),
            text,
            Sources {
                ranges: &[0..line_end],
                ..Sources::default()
            },
        );
        let typed = format!("{}Q{}", &text[..line_end], &text[line_end..]);
        if !allowed(language, text, &typed, &protected) {
            failures.push(format!(
                "{language}: typing after a locked line was refused"
            ));
        }
        let word_first = text.starts_with(char::is_alphanumeric);
        if word_first && allowed(language, text, &format!("Q{text}"), &protected) {
            failures.push(format!(
                "{language}: typing at a locked line's start was allowed"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn whitespace_and_comments_around_a_locked_value_stay_free() {
    let cases = [
        (Language::YAML, "id: abc\n", "id: abc  \n"),
        (Language::YAML, "id: abc\n", "id: abc # note\n"),
        (Language::TOML, "id = \"abc\"\n", "id = \"abc\" # note\n"),
        (
            Language::JSON,
            "  \"id\": \"abc\",\n",
            "    \"id\": \"abc\",\n",
        ),
        (Language::HCL, "id = \"abc\"\n", "  id = \"abc\"\n"),
        (
            Language::XML,
            "<id>abc</id>\n",
            "<id>abc</id> <!-- note -->\n",
        ),
        // Whitespace inside a locked field or block.
        (Language::YAML, "id: abc", "id:  abc"),
        (Language::YAML, "  a: 1\n", "  a: 1 \n"),
        (Language::YAML, "  a: 1\n", "  a: 1\n\n"),
        (Language::YAML, "  a: 1\n", "  a: 1\n  \n"),
        (Language::JSON, "    \"a\": 1", "     \"a\": 1"),
        (Language::JSON, "    \"b\": 2", "   \"b\": 2"),
        (Language::JSON, "\"a\": 1", "\"a\":  1"),
        (Language::JSON, "\"a\": 1", "\"a\" : 1"),
        (Language::JSON, "\"id\": \"abc\"", "\"id\":  \"abc\""),
        (Language::JSON, "\"b\": 2\n", "\"b\": 2 \n"),
        (Language::TOML, "a = 1", " a = 1"),
        (Language::TOML, "a = 1", "a  = 1"),
        (Language::TOML, "id = \"abc\"", "id  = \"abc\""),
        (Language::TOML, "b = 2\n", "b = 2 \n"),
        (Language::TOML, "[meta]", "[ meta ]"),
        (Language::HCL, "  a = 1", "   a = 1"),
        (Language::HCL, "  a = 1", " a = 1"),
        (Language::HCL, "a = 1", "a  = 1"),
        (Language::HCL, "id = \"abc\"", "id  = \"abc\""),
        (Language::XML, "  <a>1</a>", "   <a>1</a>"),
        (Language::XML, "  <a>1</a>", "<a>1</a>"),
        (Language::XML, "<b>2</b>\n", "<b>2</b> \n"),
        (Language::XML, "</id>\n", "</id> \n"),
    ];
    let mut failures = Vec::new();
    for (language, from, to) in cases {
        let text = documents()
            .into_iter()
            .find(|(known, _)| *known == language)
            .unwrap()
            .1;
        let protected = locks_of(language, text, &[], &[]);
        let edited = text.replacen(from, to, 1);
        if edited == text || !allowed(language, text, &edited, &protected) {
            failures.push(format!("{language}: {from:?} → {to:?} refused"));
        }
    }
    // Whitespace that changes a value, a key or (in YAML, where the indent
    // is structure) a locked line's indent stays refused.
    let refused = [
        (Language::YAML, "id: abc", "  id: abc"),
        (Language::YAML, "  a: 1\n  b: 2", "    a: 1\n    b: 2"),
        (Language::YAML, "  b: 2", "   b: 2"),
        (Language::YAML, "  b: 2", " b: 2"),
        (Language::YAML, "id: abc", "id: a bc"),
        (Language::JSON, "\"abc\"", "\"a bc\""),
        (Language::JSON, "\"abc\"", "\"abc \""),
        (Language::JSON, "\"a\"", "\"a \""),
        (Language::TOML, "\"abc\"", "\" abc\""),
        (Language::TOML, "a = 1", "a b = 1"),
        (Language::HCL, "\"abc\"", "\"ab c\""),
        (Language::XML, "<id>abc</id>", "<id>a bc</id>"),
    ];
    for (language, from, to) in refused {
        let text = documents()
            .into_iter()
            .find(|(known, _)| *known == language)
            .unwrap()
            .1;
        let protected = locks_of(language, text, &[], &[]);
        let edited = text.replacen(from, to, 1);
        if edited == text || allowed(language, text, &edited, &protected) {
            failures.push(format!("{language}: {from:?} → {to:?} allowed"));
        }
    }
    // A host's range is text, not a field: whitespace in it stays refused.
    for (language, text) in documents() {
        let start = text.find("abc").unwrap();
        let protected = protect(
            language.behavior(),
            text,
            Sources {
                ranges: &[start..start + 3],
                ..Sources::default()
            },
        );
        if allowed(language, text, &text.replacen("abc", "a bc", 1), &protected) {
            failures.push(format!("{language}: whitespace in a host range allowed"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn locks_follow_the_field_in_every_edit_as_language() {
    let schema_bundle = std::sync::Arc::new(build_schema(SCHEMA).unwrap());
    let mut failures = Vec::new();
    for (host, host_text) in documents() {
        for (edit, _) in documents() {
            if edit == host {
                continue;
            }
            let bridge = Bridge::new(host, edit, Some(schema_bundle.clone())).unwrap();
            let shown = bridge.to_edit(host_text).unwrap().text;
            let protected = locks_of(edit, &shown, &[FieldPath::from("/port")], &[]);
            let pointers: Vec<_> = protected
                .locks
                .iter()
                .filter_map(|lock| lock.pointer.clone())
                .collect();
            if !["/id", "/meta", "/port"]
                .iter()
                .all(|pointer| pointers.iter().any(|found| found == pointer))
            {
                failures.push(format!("{host} as {edit}: locks {pointers:?} in {shown:?}"));
                continue;
            }
            for (what, edited, refused) in edits(edit, &shown) {
                let Some(edited) = edited else { continue };
                if refused && allowed(edit, &shown, &edited, &protected) {
                    failures.push(format!("{host} as {edit}: {what} allowed: {edited:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_save_changing_a_read_only_value_is_refused_in_every_language() {
    let schema = schema();
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let sources = Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            paths: &[],
            ranges: &[],
            previous: None,
        };
        let pointers = locked_pointers(language.behavior(), text, sources);
        let behavior = language.behavior();
        if check_save(
            behavior,
            Some(&schema),
            text,
            &text.replacen("abc", "abd", 1),
            &pointers,
        )
        .is_ok()
        {
            failures.push(format!("{language}: changed /id saved"));
        }
        if check_save(
            behavior,
            Some(&schema),
            text,
            &text.replacen("80", "81", 1),
            &pointers,
        )
        .is_err()
        {
            failures.push(format!("{language}: a free change was refused"));
        }
        // A host range locks the fields it covers at save time.
        let start = text
            .find("80")
            .map(|at| text[..at].encode_utf16().count())
            .unwrap();
        let ranged = locked_pointers(
            behavior,
            text,
            Sources {
                ranges: &[start..start + 2],
                ..Sources::default()
            },
        );
        if check_save(
            behavior,
            Some(&schema),
            text,
            &text.replacen("80", "81", 1),
            &ranged,
        )
        .is_ok()
        {
            failures.push(format!(
                "{language}: a change in a host range saved ({ranged:?})"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A small fixed-seed generator, so a failure always reproduces.
pub(super) struct Random(pub(super) u64);

impl Random {
    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(super) fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }
}

pub(super) const PIECES: &[&str] = &[
    "a",
    "1",
    " ",
    "\n",
    ":",
    "=",
    "\"",
    "{",
    "}",
    "<",
    ">",
    "/",
    "#",
    ",",
    "id",
    "abc",
    "x: 2\n",
    "  ",
    "meta",
    "[meta]\n",
    "</id>",
    "<id>",
    "c = 3\n",
    "  c: 3\n",
    "[meta.sub]\n",
    "meta.c = 3\n",
    "<id>x</id>",
    "\"id\": 1,",
    "id: x\n",
    "id = 1\n",
];

pub(super) const WORDS: &[&str] = &["abc", "abd", "x", "81", "id", "meta", "9", "a", "zz"];

/// UTF-16 ranges of the alphanumeric runs of `units`.
pub(super) fn words(units: &[u16]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut start = None;
    for (at, unit) in units.iter().chain(std::iter::once(&0)).enumerate() {
        let word = char::from_u32(u32::from(*unit)).is_some_and(char::is_alphanumeric);
        match (word, start) {
            (true, None) => start = Some(at),
            (false, Some(from)) => {
                found.push((from, at));
                start = None;
            }
            _ => {}
        }
    }
    found
}

/// Random edits, made in every edit language on every host language and
/// saved back: an edit the guard lets through never changes a read-only
/// value of the edited text (when it parses), and no save that goes
/// through changes one in the host.
#[test]
fn random_edits_never_change_a_read_only_value() {
    let schema_bundle = std::sync::Arc::new(build_schema(SCHEMA).unwrap());
    let schema = schema();
    let expected_id = json!("abc");
    let mut failures = Vec::new();
    let (mut applied, mut saved) = (0, 0);
    let mut random = Random(0x9e37_79b9_7f4a_7c15);
    for (host, host_text) in documents() {
        for (edit, _) in documents() {
            let bridge = Bridge::new(host, edit, Some(schema_bundle.clone()));
            let mut shown = match &bridge {
                Some(bridge) => bridge.to_edit(host_text).unwrap().text,
                None => host_text.to_string(),
            };
            let mut saved_host = host_text.to_string();
            let original = codec::read(edit.behavior(), &shown, Some(&schema)).unwrap();
            // The last text that parsed, and how many edits ago.
            let mut readable = shown.clone();
            let mut unreadable_for = 0;
            let mut protected = locks_of(edit, &shown, &[], &[]);
            for step in 0..500 {
                let units: Vec<u16> = shown.encode_utf16().collect();
                let (start, end, piece) = if random.below(2) == 0 {
                    // A word replaced by another, which mostly keeps the
                    // document readable, so saves happen.
                    let words: Vec<(usize, usize)> = words(&units);
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
                if !allowed(edit, &shown, &edited, &protected) {
                    continue;
                }
                if let Ok(after) = codec::read(edit.behavior(), &edited, Some(&schema)) {
                    for pointer in ["/id", "/meta"] {
                        if original.pointer(pointer) != after.pointer(pointer) {
                            failures.push(format!(
                                "{host} as {edit}, step {step}: {pointer} changed by {piece:?} at {start}..{end}:\n{shown}\n→\n{edited}"
                            ));
                        }
                    }
                }
                applied += 1;
                // Edits go on through texts that do not parse (a key half
                // typed), the locks carried as the editor carries them;
                // after a few the user is taken to fix the text.
                let previous = std::mem::take(&mut protected);
                protected = locks_after(edit, &edited, &previous);
                shown = edited;
                if codec::read(edit.behavior(), &shown, Some(&schema)).is_err() {
                    unreadable_for += 1;
                    if unreadable_for > 4 {
                        shown = readable.clone();
                        protected = locks_of(edit, &shown, &[], &[]);
                        unreadable_for = 0;
                    }
                    continue;
                }
                unreadable_for = 0;
                readable = shown.clone();
                // Saved as the editor saves: converted, then checked.
                let converted = match &bridge {
                    Some(bridge) => bridge
                        .to_host(&shown, &saved_host)
                        .map(|conversion| conversion.text),
                    None => Ok(shown.clone()),
                };
                let Ok(converted) = converted else { continue };
                let pointers = locked_pointers(
                    host.behavior(),
                    &saved_host,
                    Sources {
                        marking_schema: Some(&schema),
                        reading_schema: Some(&schema),
                        paths: &[],
                        ranges: &[],
                        previous: None,
                    },
                );
                if check_save(
                    host.behavior(),
                    Some(&schema),
                    &saved_host,
                    &converted,
                    &pointers,
                )
                .is_ok()
                {
                    saved_host = converted;
                    saved += 1;
                }
            }
            if let Ok(saved) = codec::read(host.behavior(), &saved_host, Some(&schema))
                && saved.pointer("/id") != Some(&expected_id)
            {
                failures.push(format!(
                    "{host} as {edit}: saved /id is {:?}",
                    saved.pointer("/id")
                ));
            }
            if failures.len() > 20 {
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures:\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    // The edits did reach the documents and the hosts.
    assert!(
        applied > 2000 && saved > 700,
        "applied {applied}, saved {saved}"
    );
}

/// Editing as another language, the host's read-only ranges lock the
/// fields they cover in the host's text: a new host text puts them on what
/// it now has there, not on what the text held at mount.
#[test]
fn host_ranges_lock_the_fields_of_the_current_host_text_while_editing_as() {
    use super::super::paths::{TextPosition, TextRange};
    use super::super::sync::take_host_value;
    use super::super::test_state::TestEditor;
    use leptos::prelude::*;
    let (_owner, state) = TestEditor {
        edit_as: Some(Language::JSON),
        read_only_ranges: vec![TextRange::new(
            TextPosition::new(2, 1),
            TextPosition::new(2, 5),
        )],
        ..TestEditor::new(Language::YAML, "a: 1\nb: 2\n")
    }
    .mount();
    let locked = || {
        state.protect().protected.with_untracked(|protected| {
            protected
                .locks
                .iter()
                .filter_map(|lock| lock.pointer.clone())
                .collect::<Vec<_>>()
        })
    };
    assert_eq!(locked(), ["/b"]);
    take_host_value(state, "b: 2\na: 1\n".to_string());
    assert_eq!(locked(), ["/a"]);
}
