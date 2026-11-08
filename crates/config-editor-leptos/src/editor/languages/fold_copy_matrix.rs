//! Copy and cut with nothing selected on a folded header take the whole
//! block it folds, in every language, as Ctrl+/ does: the hidden lines and
//! the closing bracket or end tag the fold leaves shown (round 21: only
//! the header line went, the children were left behind and XML kept an
//! orphan end tag).

use super::super::commands::copied_lines;
use super::super::projection::Projection;
use super::Language;

/// Language, document, the folded header line (0-based), what is copied.
const CASES: &[(Language, &str, usize, &str)] = &[
    (
        Language::YAML,
        "a:\n  b:\n    c: 1\n  e: 3\nf: 4\n",
        0,
        "a:\n  b:\n    c: 1\n  e: 3\n",
    ),
    (
        Language::YAML,
        "a:\n  b:\n    c: 1\n  e: 3\nf: 4\n",
        1,
        "  b:\n    c: 1\n",
    ),
    (
        Language::YAML,
        "s: |\n  a\n\n  b\nn: 1\n",
        0,
        "s: |\n  a\n\n  b\n",
    ),
    (
        Language::YAML,
        "l: [\n  1,\n  2\n]\nn: 1\n",
        0,
        "l: [\n  1,\n  2\n]\n",
    ),
    (
        Language::JSON,
        "{\n  \"a\": {\n    \"c\": 1\n  },\n  \"f\": 4\n}\n",
        1,
        "  \"a\": {\n    \"c\": 1\n  },\n",
    ),
    (
        Language::JSON,
        "{\n  \"a\": [\n    1\n  ]\n}",
        0,
        "{\n  \"a\": [\n    1\n  ]\n}",
    ),
    (
        Language::TOML,
        "l = [\n  1,\n  2,\n]\nn = 1\n",
        0,
        "l = [\n  1,\n  2,\n]\n",
    ),
    (
        Language::TOML,
        "s = \"\"\"\na\n\nb\n\"\"\"\nn = 1\n",
        0,
        "s = \"\"\"\na\n\nb\n\"\"\"\n",
    ),
    (
        Language::HCL,
        "a {\n  c = 1\n}\nf = 4\n",
        0,
        "a {\n  c = 1\n}\n",
    ),
    (
        Language::HCL,
        "f = 4\na {\n  c = 1\n}",
        1,
        "a {\n  c = 1\n}",
    ),
    (
        Language::HCL,
        "s = <<EOF\na\n\nb\nEOF\nn = 1\n",
        0,
        "s = <<EOF\na\n\nb\nEOF\n",
    ),
    (
        Language::XML,
        "<r>\n  <a>\n    <c>1</c>\n  </a>\n  <f>4</f>\n</r>\n",
        1,
        "  <a>\n    <c>1</c>\n  </a>\n",
    ),
    (
        Language::XML,
        "<r>\n<!-- a\nb\n-->\n<f>4</f>\n</r>\n",
        1,
        "<!-- a\nb\n-->\n",
    ),
];

#[test]
fn copy_on_a_folded_header_takes_the_whole_block() {
    let mut failures = Vec::new();
    for (language, text, header, expected) in CASES {
        let Some(fold) = language
            .fold_ranges(text)
            .into_iter()
            .find(|range| range.start == *header)
        else {
            failures.push(format!("{language} {text:?}: no fold on line {header}"));
            continue;
        };
        let projection = Projection::new(text, &[fold]);
        let line_start: usize = text
            .split('\n')
            .take(*header)
            .map(|line| line.encode_utf16().count() + 1)
            .sum();
        let line_len = text
            .split('\n')
            .nth(*header)
            .unwrap()
            .encode_utf16()
            .count();
        // Anywhere on the header line.
        for cursor in [line_start, line_start + line_len / 2, line_start + line_len] {
            let (start, end) = copied_lines(language.behavior(), text, cursor, &projection);
            let units: Vec<u16> = text.encode_utf16().collect();
            let copied = String::from_utf16_lossy(&units[start..end]);
            if copied != *expected {
                failures.push(format!(
                    "{language} {text:?} folded on line {header}, caret {cursor}: copied {copied:?}, expected {expected:?}"
                ));
            }
        }
        // Unfolded, the line alone.
        let unfolded = Projection::new(text, &[]);
        let (start, end) = copied_lines(language.behavior(), text, line_start, &unfolded);
        let line_end = line_start + line_len + usize::from(line_start + line_len < text.len());
        if (start, end) != (line_start, line_end) {
            failures.push(format!(
                "{language} {text:?} unfolded line {header}: {start}..{end}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
