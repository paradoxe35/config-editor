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

/// A cut of a half-typed line beside a locked list is taken: the line cut
/// holds a key of the list's name, and another key of that name holds the
/// item now.
#[test]
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
/// taken: the item's field has no span, so no edit is refused for it (a
/// text reading it otherwise holds it).
#[test]
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
/// moved and undone: taken. The lock of the field the range covers ends
/// where its value does, not over the blank lines after it.
#[test]
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

/// P1: a pasted copy of a placeholder under a renamed key, then text typed
/// at it inside its quotes: refused, the copy stays the token alone.
#[test]
fn text_typed_beside_a_copied_placeholder_inside_its_quotes_is_refused() {
    let setup = Setup {
        case: 13,
        lock: Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::PasteCopy { line: 16983 },
            Act::Entry {
                anchor: 1,
                name: 12,
                paste: false,
            },
            Act::Entry {
                anchor: 3,
                name: 12,
                paste: false,
            },
            Act::Entry {
                anchor: 0,
                name: 16,
                paste: false,
            },
            Act::Type { at: 63558, text: 2 },
            Act::Type {
                at: 63257,
                text: 10,
            },
        ],
    );
}

/// P1: a JSON list holding a pasted copy of a placeholder: no line command
/// leaves the copy with text beside it in its string.
#[test]
fn a_line_command_leaves_no_text_beside_a_copied_placeholder() {
    let setup = Setup {
        case: 3,
        lock: Lock::Secret,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Type { at: 11729, text: 8 },
            Act::Entry {
                anchor: 3,
                name: 10,
                paste: false,
            },
            Act::Entry {
                anchor: 3,
                name: 13,
                paste: true,
            },
            Act::PasteCopy { line: 34475 },
            Act::Type { at: 18973, text: 2 },
            Act::Entry {
                anchor: 2,
                name: 6,
                paste: false,
            },
            Act::Line {
                line: 41387,
                command: 3,
            },
        ],
    );
}

/// An entry typed below a DotBare list whose items the language cannot
/// locate (an item holding the separator, `* 'o:ther'`): taken. The field
/// has no span; its value reading otherwise would hold the save, nothing
/// is refused for it.
#[test]
fn an_entry_below_a_spanless_list_item_is_taken() {
    let setup = Setup {
        case: 14,
        lock: Lock::Schema,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Line {
                line: 20294,
                command: 2,
            },
            Act::Type { at: 53079, text: 4 },
            Act::Entry {
                anchor: 1,
                name: 3,
                paste: false,
            },
        ],
    );
}

/// As above, after the lines around the list are pasted and typed into.
#[test]
fn an_entry_below_a_pasted_spanless_list_item_is_taken() {
    let setup = Setup {
        case: 14,
        lock: Lock::Schema,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 13,
                paste: true,
            },
            Act::Entry {
                anchor: 3,
                name: 10,
                paste: true,
            },
            Act::PasteCopy { line: 49108 },
            Act::Type { at: 40308, text: 4 },
            Act::Entry {
                anchor: 1,
                name: 0,
                paste: false,
            },
        ],
    );
}

/// P1: a new secret value pasted over a placeholder that a line typed into
/// (`-, «…»`) left read beside other text (`keys: "-, «…» - other …"`):
/// refused, so the secret is never read (and shown) beside that text; the
/// refusal is no over-refusal.
#[test]
fn a_new_secret_over_a_placeholder_read_beside_text_is_refused() {
    let setup = Setup {
        case: 1,
        lock: Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 5,
                paste: false,
            },
            Act::Entry {
                anchor: 2,
                name: 6,
                paste: false,
            },
            Act::PasteCopy { line: 32709 },
            Act::Type {
                at: 39594,
                text: 10,
            },
            Act::NewValue,
        ],
    );
}

/// P1: in DotBare (a later entry wins), the range's line copied below it,
/// the locks read again on the same text (a Replace all matching only
/// locked text), then an entry of the field's name typed last: the field
/// the range covered stays locked through the reading again, so the entry
/// shadowing it is held, never heard.
#[test]
fn a_range_field_read_again_on_the_same_text_stays_locked() {
    let setup = Setup {
        case: 13,
        lock: Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::PasteCopy { line: 61637 },
            Act::ReplaceAll { pair: 5 },
            Act::ReplaceAll { pair: 5 },
            Act::Entry {
                anchor: 1,
                name: 16,
                paste: false,
            },
        ],
    );
}

/// P1: in Dot's one-line map, its key renamed, a `"` typed after `user:`
/// (a quote Dot declares and its parser reads as text) and then a `}`
/// typed before the placeholder inside its quotes: refused, though the
/// declared quote makes the text's reading name `user` as the field there.
#[test]
fn a_moved_placeholder_after_a_quote_its_parser_reads_as_text_never_glues() {
    let setup = Setup {
        case: 11,
        lock: Lock::Secret,
        host: None,
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::ReplaceAll { pair: 6 },
            Act::Type { at: 59986, text: 5 },
            Act::Entry {
                anchor: 1,
                name: 2,
                paste: false,
            },
            Act::Type { at: 42935, text: 9 },
        ],
    );
}

/// Over-refusal: in Dot (a later key wins), a locked list item at another
/// index (held), its list's key copied down above the items, then the
/// first `keys:` (now empty) cut: taken. The copy holds the items, as the
/// parse reads them, so it is the item's holder key.
#[test]
fn a_cut_of_a_list_key_copied_above_its_held_item_is_taken() {
    let path = Setup {
        case: 12,
        lock: Lock::Path,
        host: None,
        pairs: false,
    };
    assert_ok(
        &path,
        &[
            Act::Entry {
                anchor: 3,
                name: 0,
                paste: false,
            },
            Act::Cut { line: 44898 },
            Act::Line {
                line: 41252,
                command: 2,
            },
            Act::Cut { line: 27525 },
        ],
    );
    let schema = Setup {
        case: 14,
        lock: Lock::Schema,
        host: None,
        pairs: false,
    };
    assert_ok(
        &schema,
        &[
            Act::PasteCopy { line: 14625 },
            Act::Entry {
                anchor: 3,
                name: 1,
                paste: false,
            },
            Act::Line {
                line: 8772,
                command: 2,
            },
            Act::Cut { line: 50351 },
        ],
    );
}

/// P1 and over-refusal: the range's line pasted above it makes two alike
/// lines, and the range stays on its own line (the second): an entry
/// typed into the copy is taken, one typed into the range's line refused,
/// in a declared language and in XML.
#[test]
fn of_a_range_line_and_its_copy_only_the_copy_takes_an_entry() {
    use leptos::prelude::*;

    use super::mount::mount;
    use crate::editor::edit::Edit;

    for (case, anchor, written) in [
        (11, "<user: 'app';", " zz: 'x';"),
        (8, "user=\"app\"", " zz=\"5\""),
    ] {
        let setup = Setup {
            case,
            lock: Lock::Range,
            host: None,
            pairs: false,
        };
        let mounted = mount(&setup).expect("the case mounts");
        let state = mounted.state;
        let line = usize::from(case == 8);
        let entry = |line: usize| {
            let before = state.doc().text.get_untracked();
            let start: usize = before.split_inclusive('\n').take(line).map(str::len).sum();
            let at = start + before[start..].find(anchor).expect("the anchor") + anchor.len();
            let after = format!("{}{written}{}", &before[..at], &before[at..]);
            let from = before[..at].encode_utf16().count();
            let caret = from + written.encode_utf16().count();
            state.commit(Edit::new_units(before, (from, from), after, caret))
        };
        let copied = super::acts::make(&setup, &mounted, &Act::PasteCopy { line: line as u16 });
        assert!(copied.is_ok(), "the copy is pasted ({case})");
        assert!(!entry(line + 1), "the range's line takes no entry ({case})");
        assert!(entry(line), "the copy takes an entry ({case})");
    }
    let dot = Setup {
        case: 11,
        lock: Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &dot,
        &[
            Act::PasteCopy { line: 62418 },
            Act::Type { at: 64047, text: 7 },
            Act::Entry {
                anchor: 0,
                name: 15,
                paste: false,
            },
        ],
    );
    let xml = Setup {
        case: 8,
        lock: Lock::Range,
        host: None,
        pairs: false,
    };
    assert_ok(
        &xml,
        &[
            Act::PasteCopy { line: 23661 },
            Act::ReplaceAll { pair: 0 },
            Act::Entry {
                anchor: 1,
                name: 14,
                paste: true,
            },
        ],
    );
}

/// Over-refusal: in XML, the list's closing tag moved up above its last
/// item (the locked item left alone in its list, written once), an item
/// added to the list, then an entry typed a key at a time: taken. The
/// field held while written once (`…/key`) is the one locked as a list's
/// item (`…/key/0`), at the same pinned value, so the hold is not lost.
#[test]
fn an_entry_typed_after_an_item_moved_out_of_a_locked_xml_list_is_taken() {
    for (pairs, acts) in [
        (
            false,
            [
                Act::Line {
                    line: 49126,
                    command: 1,
                },
                Act::Entry {
                    anchor: 0,
                    name: 12,
                    paste: true,
                },
                Act::Entry {
                    anchor: 2,
                    name: 12,
                    paste: false,
                },
            ],
        ),
        (
            true,
            [
                Act::Line {
                    line: 27628,
                    command: 1,
                },
                Act::Entry {
                    anchor: 0,
                    name: 8,
                    paste: true,
                },
                Act::Entry {
                    anchor: 3,
                    name: 15,
                    paste: false,
                },
            ],
        ),
    ] {
        let setup = Setup {
            case: 7,
            lock: Lock::Path,
            host: None,
            pairs,
        };
        assert_ok(&setup, &acts);
    }
}

/// Over-refusal: in XML, the locked item of a list moved out of it (the
/// list left with one element, which its host path `…/key/0` now names, so
/// it reads as the lone `…/key`), another item added, then an entry typed a
/// key at a time: taken. The host path keeps the one spelling it is held
/// under, so no second field (and no second hold, the lone element's,
/// which nothing re-places once the list has two items) is made of it.
#[test]
fn xml_list_items_outside_their_list_next_to_a_held_item_take_an_entry() {
    for (pairs, acts) in [
        (
            false,
            vec![
                Act::Line {
                    line: 61706,
                    command: 1,
                },
                Act::Entry {
                    anchor: 1,
                    name: 2,
                    paste: true,
                },
                Act::Entry {
                    anchor: 3,
                    name: 15,
                    paste: false,
                },
            ],
        ),
        (
            true,
            vec![
                Act::Line {
                    line: 53977,
                    command: 0,
                },
                Act::PasteCopy { line: 44105 },
                Act::Entry {
                    anchor: 2,
                    name: 8,
                    paste: true,
                },
                Act::Entry {
                    anchor: 1,
                    name: 4,
                    paste: false,
                },
            ],
        ),
    ] {
        let setup = Setup {
            case: 7,
            lock: Lock::Path,
            host: None,
            pairs,
        };
        assert_ok(&setup, &acts);
    }
}

/// Over-refusal: a list's key line copied above its items, then cut there:
/// the cut is taken, the copy holds the items. P1: whichever line is cut
/// or the value pasted over next, the held item stays protected.
#[test]
fn a_cut_of_a_list_key_with_a_copy_holding_its_items_is_taken() {
    let setup = Setup {
        case: 14,
        lock: Lock::Path,
        host: None,
        pairs: false,
    };
    let base = || {
        vec![
            Act::Line {
                line: 29977,
                command: 0,
            },
            Act::Type { at: 25433, text: 4 },
            Act::Line {
                line: 45228,
                command: 2,
            },
            Act::Cut { line: 33489 },
            Act::Cut { line: 33260 },
        ]
    };
    assert_ok(&setup, &base());
    for line in 0..4 {
        let mut acts = base();
        acts.push(Act::Cut { line });
        assert_ok(&setup, &acts);
    }
    let mut acts = base();
    acts.push(Act::NewValue);
    assert_ok(&setup, &acts);
}
