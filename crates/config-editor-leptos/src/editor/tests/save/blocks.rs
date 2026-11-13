//! YAML block scalars edited from every other view: the host's header
//! (literal or folded, its indentation indicator and chomping) stays, the
//! text is spliced in place, and nothing else in the document changes.

use crate::editor::Language;
use crate::editor::save_matrix::{from_every_view, report, slot};
use serde_json::Value;

/// Every block form: literal and folded, each chomping, indentation
/// indicators, a more-indented folded line, text that looks like a comment,
/// trailing spaces and a tab.
const BLOCKS: &str = "a: | # lit comment\n  alpha\n  beta\nb: |-\n  bravo\n  two\nc: |+\n  charlie\n\nd: >\n  delta one\n    more indented\n  delta two\ne: >+\n  echo\n\nf: >2-\n   lead\n  g\nh: |\n  trail  \n  x\ni: |4\n      four\n    base\nj: |\n  # not a comment\n  line\nk: |\n  tab\tin\n  z\nn: 1\n";

/// Nested block scalars: under a key, in a list, in a list item's map, with
/// an indicator, and a folded line long enough to have been wrapped.
const NESTED: &str = "# c\nfolded: >\n  folded text that is long enough\n  second\nind: |2\n    two extra\n  base\nlist:\n  - |\n    in list\n    two\n  - |2-\n     spaced item\n  - k: |\n      nested map\n      lines\n    # kc\n    j: 2 # jc\n  - - |\n      compact block\n    - z\nmap:\n  inner: |1\n    indented\n   less\nn: 1\n";

/// `word` in the string at `key` (a `/`-separated path) made `word` + `tail`.
fn append(key: &'static str, word: &'static str, tail: &'static str) -> impl Fn(&mut Value) {
    move |value| {
        let pointer = format!("/{key}");
        let slot = slot(value, &pointer);
        let text = slot.as_str().expect("a string").to_string();
        assert!(text.contains(word), "{key} holds {word:?}: {text:?}");
        *slot = text.replacen(word, &format!("{word}{tail}"), 1).into();
    }
}

#[test]
fn block_scalars_keep_their_indicators_and_folding_through_any_view() {
    let mut failures = Vec::new();
    for (key, word) in [
        ("a", "alpha"),
        ("b", "bravo"),
        ("c", "charlie"),
        ("d", "delta two"),
        ("d", "more"),
        ("e", "echo"),
        ("f", "lead"),
        ("f", "g"),
        ("h", "trail"),
        ("i", "four"),
        ("i", "base"),
        ("j", "line"),
        ("k", "tab"),
    ] {
        from_every_view(
            &mut failures,
            &format!("{key}: {word}"),
            Language::YAML,
            BLOCKS,
            &append(key, word, "X"),
            &BLOCKS.replacen(&format!("  {word}"), &format!("  {word}X"), 1),
        );
    }
    for (key, word) in [
        ("folded", "second"),
        ("folded", "long"),
        ("ind", "two extra"),
        ("ind", "base"),
        ("list/0", "in list"),
        ("list/1", "spaced"),
        ("list/2/k", "nested map"),
        ("list/2/k", "lines"),
        ("list/3/0", "compact block"),
        ("map/inner", "indented"),
        ("map/inner", "less"),
    ] {
        from_every_view(
            &mut failures,
            &format!("{key}: {word}"),
            Language::YAML,
            NESTED,
            &append(key, word, "X"),
            &NESTED.replacen(&format!(" {word}"), &format!(" {word}X"), 1),
        );
    }
    report(failures);
}

/// A redundant keep indicator (`+` on a block ending in one line break,
/// which reads as clipping) stays as the host wrote it (hard round 28's
/// `D28.editing-1`: `>2+` became `>2`).
#[test]
fn a_redundant_keep_indicator_stays() {
    const KEPT: &str =
        "a: >2+\n   alead\n  atext\nb: |2+\n   blead\n  btext\nc: |+\n  plain\nn: 1\n";
    let mut failures = Vec::new();
    for (key, word) in [("a", "atext"), ("b", "btext"), ("c", "plain")] {
        from_every_view(
            &mut failures,
            &format!("{key}: {word}"),
            Language::YAML,
            KEPT,
            &append(key, word, "X"),
            &KEPT.replacen(&format!("  {word}\n"), &format!("  {word}X\n"), 1),
        );
    }
    report(failures);
}

/// A new value whose first line starts with a space takes an indentation
/// indicator (relative to its key or dash) instead of becoming a quoted
/// string; folded text stays folded.
#[test]
fn a_block_scalar_given_a_leading_space_takes_an_indentation_indicator() {
    let mut failures = Vec::new();
    for (host, pointer, value, expected) in [
        (
            "# c\ns: |\n  x\nn: 1\n",
            "/s",
            "  x\n",
            "# c\ns: |2\n    x\nn: 1\n",
        ),
        (
            "# c\nm:\n  s: |-\n    x\nn: 1\n",
            "/m/s",
            " x",
            "# c\nm:\n  s: |2-\n     x\nn: 1\n",
        ),
        (
            "# c\nl:\n  - |\n    x\n  - y\nn: 1\n",
            "/l/0",
            "  x\n",
            "# c\nl:\n  - |2\n      x\n  - y\nn: 1\n",
        ),
        (
            "# c\nl:\n  - k: |\n      x\n  - k: z\nn: 1\n",
            "/l/0/k",
            "  x\ny\n",
            "# c\nl:\n  - k: |2\n        x\n      y\n  - k: z\nn: 1\n",
        ),
        (
            "# c\nf: >\n  x y\nn: 1\n",
            "/f",
            " x y\n",
            "# c\nf: >2\n   x y\nn: 1\n",
        ),
        (
            "# c\nf: >-\n  x\n  y\nn: 1\n",
            "/f",
            "x\n  indented\ny",
            "# c\nf: >-\n  x\n    indented\n  y\nn: 1\n",
        ),
    ] {
        from_every_view(
            &mut failures,
            &format!("{pointer} = {value:?}"),
            Language::YAML,
            host,
            &move |document: &mut Value| {
                *slot(document, pointer) = value.into();
            },
            expected,
        );
    }
    report(failures);
}

/// Text with a tab after a line's indentation is block content, in a list
/// item as under a key: the value is spliced in place and the comments
/// stay (round 16 regenerated the whole document for a list item).
#[test]
fn block_scalar_text_with_tabs_is_spliced_in_place_in_lists_and_maps() {
    let hosts = [
        (
            "list",
            "# c\nl:\n  - |\n    x\n  - y\nn: 1 # n\n",
            "/l/0",
            "  - ",
            "    ",
        ),
        (
            "map",
            "# c\nm:\n  s: |\n    x\nn: 1 # n\n",
            "/m/s",
            "  s: ",
            "    ",
        ),
        ("top", "# c\ns: |\n  x\nn: 1 # n\n", "/s", "s: ", "  "),
        (
            "list of maps",
            "# c\nl:\n  - s: |\n      x\n  - s: y\nn: 1 # n\n",
            "/l/0/s",
            "  - s: ",
            "      ",
        ),
        // A comment line and a key after the block, in the item's map
        // (round 17 wrote the whole document).
        (
            "list of maps with a comment",
            "# c\nl:\n  - s: |\n      x\n    # kc\n    j: 2\nn: 1 # n\n",
            "/l/0/s",
            "  - s: ",
            "      ",
        ),
        // A compact nested sequence (`- - |`).
        (
            "compact",
            "# c\nl:\n  - - |\n      x\n    - y\nn: 1 # n\n",
            "/l/0/0",
            "  - - ",
            "      ",
        ),
    ];
    let mut failures = Vec::new();
    for (label, host, pointer, prefix, indent) in hosts {
        let old = format!("{prefix}|\n{indent}x\n");
        for (value, header, body) in [
            ("x\n\tq", "|-", "x\n\tq"),
            ("x\n\t\tq", "|-", "x\n\t\tq"),
            ("x\n  \tq", "|-", "x\n  \tq"),
            ("x\n\tq\n", "|", "x\n\tq"),
            ("x\n\t q", "|-", "x\n\t q"),
        ] {
            let lines: Vec<String> = body
                .split('\n')
                .map(|line| format!("{indent}{line}\n"))
                .collect();
            let new = format!("{prefix}{header}\n{}", lines.concat());
            from_every_view(
                &mut failures,
                &format!("{label}: {value:?}"),
                Language::YAML,
                host,
                &move |document: &mut Value| {
                    // The XML view shows the nested list flat: its first item.
                    let pointer = if document.pointer(pointer).is_none() && pointer == "/l/0/0" {
                        "/l/0"
                    } else {
                        pointer
                    };
                    *slot(document, pointer) = value.into();
                },
                &host.replacen(&old, &new, 1),
            );
        }
    }
    report(failures);
}
