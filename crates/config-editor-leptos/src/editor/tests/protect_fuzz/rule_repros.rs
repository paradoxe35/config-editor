//! Runs the sweeps of the one span rule found: each must keep every
//! protection and take every ordinary edit.

use super::acts::Act;
use super::assert_ok;
use super::cases::Lock;
use super::mount::Setup;
use crate::editor::Language;

/// P1: an XML range's line moved and copied, a copy pasted, then undone:
/// the range's attribute stays as the host gave it.
#[test]
fn an_undo_after_line_copies_keeps_an_xml_range() {
    let setup = Setup {
        case: 8,
        lock: Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Line {
                line: 17662,
                command: 3,
            },
            Act::Line {
                line: 56418,
                command: 2,
            },
            Act::Line {
                line: 63347,
                command: 2,
            },
            Act::PasteCopy { line: 28281 },
            Act::Undo,
        ],
    );
}

/// A cut of a half-typed line beside a locked list is taken. Open
/// (over-refusal, also before the one rule): the cut line holds the key
/// holding the item, which a span rule refuses to cut.
#[test]
#[ignore = "open: a holder key cut where another key of the same name holds the item now"]
fn a_cut_of_a_half_typed_key_beside_a_locked_list_is_taken() {
    let setup = Setup {
        case: 1,
        lock: Lock::Path,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Type { at: 63533, text: 4 },
            Act::Entry {
                anchor: 1,
                name: 3,
                paste: false,
            },
            Act::Line {
                line: 52054,
                command: 3,
            },
            Act::Entry {
                anchor: 1,
                name: 16,
                paste: false,
            },
            Act::Line {
                line: 3600,
                command: 2,
            },
            Act::Entry {
                anchor: 0,
                name: 11,
                paste: false,
            },
            Act::Cut { line: 62932 },
        ],
    );
}

/// A cut of a half-typed list item in an Edit-as view is taken.
#[test]
fn a_cut_of_a_half_typed_item_in_an_edit_as_view_is_taken() {
    let setup = Setup {
        case: 1,
        lock: Lock::Schema,
        host: Some(Language::TOML),
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Type { at: 14508, text: 4 },
            Act::Entry {
                anchor: 3,
                name: 14,
                paste: false,
            },
            Act::Entry {
                anchor: 2,
                name: 13,
                paste: true,
            },
            Act::Entry {
                anchor: 2,
                name: 14,
                paste: false,
            },
            Act::Cut { line: 33783 },
        ],
    );
}

/// Of two alike lines below a range, the cut of one is taken.
#[test]
fn a_cut_of_one_of_two_alike_lines_beside_a_range_is_taken() {
    let first = Setup {
        case: 14,
        lock: Lock::Range,
        host: None,
        pairs: true,
    };
    assert_ok(
        &first,
        &[
            Act::Entry {
                anchor: 1,
                name: 6,
                paste: true,
            },
            Act::Line {
                line: 43096,
                command: 2,
            },
            Act::Line {
                line: 44836,
                command: 2,
            },
            Act::Line {
                line: 43332,
                command: 0,
            },
            Act::NewValue,
            Act::Cut { line: 16140 },
        ],
    );
    let second = Setup {
        case: 14,
        lock: Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &second,
        &[
            Act::PasteCopy { line: 50465 },
            Act::Entry {
                anchor: 3,
                name: 5,
                paste: false,
            },
            Act::Line {
                line: 63393,
                command: 1,
            },
            Act::Line {
                line: 30113,
                command: 2,
            },
            Act::NewValue,
            Act::Cut { line: 4569 },
        ],
    );
}

/// An entry typed below a half-typed list item in a declared language is
/// taken. Open (over-refusal, also before the one rule): the item's field
/// has no span, and the text kept for it is counted as written whole.
#[test]
#[ignore = "open: an unplaced field's value text counted in a typed word while broken"]
fn an_entry_below_a_half_typed_item_is_taken() {
    let setup = Setup {
        case: 14,
        lock: Lock::Schema,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 3,
                paste: false,
            },
            Act::Entry {
                anchor: 2,
                name: 2,
                paste: true,
            },
            Act::Entry {
                anchor: 1,
                name: 1,
                paste: true,
            },
            Act::Type { at: 49293, text: 4 },
            Act::Entry {
                anchor: 2,
                name: 2,
                paste: false,
            },
            Act::Entry {
                anchor: 3,
                name: 16,
                paste: false,
            },
        ],
    );
}

/// A list item a host range covers, at another index after a line typed
/// above it in a custom language: the range keeps its text, and the next
/// entry typed there is taken.
#[test]
fn an_entry_above_a_range_covered_item_is_taken() {
    let setup = Setup {
        case: 10,
        lock: Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 0,
                name: 10,
                paste: false,
            },
            Act::ReplaceAll { pair: 5 },
            Act::Type { at: 2118, text: 5 },
            Act::ReplaceAll { pair: 5 },
            Act::Cut { line: 38781 },
            Act::Entry {
                anchor: 0,
                name: 1,
                paste: false,
            },
        ],
    );
}

/// A TOML table typed a key at a time below a range's table, after a line
/// moved and undone: taken. Open (over-refusal, also before the one rule):
/// the lock of the field the range covers runs on over the blank lines
/// after it, so a key typed there writes inside its span.
#[test]
#[ignore = "open: a range-covered field's span runs on over blank lines after an undone line move"]
fn a_table_typed_below_a_range_after_an_undone_move_is_taken() {
    let setup = Setup {
        case: 4,
        lock: Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 12,
                paste: false,
            },
            Act::Line {
                line: 10085,
                command: 1,
            },
            Act::Undo,
            Act::Entry {
                anchor: 2,
                name: 5,
                paste: false,
            },
        ],
    );
}
