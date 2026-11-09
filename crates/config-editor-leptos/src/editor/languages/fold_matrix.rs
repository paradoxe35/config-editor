//! Fold ranges across the five languages: nested containers, empty and
//! one-line ones (never foldable), comments, multi-line strings and
//! unterminated structures. Every failing case is listed at once.

use super::super::language::FoldRange;
use super::Language;

/// A case: language, name, document, expected `(start, end)` pairs.
type Case = (
    Language,
    &'static str,
    &'static str,
    &'static [(usize, usize)],
);

const CASES: &[Case] = &[
    // YAML: mappings, sequences, block scalars by indentation.
    (
        Language::YAML,
        "nested mapping and sequence",
        "a:\n  b: 1\n  c:\n    - x\n    - y\nd: 2",
        &[(0, 4), (2, 4)],
    ),
    (
        Language::YAML,
        "sequence at its key's indent",
        "list:\n- a\n- b\ne: 1",
        &[(0, 2)],
    ),
    (
        Language::YAML,
        "items with mappings",
        "- name: a\n  port: 1\n- name: b",
        &[(0, 1)],
    ),
    (
        Language::YAML,
        "block scalar",
        "s: |\n  line 1\n  # not a comment\n  line 3\nt: 1",
        &[(0, 3)],
    ),
    (
        Language::YAML,
        "comment run, comment inside a mapping",
        "# a\n# b\nk:\n  # c\n  v: 1\nz: 2",
        &[(0, 1), (2, 4)],
    ),
    (
        Language::YAML,
        "empty and one-line containers",
        "a: {}\nb: [1, 2]\nc:\nd: 1",
        &[],
    ),
    (
        Language::YAML,
        "unterminated flow mapping",
        "a: {\n  b: 1\n",
        &[(0, 1)],
    ),
    // JSON: objects and arrays.
    (
        Language::JSON,
        "nested containers, closers kept visible",
        "{\n  \"a\": {\n    \"b\": 1\n  },\n  \"c\": [1, 2],\n  \"d\": []\n}",
        &[(0, 5), (1, 2)],
    ),
    (
        Language::JSON,
        "empty container over two lines",
        "{\n  \"e\": [\n  ]\n}",
        &[(0, 2)],
    ),
    (
        Language::JSON,
        "brackets in strings",
        "{\n  \"x\": \"{[\",\n  \"y\": \"}\"\n}",
        &[(0, 2)],
    ),
    (
        Language::JSON,
        "unterminated",
        "{\n  \"a\": {\n    \"b\": 1\n",
        &[],
    ),
    (Language::JSON, "one line", "{\"a\": {\"b\": 1}}", &[]),
    // TOML: tables to the next header, arrays, strings, comments.
    (
        Language::TOML,
        "tables, array of tables, multi-line array",
        "title = \"x\"\n\n[server]\nhost = \"a\"\nports = [\n  1,\n  2,\n]\n\n# next\n[[items]]\nname = \"a\"",
        &[(2, 7), (4, 6), (10, 11)],
    ),
    (
        Language::TOML,
        "multi-line string with brackets and a header-like line",
        "s = \"\"\"\n[not.a.table]\n{\n\"\"\"\nk = 1",
        &[(0, 2)],
    ),
    (
        Language::TOML,
        "comment run and empty table",
        "# a\n# b\n[empty]\n[full]\nk = 1",
        &[(0, 1), (3, 4)],
    ),
    (
        Language::TOML,
        "inline table over lines",
        "t = {\n  a = 1,\n}",
        &[(0, 1)],
    ),
    (Language::TOML, "unterminated array", "a = [\n  1,\n", &[]),
    // HCL: blocks, objects, lists, heredocs, comments.
    (
        Language::HCL,
        "block, object, heredoc",
        "resource \"x\" \"y\" {\n  tags = {\n    a = 1\n  }\n  list = [1, 2]\n  script = <<EOF\necho {\nEOF\n}",
        &[(0, 7), (1, 2), (5, 6)],
    ),
    (
        Language::HCL,
        "block comment and comment run",
        "/* multi\nline */\n# a\n// b\nx = 1",
        &[(0, 1), (2, 3)],
    ),
    (
        Language::HCL,
        "empty block and one-line block",
        "a {\n}\nb { c = 1 }",
        &[],
    ),
    (
        Language::HCL,
        "call over lines, brackets in strings",
        "x = merge(\n  \"{\",\n  y,\n)",
        &[(0, 2)],
    ),
    (
        Language::HCL,
        "unterminated block",
        "a {\n  b {\n    c = 1\n  }\n",
        &[(1, 2)],
    ),
    // XML: elements, comments, CDATA, multi-line tags.
    (
        Language::XML,
        "elements, comment, CDATA",
        "<root>\n  <a>\n    <b>1</b>\n  </a>\n  <c/>\n  <!-- one\n  two -->\n  <d><![CDATA[x\ny]]></d>\n</root>",
        &[(0, 8), (1, 2), (5, 6), (7, 8)],
    ),
    (
        Language::XML,
        "one-line and empty elements",
        "<r>\n  <a></a>\n  <b>\n  </b>\n</r>",
        &[(0, 3)],
    ),
    (
        Language::XML,
        "multi-line start tag, self-closing",
        "<r>\n  <a\n    x=\"1\"\n  />\n</r>",
        &[(0, 3), (1, 2)],
    ),
    (
        Language::XML,
        "unterminated element, quoted >",
        "<r>\n  <a x=\">\">\n    <b>\n      1\n    </b>\n",
        &[(2, 3)],
    ),
];

#[test]
fn fold_ranges_match_across_languages() {
    let mut failures = Vec::new();
    for (language, case, source, expected) in CASES {
        let expected: Vec<FoldRange> = expected
            .iter()
            .map(|(start, end)| FoldRange::new(*start, *end))
            .collect();
        let found = language.fold_ranges(source);
        if found != expected {
            failures.push(format!(
                "{language} {case}: {:?}, expected {:?}",
                found
                    .iter()
                    .map(|range| (range.start, range.end))
                    .collect::<Vec<_>>(),
                expected
                    .iter()
                    .map(|range| (range.start, range.end))
                    .collect::<Vec<_>>(),
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Fold ranges are well formed on every prefix of the samples (text being
/// typed is often unterminated): sorted, one per start, inside the text.
#[test]
fn fold_ranges_stay_well_formed_while_typing() {
    let mut failures = Vec::new();
    for (language, case, source, _) in CASES {
        for cut in (0..=source.len()).filter(|cut| source.is_char_boundary(*cut)) {
            let text = &source[..cut];
            let lines = text.split('\n').count();
            let ranges = language.fold_ranges(text);
            let sorted = ranges.windows(2).all(|pair| pair[0].start < pair[1].start);
            let inside = ranges
                .iter()
                .all(|range| range.start < range.end && range.end < lines);
            if !sorted || !inside {
                failures.push(format!("{language} {case} cut at {cut}: {ranges:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The 2000-line samples fold quickly: a 10k-line document's ranges in
/// well under a frame.
#[test]
fn fold_ranges_of_large_documents_are_fast() {
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
    for (language, sample) in samples {
        let large = sample.repeat(5);
        let (elapsed, ranges) =
            crate::editor::test_timing::best_of(3, || language.fold_ranges(&large));
        assert!(!ranges.is_empty(), "{language}");
        assert!(
            elapsed.as_millis() < 500,
            "{language}: {} ranges in {elapsed:?}",
            ranges.len()
        );
    }
}
