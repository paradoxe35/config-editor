//! Paths and positions, both ways, in each language's version of one
//! document: `range_of` finds what a path names, and `path_at` anywhere in
//! that range (and in the value) gives the path back. Each test lists every
//! failing case at once.

use super::super::Language;
use super::{FieldPath, TextPosition};

const YAML: &str = "\
listen: 7835
web:
  http: 80
  hosts:
    - a.example
    - b.example
tags: [x, y]
";

const JSON: &str = r#"{
  "listen": 7835,
  "web": {
    "http": 80,
    "hosts": [
      "a.example",
      "b.example"
    ]
  },
  "tags": ["x", "y"]
}
"#;

const TOML: &str = r#"listen = 7835
tags = ["x", "y"]

[web]
http = 80
hosts = [
  "a.example",
  "b.example",
]
"#;

const HCL: &str = r#"listen = 7835
tags   = ["x", "y"]
web {
  http = 80
  hosts = [
    "a.example",
    "b.example",
  ]
}
"#;

const XML: &str = "\
<listen>7835</listen>
<web>
  <http>80</http>
  <hosts>a.example</hosts>
  <hosts>b.example</hosts>
</web>
<tags>x</tags>
<tags>y</tags>
";

pub(super) fn documents() -> [(Language, &'static str); 5] {
    [
        (Language::YAML, YAML),
        (Language::JSON, JSON),
        (Language::TOML, TOML),
        (Language::HCL, HCL),
        (Language::XML, XML),
    ]
}

/// What `range_of` selects for each path, per language (YAML, JSON, TOML,
/// HCL, XML).
const RANGES: &[(&str, [&str; 5])] = &[
    (
        "/listen",
        [
            "listen: 7835",
            r#""listen": 7835"#,
            "listen = 7835",
            "listen = 7835",
            "<listen>7835</listen>",
        ],
    ),
    ("/web", ["web", r#""web""#, "[web]", "web", "web"]),
    (
        "/web/http",
        [
            "http: 80",
            r#""http": 80"#,
            "http = 80",
            "http = 80",
            "<http>80</http>",
        ],
    ),
    (
        "/web/hosts",
        ["hosts", r#""hosts""#, "hosts", "hosts", "hosts"],
    ),
    (
        "/web/hosts/1",
        [
            "b.example",
            r#""b.example""#,
            r#""b.example""#,
            r#""b.example""#,
            "<hosts>b.example</hosts>",
        ],
    ),
    (
        "/tags/1",
        ["y", r#""y""#, r#""y""#, r#""y""#, "<tags>y</tags>"],
    ),
];

#[track_caller]
fn report(failures: Vec<String>) {
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn range_of_selects_the_field_in_every_language() {
    let mut failures = Vec::new();
    for (pointer, expected) in RANGES {
        for ((language, text), expected) in documents().into_iter().zip(expected) {
            let found = language
                .range_of(text, *pointer)
                .map(|range| range.slice(text));
            if found.as_deref() != Some(*expected) {
                failures.push(format!(
                    "{language} {pointer}: expected {expected:?}, found {found:?}"
                ));
            }
        }
    }
    report(failures);
}

/// QA 21: on a quoted key's closing quote (TOML), or just after a `}` in a
/// string (HCL `"${x}"`), `path_at` gave the table or block around the
/// field (so did a host range starting there).
#[test]
fn path_at_on_quoted_keys_and_interpolated_strings_gives_the_field() {
    // (language, text, the field's path, its key and value as written)
    let cases = [
        (
            Language::YAML,
            "\"quoted key\": \"${x}\"\n",
            "/quoted key",
            ["\"quoted key\"", "\"${x}\""],
        ),
        (
            Language::JSON,
            "{\"quoted key\": \"${x}\"}\n",
            "/quoted key",
            ["\"quoted key\"", "\"${x}\""],
        ),
        (
            Language::TOML,
            "[t]\n\"quoted key\" = \"${x}\"\n",
            "/t/quoted key",
            ["\"quoted key\"", "\"${x}\""],
        ),
        (
            Language::TOML,
            "[t]\n'lit key' = 1\n",
            "/t/lit key",
            ["'lit key'", "1"],
        ),
        (
            Language::HCL,
            "b {\n  k = \"${x}\"\n}\n",
            "/b/k",
            ["k", "\"${x}\""],
        ),
        (
            Language::HCL,
            "b {\n  k = \"$${x}\"\n}\n",
            "/b/k",
            ["k", "\"$${x}\""],
        ),
        (
            Language::XML,
            "<b>\n  <k>${x}</k>\n</b>\n",
            "/b/k",
            ["k", "${x}"],
        ),
    ];
    let mut failures = Vec::new();
    for (language, text, pointer, written) in cases {
        for part in written {
            // On each of its characters.
            let at = text.find(part).expect("written in the text");
            let start = TextPosition::from_offset(text, at);
            for column in start.column..start.column + part.chars().count() {
                let found = language.path_at(text, TextPosition::new(start.line, column));
                if found.as_ref().map(FieldPath::to_pointer).as_deref() != Some(pointer) {
                    failures.push(format!(
                        "{language} {part:?} at {}:{column}: {:?}",
                        start.line,
                        found.map(|path| path.to_pointer())
                    ));
                }
            }
        }
    }
    report(failures);
}

#[test]
fn path_at_anywhere_in_a_field_gives_its_path_back() {
    let mut failures = Vec::new();
    for (pointer, _) in RANGES {
        for (language, text) in documents() {
            let Some(range) = language.range_of(text, *pointer) else {
                failures.push(format!("{language} {pointer}: not found"));
                continue;
            };
            // Repeated elements have no key of their own: the range is the
            // first one's, which is on that item.
            let wanted = match (language.label(), *pointer) {
                ("XML", "/web/hosts") => FieldPath::from("/web/hosts/0"),
                _ => FieldPath::from(*pointer),
            };
            let start = range.start.to_offset(text);
            let end = range.end.to_offset(text);
            // Every caret position from the start of the range to its end.
            for offset in start..=end {
                let at = TextPosition::from_offset(text, offset);
                let found = language.path_at(text, at);
                if found.as_ref() != Some(&wanted) {
                    failures.push(format!(
                        "{language} {pointer} at {}:{}: found {}",
                        at.line,
                        at.column,
                        found.map_or("None".to_string(), |path| path.to_pointer())
                    ));
                }
            }
        }
    }
    report(failures);
}

#[test]
fn the_root_is_the_whole_document_and_unknown_paths_are_none() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        let whole = language.range_of(text, "").map(|range| range.slice(text));
        if whole.as_deref() != Some(text) {
            failures.push(format!("{language}: root selects {whole:?}"));
        }
        for missing in ["/nope", "/web/nope", "/web/hosts/7"] {
            if let Some(range) = language.range_of(text, missing) {
                failures.push(format!(
                    "{language} {missing}: found {:?}",
                    range.slice(text)
                ));
            }
        }
        if let Some(path) = language.path_at(text, TextPosition::new(99, 1)) {
            failures.push(format!("{language}: {path} past the last line"));
        }
        let end = TextPosition::from_offset(text, text.encode_utf16().count() + 5);
        if language.path_at(text, end).is_none() {
            failures.push(format!("{language}: nothing at the end ({end:?})"));
        }
    }
    report(failures);
}

#[test]
fn pointers_and_segments_name_the_same_path() {
    let built = FieldPath::root().key("a/b").key("c~d").index(3);
    assert_eq!(built.to_pointer(), "/a~1b/c~0d/3");
    assert_eq!(FieldPath::from(built.to_pointer()), built);
    assert_eq!(FieldPath::from("a~1b/c~0d/3"), built);
    assert!(FieldPath::from("").is_root());
    // RFC 6901: "/" is the key "" under the root, not the root.
    assert_eq!(FieldPath::from("/"), FieldPath::root().key(""));
    assert_eq!(FieldPath::root().key("").to_pointer(), "/");
    assert_eq!(FieldPath::root().to_pointer(), "");
}

#[test]
fn positions_and_offsets_round_trip_over_wide_characters() {
    let text = "a: \"é😀\"\nb: 1\n";
    for offset in 0..=text.encode_utf16().count() {
        let position = TextPosition::from_offset(text, offset);
        // An offset inside a surrogate pair rounds up to the character's end.
        let back = position.to_offset(text);
        assert!(
            back == offset || back == offset + 1,
            "{offset} -> {position:?} -> {back}"
        );
    }
    assert_eq!(TextPosition::from_offset(text, 8), TextPosition::new(1, 8));
    assert_eq!(TextPosition::from_offset(text, 9), TextPosition::new(2, 1));
}

#[test]
fn the_caret_path_stays_cheap_in_large_documents() {
    // Computed on every caret move while a host follows it.
    let samples = [
        (
            Language::YAML,
            include_str!("../../../tests/fixtures/samples/burrow.large.yaml"),
        ),
        (
            Language::JSON,
            include_str!("../../../tests/fixtures/samples/burrow.large.json"),
        ),
        (
            Language::TOML,
            include_str!("../../../tests/fixtures/samples/burrow.large.toml"),
        ),
        (
            Language::HCL,
            include_str!("../../../tests/fixtures/samples/burrow.large.hcl"),
        ),
        (
            Language::XML,
            include_str!("../../../tests/fixtures/samples/burrow.large.xml"),
        ),
    ];
    let mut failures = Vec::new();
    for (language, text) in samples {
        let lines = text.split('\n').count();
        // No read of the whole document for spans at a caret move.
        let ((), reads) = crate::editor::work_count::counted(
            crate::editor::work_count::Work::DocumentRead,
            || {
                let _ = language.path_at(text, TextPosition::new(lines / 2, 5));
            },
        );
        if reads > 0 {
            failures.push(format!("{language}: {reads} document reads per caret move"));
        }
        // The best of many runs: a few milliseconds each, and a run on a
        // loaded machine (parallel test lanes) takes several times as long.
        let (elapsed, ()) = crate::editor::test_timing::best_of(20, || {
            for line in [lines / 2, lines - 2] {
                let _ = language.path_at(text, TextPosition::new(line, 5));
            }
        });
        let elapsed = elapsed / 2;
        // About 1-2 ms here, XML (a markup walk) about 5.
        let budget = if language == Language::XML { 25 } else { 15 };
        if elapsed > std::time::Duration::from_millis(budget) {
            failures.push(format!("{language}: {elapsed:?} per caret move"));
        }
    }
    report(failures);
}

#[test]
fn the_pointer_slash_names_the_empty_key_in_every_language_that_has_one() {
    let cases = [
        (Language::YAML, "\"\": empty\na: 1\n", "\"\": empty"),
        (Language::JSON, "{\n  \"\": 1,\n  \"a\": 2\n}\n", "\"\": 1"),
        (Language::TOML, "\"\" = 1\na = 2\n", "\"\" = 1"),
    ];
    let mut failures = Vec::new();
    for (language, text, expected) in cases {
        let found = language
            .range_of(text, "/")
            .map(|range| range.slice(text).to_string());
        if found.as_deref() != Some(expected) {
            failures.push(format!("{language}: {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
