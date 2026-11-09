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
            include_str!("../../../tests/fixtures/samples/tunnel.large.yaml"),
        ),
        (
            Language::JSON,
            include_str!("../../../tests/fixtures/samples/tunnel.large.json"),
        ),
        (
            Language::TOML,
            include_str!("../../../tests/fixtures/samples/tunnel.large.toml"),
        ),
        (
            Language::HCL,
            include_str!("../../../tests/fixtures/samples/tunnel.large.hcl"),
        ),
        (
            Language::XML,
            include_str!("../../../tests/fixtures/samples/tunnel.large.xml"),
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

/// Without an empty key, `/` names nothing, in every language.
#[test]
fn the_pointer_slash_is_none_without_an_empty_key() {
    let cases = [
        (Language::YAML, "a: 1\n"),
        (Language::YAML, "l:\n  - ? k\n    : v\n"),
        (Language::YAML, "? k\n: v\n"),
        (Language::JSON, "{\"a\": 1}\n"),
        (Language::TOML, "a = 1\n"),
        (Language::HCL, "a = 1\n"),
        (Language::XML, "<r><a>1</a></r>\n"),
    ];
    let mut failures = Vec::new();
    for (language, text) in cases {
        if let Some(range) = language.range_of(text, "/") {
            failures.push(format!("{language} {text:?}: {range:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// YAML forms whose key or item marker is written apart: an item whose
/// dash is alone on its line is marked at the dash, and an explicit key
/// (`? k`) is addressable, its value after `:` on the next line.
#[test]
fn yaml_items_and_keys_written_apart_have_their_range() {
    let cases = [
        ("l:\n  -\n    x: 1\n  - a\n", "/l/0", "-"),
        ("l:\n  -\n    x: 1\n", "/l/0/x", "x: 1"),
        ("-\n  x: 1\n", "/0", "-"),
        ("l:\n  - ? k\n    : v\n", "/l/0/k", "k"),
        ("? k\n: v\n", "/k", "k"),
        ("? k\n: v\nz: 1\n", "/z", "z: 1"),
    ];
    let mut failures = Vec::new();
    for (text, pointer, expected) in cases {
        let found = Language::YAML.range_of(text, pointer);
        if found.map(|range| range.slice(text).to_string()).as_deref() != Some(expected) {
            failures.push(format!("{pointer} in {text:?}: {found:?}"));
            continue;
        }
        let back = Language::YAML
            .path_at(text, found.unwrap().start)
            .map(|path| path.to_pointer());
        if back.as_deref() != Some(pointer) {
            failures.push(format!("{pointer} in {text:?}: path_at gives {back:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// YAML anchors and tags, on a block's key line (`base: &b`) or on a key
/// itself (`&k b: 1`), and comments after a block's key: the block's lines
/// stay under its key wherever the caret is (on a key, in a value, on a
/// blank line where a key is typed), and a key is named without them.
#[test]
fn yaml_node_properties_keep_the_paths_below_them() {
    // `|` marks the caret.
    let cases = [
        ("base: &b\n  x: 1\n  |\n", "/base"),
        ("base: !t\n  x: 1\n  |\n", "/base"),
        ("base: &b !t # shared\n  x: 1\n  |\n", "/base"),
        ("base: # note\n  x: 1\n  |\n", "/base"),
        ("base: &b\n  x: o|ne\n", "/base/x"),
        ("base: &b\n  |x: 1\n", "/base/x"),
        ("l: !list\n  - d: |1\n", "/l/0/d"),
        ("a:\n  &k b: 1|\n", "/a/b"),
        ("a:\n  &k |b: 1\n", "/a/b"),
        ("a:\n  !t c|: 2\n", "/a/c"),
        ("a:\n  &k b:\n    c: |1\n", "/a/b/c"),
        ("? &k b\n: |1\n", "/b"),
        ("- - 1\n|- - 3\n", "/1"),
        ("- - 1\n- |- 3\n", "/1/0"),
    ];
    let mut failures = Vec::new();
    for (marked, wanted) in cases {
        let caret = marked.find('|').unwrap();
        let text = marked.replacen('|', "", 1);
        let at = TextPosition::from_offset(&text, text[..caret].encode_utf16().count());
        let found = Language::YAML
            .path_at(&text, at)
            .map(|path| path.to_pointer());
        if found.as_deref() != Some(wanted) {
            failures.push(format!(
                "{marked:?}: path_at gives {found:?}, wanted {wanted}"
            ));
        }
    }
    // What a key with its own anchor or tag covers.
    for (text, pointer, expected) in [
        ("a:\n  &k b: 1\n  c: 2\n", "/a/b", "b: 1"),
        ("a:\n  &k b: 1\n  c: 2\n", "/a/c", "c: 2"),
        ("a:\n  !t c: 2\n", "/a/c", "c: 2"),
        ("- - 1\n- - - 3\n", "/1", "-"),
    ] {
        let found = Language::YAML.range_of(text, pointer);
        if found.map(|range| range.slice(text).to_string()).as_deref() != Some(expected) {
            failures.push(format!("{pointer} in {text:?}: range {found:?}"));
        }
    }
    report(failures);
}
