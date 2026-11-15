//! The smallest cases of the classes `known` lists, each as a test of the
//! behaviour the fix should give: ignored while its finding is open (run
//! them with `--ignored`); a fix removes the `#[ignore]` and the class's
//! entry in `known`.

use leptos::prelude::*;

use super::actions::Action::*;
use crate::editor::keys::line_edit::LineCommand;
use crate::editor::keys::lines::{Command, run};
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

/// FE38-7: Ctrl+/ on a line holding overlapping comment markers panics.
#[test]
#[ignore = "FE38-7 open"]
fn toggling_an_xml_line_of_overlapping_comment_markers_does_not_panic() {
    for text in ["<!--->", "<!--->\n", "<a/>\n<!--->\n"] {
        let _ = Language::XML.behavior().toggle_comments(text, 0, 0);
    }
}

/// FE38-8: suggestions in a TOML view whose header ends with a character
/// of several bytes panic (`toml::style::key_places` cuts it in half).
#[test]
#[ignore = "FE38-8 open"]
fn suggestions_beside_a_toml_header_ending_in_non_ascii_do_not_panic() {
    super::undo::replay(super::undo::UndoCase {
        host: Language::YAML,
        view: Some(Language::TOML),
        schema: true,
        text: "listen: 1 # port\ntcp: {nodelay: true}\nself:\n  allow: [a, b, c] # cidrs\nclients:\n  acme:\n    secret: s1\n    max_connections: 2\n  beta:\n    secret: s2\nlimits:\n  max_pending: 5 # mp\n".into(),
        interleaved: false,
        actions: vec![
            Enter { at: 10455521 },
            Type { at: 6042376, text: "x" },
            Type { at: 4563167, text: "é" },
            Enter { at: 10404729 },
            Complete { at: 5470963, pick: 0 },
        ],
    });
}

/// FE38-5: moving the last lines down in a text without a last newline
/// leaves the caret past the text's end.
#[test]
#[ignore = "FE38-5 open"]
fn moving_a_line_down_to_the_end_keeps_the_caret_in_the_text() {
    let text = "a\nb";
    let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
    run(
        state,
        Command::Lines(LineCommand::Move { down: true }),
        (0, 2),
        false,
    );
    assert_eq!(state.doc().text.get_untracked(), "b\na");
    assert!(state.doc().caret.get_untracked() <= 3);
    // A blank first line moved down and back up again.
    super::valid::replay(super::valid::ValidCase {
        language: Language::HCL,
        text: "\na = \"\"".into(),
        first: 1,
        last: 1,
    });
}

/// FE38-3: Ctrl+/ twice on an XML element holding a comment gives the
/// text back.
#[test]
#[ignore = "FE38-3 open"]
fn an_xml_element_holding_a_comment_is_commented_and_uncommented() {
    let text = "<a>1</a>\n<c>\n  <!-- n -->\n  <b>2</b>\n</c>\n";
    let behavior = Language::XML.behavior();
    let at = text.find("</c>").expect("the closer");
    let (once, from, to) = behavior
        .toggle_comments(text, at, at + 4)
        .expect("commented");
    let (from, to) =
        crate::editor::commands::toggled_selection(text, (at, at + 4), &once, (from, to));
    let (twice, _, _) = behavior
        .toggle_comments(&once, from, to)
        .expect("uncommented");
    assert_eq!(twice, text);
}

/// FE38-2: a replace that changes nothing is no undo step.
#[test]
#[ignore = "FE38-2 open"]
fn a_replace_changing_nothing_is_no_undo_step() {
    super::undo::replay(super::undo::UndoCase {
        host: Language::YAML,
        view: None,
        schema: false,
        text: "listen: 1 # keep\n".into(),
        interleaved: false,
        actions: vec![ReplaceAll {
            query: " ",
            with: " ",
            options: 0,
        }],
    });
}

/// FE38-6: Enter after a YAML block's opening line indents to where its
/// entries are written (four spaces in a file indented by two elsewhere).
#[test]
#[ignore = "FE38-6 open"]
fn enter_after_a_yaml_key_indents_to_its_entries() {
    super::valid::replay(super::valid::ValidCase {
        language: Language::YAML,
        text: "self:\n  allow: 1\npool:\n    max: 2\n".into(),
        first: 3,
        last: 3,
    });
}

/// FE38-1: a decimal with more digits than a double holds converts with
/// its digits, or the conversion lists it as a loss.
#[test]
#[ignore = "FE38-1 open"]
fn long_decimals_convert_exactly_or_are_listed() {
    super::convert::replay(
        Language::JSON,
        Language::TOML,
        "{\n  \"web\": [123456789.123456789]\n}\n",
    );
}

/// FE38-9: Enter after a top-level YAML entry keeps its column.
#[test]
#[ignore = "FE38-9 open"]
fn enter_after_a_top_level_yaml_entry_keeps_its_column() {
    super::valid::replay(super::valid::ValidCase {
        language: Language::YAML,
        text: "port:\n        tls: x\"y\ntls: 19458\n".into(),
        first: 3,
        last: 3,
    });
}

/// FE38-4: Ctrl+/ twice on a YAML comment of prose gives the text back.
#[test]
#[ignore = "FE38-4 open"]
fn a_yaml_comment_of_prose_is_uncommented_and_commented_back() {
    super::valid::replay(super::valid::ValidCase {
        language: Language::YAML,
        text: "l:\n  - name: a\n    sub:\n      # about 1\n      - 2\n".into(),
        first: 4,
        last: 4,
    });
}

/// FE38-10: suggestions in a half-typed TOML header do not panic.
#[test]
#[ignore = "FE38-10 open"]
fn suggestions_in_a_half_typed_toml_header_do_not_panic() {
    super::undo::replay(super::undo::UndoCase {
        host: Language::HCL,
        view: Some(Language::TOML),
        schema: true,
        text: "secret = <<EOT\nEOT\nself {\n  allow = [\"a\", \"b\"]\n}\n".into(),
        interleaved: true,
        actions: vec![
            Backspace {
                at: 8151580,
                count: 4,
            },
            Complete {
                at: 14215309,
                pick: 6,
            },
        ],
    });
}

/// FE38-11: suggestions accepted in a row on a YAML key line do not
/// panic.
#[test]
#[ignore = "FE38-11 open"]
fn suggestions_accepted_in_a_row_do_not_panic() {
    super::undo::replay(super::undo::UndoCase {
        host: Language::YAML,
        view: None,
        schema: true,
        text: "web:\n  - \"\"\n  - false\nallow:\n  mode: web\n".into(),
        interleaved: false,
        actions: vec![
            Line {
                at: 9998329,
                to: 9998329,
                command: 1,
            },
            Type {
                at: 16531850,
                text: "x",
            },
            Pair {
                at: 5918744,
                to: 6665152,
                opening: '\'',
            },
            Type {
                at: 1010328,
                text: " ",
            },
            Type {
                at: 11384061,
                text: ": ",
            },
            Complete {
                at: 3372012,
                pick: 3,
            },
            Complete {
                at: 3363363,
                pick: 3,
            },
            Type {
                at: 1910421,
                text: "1",
            },
            Complete {
                at: 4930958,
                pick: 3,
            },
        ],
    });
}
