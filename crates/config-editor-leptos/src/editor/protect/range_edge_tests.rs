//! A host range over line breaks protects them as text (BR38-3): a line
//! break typed inside it, or its own last line break taken away (the line
//! after it cut, everything after it deleted), changes its text and is
//! refused, however a line diff of the two texts would pair their lines.

use super::super::Language;
use super::guard::check;
use super::locks::{Sources, protect};

const TEXT: &str = "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n";

/// Whether the edit from `before` to `after` is let through with a host
/// range over `locked` (written once in `before`).
fn taken(before: &str, locked: &str, after: &str) -> bool {
    let start = before.find(locked).expect("the locked text");
    let units = |byte: usize| before[..byte].encode_utf16().count();
    let range = units(start)..units(start + locked.len());
    let behavior = Language::YAML.behavior();
    let protected = protect(
        behavior,
        before,
        Sources {
            ranges: std::slice::from_ref(&range),
            ..Sources::default()
        },
    );
    check(behavior, None, &protected, before, after).is_ok()
}

#[test]
fn a_line_break_typed_inside_a_range_over_lines_is_refused() {
    let locked = "el server configuration\nlisten";
    let after = TEXT.replacen("configuration\nlisten", "configuration\n\nlisten", 1);
    assert!(!taken(TEXT, locked, &after));
}

#[test]
fn a_range_keeps_its_last_line_break() {
    let locked = "public_address: tunnel.example.com\ninsecure_plaintext: false\n";
    // The line after it cut (a line typed there first).
    let typed = format!("{TEXT}v");
    let cut = typed.strip_suffix("\nv").expect("the typed line");
    assert!(!taken(&typed, locked, cut), "the line after it cut");
    // Everything after it deleted.
    let more = format!("{TEXT}tail: 1\n");
    assert!(
        !taken(&more, locked, TEXT.trim_end_matches('\n')),
        "the rest deleted"
    );
    // The line after it cut, its own line break kept: taken.
    assert!(taken(&more, locked, TEXT), "the next line cut");
}

#[test]
fn a_line_moved_above_a_range_ending_the_text_leaves_its_line_break() {
    // The last line locked with its line break; a line with none after it
    // (a suggestion accepted at the end) moved up above it (Alt+Up).
    let locked = "insecure_plaintext: false\n";
    let before = format!("{TEXT}tls: true");
    let moved = TEXT.replacen(locked, "tls: true\ninsecure_plaintext: false", 1);
    assert!(!taken(&before, locked, &moved));
}
