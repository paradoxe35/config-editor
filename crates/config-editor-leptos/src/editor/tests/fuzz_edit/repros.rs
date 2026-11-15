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
fn toggling_an_xml_line_of_overlapping_comment_markers_does_not_panic() {
    for text in ["<!--->", "<!--->\n", "<a/>\n<!--->\n"] {
        let _ = Language::XML.behavior().toggle_comments(text, 0, 0);
    }
}

/// FE38-8: suggestions in a TOML view whose header ends with a character
/// of several bytes panic (`toml::style::key_places` cuts it in half).
#[test]
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
fn long_decimals_convert_exactly_or_are_listed() {
    super::convert::replay(
        Language::JSON,
        Language::TOML,
        "{\n  \"web\": [123456789.123456789]\n}\n",
    );
}

/// FE38-9: Enter after a top-level YAML entry keeps its column.
#[test]
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

/// The hooks sweep: suggestions in an XML end tag whose local name, past
/// a prefix, is a character of several bytes, the caret before the prefix.
#[test]
fn suggestions_before_a_prefixed_wide_end_tag_name_do_not_panic() {
    super::hooks::replay(super::hooks::HookCase {
        language: Language::XML,
        text: "<b</:é".into(),
        at: 5113420,
    });
}

/// The scale run: Ctrl+/ in an Edit-as XML view after random edits
/// with a character of several bytes in it.
#[test]
fn commenting_xml_lines_after_wide_typing_does_not_panic() {
    super::undo::replay(super::undo::UndoCase {
        host: Language::YAML,
        view: Some(Language::XML),
        schema: false,
        text: "allow:\n  x_1:\n    a: false\n  name: web\n  allow:\n    - 24860\n    - host\nb: false\n".into(),
        interleaved: false,
        actions: vec![
            Enter { at: 9288228 },
            Tab {
                at: 16399464,
                to: 16399464,
                shift: true,
            },
            Line {
                at: 14993907,
                to: 14993907,
                command: 1,
            },
            LineBelow { at: 10680625 },
            ReplaceAll {
                query: "^ +",
                with: "",
                options: 7,
            },
            ReplaceOne {
                query: "x_1",
                with: "p",
                options: 7,
                index: 3,
            },
            ReplaceAll {
                query: "[0-9]+",
                with: "",
                options: 7,
            },
            Enter { at: 11165439 },
            ReplaceAll {
                query: "e",
                with: "$0-",
                options: 1,
            },
            Type {
                at: 12957977,
                text: "é",
            },
            LineBelow { at: 12237063 },
            Enter { at: 14723974 },
            Type {
                at: 7113092,
                text: "{",
            },
            Comment {
                at: 12577518,
                to: 12577518,
            },
        ],
    });
}

/// FE38-11: suggestions accepted in a row on a YAML key line do not
/// panic.
#[test]
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

/// SW38-5: an indent, Enter and Shift+Tab in an HCL view leave an empty
/// link: each link stays a range of the text after an edit.
#[test]
fn an_indent_enter_and_outdent_in_a_view_keep_every_link_in_the_text() {
    super::undo::replay(super::undo::UndoCase {
        host: Language::JSON,
        view: Some(Language::HCL),
        schema: false,
        text: "{\n  \"listen\": 1,\n  \"tcp\": {\"nodelay\": true},\n  \"self\": {\n    \"allow\": [\"a\", \"b\", \"c\"]\n  },\n  \"clients\": {\n    \"acme\": {\n      \"secret\": \"s1\",\n      \"max_connections\": 2\n    },\n    \"beta\": {\n      \"secret\": \"s2\"\n    }\n  },\n  \"limits\": {\n    \"max_pending\": 5\n  }\n}\n".into(),
        interleaved: true,
        actions: vec![
            Indent {
                at: 11403920,
                to: 11053544,
                outdent: false,
            },
            Enter { at: 2674138 },
            Tab {
                at: 7422702,
                to: 10870560,
                shift: true,
            },
        ],
    });
}

#[test]
fn every_undo_step_after_a_paste_then_typing_changes_the_text() {
    super::undo::replay(super::undo::UndoCase {
        host: Language::YAML,
        view: None,
        schema: false,
        text: "a:\n".into(),
        interleaved: true,
        actions: vec![
            Paste {
                at: 1357702,
                to: 8363232,
            },
            Type {
                at: 7181582,
                text: "-",
            },
            Enter { at: 1249606 },
        ],
    });
}

#[test]
fn undo_steps_in_an_xml_host_shown_as_json_follow_the_grouping() {
    super::undo::replay(super::undo::UndoCase {
        host: Language::XML,
        view: Some(Language::JSON),
        schema: false,
        text: "<a>16762</a>\n".into(),
        interleaved: false,
        actions: vec![
            Enter { at: 7682218 },
            Enter { at: 4143467 },
            Type {
                at: 11402574,
                text: "-",
            },
            Tab {
                at: 67002,
                to: 2901362,
                shift: false,
            },
            Type {
                at: 2618730,
                text: ">",
            },
        ],
    });
}
