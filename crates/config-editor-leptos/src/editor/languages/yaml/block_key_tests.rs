//! A key written as a block scalar (`? |`): the caret anywhere in its text
//! is on that key, as on its `?` line and its `: value` line (round 25
//! gave the document's root inside the text).

use super::super::Language;
use crate::editor::paths::TextPosition;

#[test]
fn the_caret_in_a_block_scalar_key_is_on_that_key() {
    // Each document, and the key its lines from `first` to `last` (one-based)
    // write, the `?` line and the `:` line included.
    let cases: &[(&str, &str, usize, usize)] = &[
        ("? |\n  multi\n  line\n: v\nz: 1\n", "/multi\nline\n", 1, 4),
        ("? |-\n  one\n: v\n", "/one", 1, 3),
        ("? >\n  folded\n  text\n: v\n", "/folded text\n", 1, 4),
        ("a:\n  ? |\n    in\n    map\n  : 1\n", "/a/in\nmap\n", 2, 5),
        ("l:\n  - ? >-\n      folded\n      key\n    :\n      x: 1\n", "/l/0/folded key", 2, 5),
        ("? |\n  deep\n    more\n: v\n", "/deep\n  more\n", 1, 4),
    ];
    let mut failures = Vec::new();
    for (text, pointer, first, last) in cases {
        let expected = pointer.to_string();
        for line in *first..=*last {
            let written = text.split('\n').nth(line - 1).unwrap_or("");
            // On the `?` line, from the `?` on.
            let from = written.find('?').map_or(1, |at| at + 1);
            for column in from..=written.chars().count() + 1 {
                let found = Language::YAML
                    .path_at(text, TextPosition::new(line, column))
                    .map(|path| path.to_pointer());
                if found.as_deref() != Some(expected.as_str()) {
                    failures.push(format!("{text:?} at {line}:{column}: {found:?}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
