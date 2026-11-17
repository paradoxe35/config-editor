//! A host range over line breaks protects them as text: a line
//! break typed inside it, or its own last line break taken away (the line
//! after it cut, everything after it deleted), changes its text and is
//! refused, however a line diff of the two texts would pair their lines.

use super::super::Language;
use super::super::text::{TextRange, TextSize, text_size};
use super::guard::{check, check_at};
use super::locks::{Sources, protect};

const TEXT: &str = "# Tunnel server configuration\nlisten: 7835\npublic_address: tunnel.example.com\ninsecure_plaintext: false\n";

/// Whether the edit from `before` to `after` is let through with a host
/// range over `locked` (written once in `before`).
fn taken(before: &str, locked: &str, after: &str) -> bool {
    let start = before.find(locked).expect("the locked text");
    let units = text_size;
    let range = TextRange::new(units(start), units(start + locked.len()));
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

#[test]
fn a_line_cut_above_a_locked_one_alike_in_its_start_is_taken() {
    // `password2: x` cut above the locked `password: hunter2`: the texts
    // share `  password` on both sides of it.
    let before = "db:\n  user: app\n  password2: x\n  password: hunter2\n";
    let after = "db:\n  user: app\n  password: hunter2\n";
    let paths = [super::super::paths::FieldPath::from("/db/password")];
    let behavior = Language::YAML.behavior();
    let protected = protect(
        behavior,
        before,
        Sources {
            paths: &paths,
            ..Sources::default()
        },
    );
    let at = before.find("  password2").expect("the line");
    let caret = text_size(at);
    for caret in [None, Some(caret)] {
        let result = check_at(behavior, None, &protected, (before, after), caret);
        assert!(result.is_ok(), "caret {caret:?}: {result:?}");
    }
}

#[test]
fn of_two_like_lines_the_ranges_own_deleted_is_refused() {
    // A copy of the range's line pasted below it, then the range's own line
    // deleted (Ctrl+Shift+K there): the texts read alike whichever goes,
    // the caret says which one did, and the host's range follows it.
    let line = "insecure_plaintext: false\n";
    let before = format!("{TEXT}{line}");
    let start = TEXT.len() - line.len();
    let units = text_size;
    // The range over the line, its line break left out.
    let range = TextRange::new(units(start), units(TEXT.len() - 1));
    let behavior = Language::YAML.behavior();
    let protected = protect(
        behavior,
        &before,
        Sources {
            ranges: std::slice::from_ref(&range),
            ..Sources::default()
        },
    );
    let after = TEXT.to_string();
    let own = check_at(
        behavior,
        None,
        &protected,
        (&before, &after),
        Some(range.start()),
    );
    assert!(own.is_err(), "the range's own line deleted");
    // The copy deleted (the caret on its line): taken.
    let copy = check_at(
        behavior,
        None,
        &protected,
        (&before, &after),
        Some(range.end() + TextSize::from(1)),
    );
    assert!(copy.is_ok(), "the copy deleted");
}

#[test]
fn a_line_moved_in_between_a_ranges_lines_is_refused() {
    // The range over two lines, a line moved up between them (Alt+Up on
    // the line below): a line diff keeps both of its lines, one by one,
    // but not together.
    let locked = "l server configuration\nlisten:";
    let moved = "public_address: tunnel.example.com\n";
    let after = TEXT
        .replacen(moved, "", 1)
        .replacen("listen", &format!("{moved}listen"), 1);
    let start = TEXT.find(locked).expect("the locked text");
    let units = text_size;
    let range = TextRange::new(units(start), units(start + locked.len()));
    let behavior = Language::YAML.behavior();
    let protected = protect(
        behavior,
        TEXT,
        Sources {
            ranges: std::slice::from_ref(&range),
            ..Sources::default()
        },
    );
    let at = after.find(moved).expect("the moved line");
    for caret in [None, Some(units(at)), Some(units(at + moved.len() - 1))] {
        let result = check_at(behavior, None, &protected, (TEXT, &after), caret);
        assert!(result.is_err(), "caret {caret:?}: taken");
    }
}
