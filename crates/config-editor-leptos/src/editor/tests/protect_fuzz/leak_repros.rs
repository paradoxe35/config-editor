//! P1: runs where a typed secret was quoted in a problem message, once
//! the field holding it read in another shape (an XML element repeated
//! into a list, a broken text read as one string).

use super::acts::Act;
use super::assert_ok;
use super::cases::Lock;
use super::mount::Setup;
use crate::editor::Language;

fn secret(case: usize, host: Option<Language>, pairs: bool) -> Setup {
    Setup {
        case,
        lock: Lock::Secret,
        host,
        pairs,
    }
}

#[test]
fn a_secret_in_a_repeated_xml_element_is_not_quoted() {
    assert_ok(
        &secret(6, None, false),
        &[
            Act::NewValue,
            Act::Entry {
                anchor: 3,
                name: 15,
                paste: false,
            },
        ],
    );
}

#[test]
fn a_secret_in_a_repeated_xml_attribute_holder_is_not_quoted() {
    assert_ok(
        &secret(8, None, true),
        &[
            Act::NewValue,
            Act::Entry {
                anchor: 2,
                name: 15,
                paste: true,
            },
        ],
    );
}

#[test]
fn a_secret_in_a_pasted_xml_line_copy_is_not_quoted() {
    assert_ok(
        &secret(8, None, false),
        &[Act::PasteCopy { line: 47211 }, Act::NewValue],
    );
}

#[test]
fn a_secret_in_a_yaml_list_read_as_one_string_is_not_quoted() {
    assert_ok(
        &secret(1, None, false),
        &[
            Act::Entry {
                anchor: 0,
                name: 5,
                paste: false,
            },
            Act::Entry {
                anchor: 0,
                name: 6,
                paste: false,
            },
            Act::Type {
                at: 23814,
                text: 11,
            },
            Act::NewValue,
        ],
    );
}

#[test]
fn a_secret_in_a_root_read_as_a_list_is_not_quoted() {
    assert_ok(
        &secret(1, None, false),
        &[
            Act::Cut { line: 51903 },
            Act::Cut { line: 57555 },
            Act::NewValue,
        ],
    );
}

#[test]
fn a_secret_in_a_yaml_view_of_xml_read_as_one_string_is_not_quoted() {
    assert_ok(
        &secret(1, Some(Language::XML), true),
        &[
            Act::Cut { line: 34707 },
            Act::NewValue,
            Act::Entry {
                anchor: 1,
                name: 6,
                paste: true,
            },
            Act::Type { at: 54098, text: 8 },
        ],
    );
}

#[test]
fn a_secret_in_a_dot_map_read_as_one_string_is_not_quoted() {
    assert_ok(
        &secret(11, None, false),
        &[Act::NewValue, Act::Type { at: 36723, text: 0 }],
    );
}

/// A syntax message quoting a broken XML name that runs into the
/// element's new secret value hides the value.
#[test]
fn a_secret_after_a_broken_xml_name_is_not_quoted_in_the_syntax_message() {
    assert_ok(
        &secret(6, None, false),
        &[
            Act::Entry {
                anchor: 3,
                name: 5,
                paste: false,
            },
            Act::Entry {
                anchor: 0,
                name: 15,
                paste: true,
            },
            Act::Type { at: 2317, text: 5 },
            Act::Type { at: 23299, text: 5 },
            Act::Entry {
                anchor: 1,
                name: 2,
                paste: true,
            },
            Act::NewValue,
        ],
    );
}

/// A YAML list edited as XML that XML would read otherwise: the save's
/// reason hides the secret.
#[test]
fn a_secret_in_a_refused_xml_save_of_a_yaml_list_is_not_quoted() {
    assert_ok(
        &secret(1, Some(Language::XML), false),
        &[
            Act::NewValue,
            Act::Entry {
                anchor: 2,
                name: 6,
                paste: false,
            },
            Act::Type { at: 21490, text: 4 },
            Act::Pause,
        ],
    );
}
