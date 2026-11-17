//! Runs the sweep found where an edit path and the span check disagreed
//! (P1): each must keep every protection and take every ordinary edit.

use super::acts::Act;
use super::assert_ok;
use super::cases::Lock;
use super::mount::Setup;
use crate::editor::Language;

/// P1: in an Edit-as view of a host whose range covers a field, a line
/// delete taking away the field's parent line moves the field to another
/// path: held (nothing reaches the host), or refused.
#[test]
fn a_line_delete_moving_a_range_covered_field_in_an_edit_as_view_is_held() {
    let block = Setup {
        case: 0,
        lock: Lock::Range,
        host: Some(Language::HCL),
        pairs: false,
    };
    assert_ok(
        &block,
        &[
            Act::ReplaceAll { pair: 0 },
            Act::Line {
                line: 1400,
                command: 3,
            },
            Act::Type { at: 49655, text: 5 },
            Act::Type { at: 13465, text: 2 },
            Act::Line {
                line: 15542,
                command: 3,
            },
        ],
    );
    let list = Setup {
        case: 1,
        lock: Lock::Range,
        host: Some(Language::TOML),
        pairs: false,
    };
    assert_ok(
        &list,
        &[
            Act::Entry {
                anchor: 2,
                name: 12,
                paste: false,
            },
            Act::Entry {
                anchor: 0,
                name: 12,
                paste: true,
            },
            Act::Entry {
                anchor: 2,
                name: 2,
                paste: false,
            },
            Act::Entry {
                anchor: 2,
                name: 10,
                paste: true,
            },
            Act::PasteCopy { line: 40679 },
            Act::Line {
                line: 14951,
                command: 3,
            },
        ],
    );
}

/// In Dot's one-line map, an entry with the locked key pasted right before
/// it (`password: 'x'; password: 'hunter2'`): the lock stays on its own
/// value, where the caret says the paste went, so its value cannot be
/// written over and Replace all takes the pasted entry's match.
#[test]
fn a_lock_stays_on_its_own_value_past_an_alike_entry_pasted_before_it() {
    let setup = Setup {
        case: 11,
        lock: Lock::Path,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 0,
                name: 16,
                paste: true,
            },
            Act::NewValue,
            Act::ReplaceAll { pair: 2 },
        ],
    );
}

/// An XML element read as a list of one, its lock named by its item's
/// path and by the element's: a `<` typed on a blank line beside it breaks
/// the text, and its text stays locked (under the item's path).
#[test]
fn a_lone_xml_item_stays_locked_while_a_tag_is_half_typed() {
    let setup = Setup {
        case: 7,
        lock: Lock::Path,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::PasteCopy { line: 59345 },
            Act::PasteCopy { line: 18688 },
            Act::Cut { line: 57883 },
            Act::Entry {
                anchor: 2,
                name: 2,
                paste: false,
            },
        ],
    );
}

/// P1: in an Edit-as view, a range-covered list item moved down under a
/// scalar (`port: 1` then `  - hunter2`, read as `1 - hunter2`): its list
/// reads it no more, though its pointer's index now names another item
/// that reads as pinned. Held, nothing reaching the host.
#[test]
fn a_range_covered_item_moved_out_of_its_list_is_held() {
    let setup = Setup {
        case: 1,
        lock: Lock::Range,
        host: Some(Language::TOML),
        pairs: false,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 0,
                name: 11,
                paste: true,
            },
            Act::Line {
                line: 37566,
                command: 0,
            },
            Act::Undo,
            Act::Line {
                line: 9722,
                command: 0,
            },
            Act::Line {
                line: 61969,
                command: 1,
            },
        ],
    );
}

/// The element of an XML list of one moved out of it, the list then
/// holding the user's item: a `<` typed beside it keeps the field locked.
#[test]
fn a_lone_xml_item_moved_out_stays_locked_while_a_tag_is_half_typed() {
    let setup = Setup {
        case: 7,
        lock: Lock::Path,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Entry {
                anchor: 2,
                name: 7,
                paste: false,
            },
            Act::Cut { line: 11918 },
            Act::Line {
                line: 55486,
                command: 1,
            },
            Act::Entry {
                anchor: 2,
                name: 7,
                paste: false,
            },
        ],
    );
}

/// P1: in Odd, a `,` typed after a placeholder's `:=` while the text does
/// not parse, its block's key renamed by Replace all (the token plain text
/// there), then a cut making the text parse: the placeholder reads glued
/// into its value. The change is held and the save waits.
#[test]
fn a_placeholder_read_glued_under_a_renamed_key_holds_the_save() {
    let setup = Setup {
        case: 9,
        lock: Lock::Secret,
        host: None,
        pairs: true,
    };
    assert_ok(
        &setup,
        &[
            Act::Line {
                line: 45187,
                command: 2,
            },
            Act::Entry {
                anchor: 1,
                name: 0,
                paste: false,
            },
            Act::Type { at: 4708, text: 10 },
            Act::ReplaceAll { pair: 6 },
            Act::Entry {
                anchor: 1,
                name: 16,
                paste: true,
            },
            Act::Cut { line: 48439 },
        ],
    );
}
