//! The smallest cases of the classes `known` lists, each as a test of the
//! behaviour the fix should give: ignored while its finding is open (run
//! them with `--ignored`); a fix removes the `#[ignore]` and the class's
//! entry in `known`.

use leptos::prelude::*;

use crate::editor::edit::Edit;
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

/// P2: once every CRLF line of a mixed file (mostly CRLF) is cut, the next
/// edit writes the lines left alone with the endings they have, not with
/// the file's most common ending once.
#[test]
fn lines_left_alone_keep_their_ending_after_the_last_crlf_line_goes() {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(Language::YAML, "a: 1\r\nb: 2\r\nx: 0\r\nc: 3\nd: 4\n");
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|all| all.push(text))
    }));
    let (_owner, state) = editor.mount();
    let replace = |from: usize, to: usize, with: &str| {
        let before = state.doc().text.get_untracked();
        let text = format!("{}{with}{}", &before[..from], &before[to..]);
        let caret = from + with.len();
        state.commit(Edit::input_units(
            before,
            (from, to),
            text,
            (caret, caret),
            "insertText".into(),
            0.0,
        ));
    };
    // The three CRLF lines cut: what is left was LF.
    replace(0, 15, "");
    assert_eq!(heard.get_untracked().last().unwrap(), "c: 3\nd: 4\n");
    // A digit typed on the last line.
    replace(9, 9, "0");
    assert_eq!(heard.get_untracked().last().unwrap(), "c: 3\nd: 40\n");
    // Another typed on the first stays LF too.
    replace(4, 4, "0");
    assert_eq!(heard.get_untracked().last().unwrap(), "c: 30\nd: 40\n");
}

/// P2: an empty line after a lone CR, given an LF, reaches the host as a
/// line of its own, not as one CRLF with the CR before it.
#[test]
fn an_empty_line_after_a_lone_cr_reaches_the_host() {
    super::endings::replay(super::endings::EndingsCase {
        language: "HCL",
        view: None,
        text: "name = 46279\nk = 92154\r\n\na {\r  a = [\r    true,\n    null,\r    5e-324\r\n  ]\r}\n".into(),
        steps: vec![super::endings::Step::Paste {
            at: 12545025,
            to: 5716122,
            text: "\n\n",
        }],
    });
}

/// A JSON or HCL syntax error after characters of several bytes on its
/// line is placed by characters, not by bytes (serde's column).
#[test]
fn a_json_syntax_error_is_placed_by_characters() {
    let column = |text: &str| Language::JSON.parse(text).unwrap_err().column;
    assert_eq!(
        column("{\"é😀\": 1 \"x\": 2}"),
        column("{\"ab\": 1 \"x\": 2}")
    );
    assert_eq!(column("\"\\\"\u{d7ff}"), Some(4));
    // HCL's reader (hcl-edit) counts bytes too.
    let column = |text: &str| Language::HCL.parse(text).unwrap_err().column;
    assert_eq!(column("éé\u{202e}"), column("ee\u{202e}"));
}

/// The custom test language (`odd_syntax`) placed its parse errors by
/// bytes too (a fixture's bug, as a host's language could have it).
#[test]
fn the_custom_language_places_its_errors_by_characters() {
    for text in ["枫", "a:=(𘩭"] {
        super::reading::replay(super::reading::ReadCase {
            language: "ODD",
            text: text.into(),
            at: 0,
        });
    }
}

/// P5: the suggestions' reading of an XML start tag holding many tokens
/// (a long line after `<a `) tokenizes the tag once, not once per token.
#[test]
fn reading_a_long_xml_start_tag_grows_with_it() {
    super::linear::replay(super::linear::LinearCase {
        language: "XML",
        shape: super::linear::Shape::LongLine,
        breaker: 9,
        unit: 4,
        carriage_returns: false,
    });
}

/// P5: HCL's tokenizer finds the end of a line once, not once per string
/// on it (a long line of strings after an open `"""`).
#[test]
fn reading_a_long_hcl_line_of_strings_grows_with_it() {
    super::linear::replay(super::linear::LinearCase {
        language: "HCL",
        shape: super::linear::Shape::LongLine,
        breaker: 3,
        unit: 3,
        carriage_returns: false,
    });
}

/// P5: the path at a caret after a run of XML end tags on one line steps
/// back over the whole run at once, reading the text from its top once
/// (one nested call per end tag could exhaust the stack).
#[test]
fn the_path_after_a_run_of_end_tags_reads_the_text_once() {
    use crate::editor::work_count::{Work, counted};
    let behavior = Language::XML.behavior();
    for depth in [30, 300] {
        let text = format!("<r>{}x{}</r>", "<a>".repeat(depth), "</a>".repeat(depth));
        let at = text.len() - "</r>".len();
        let (path, walks) = counted(Work::PathLookup, || {
            crate::editor::paths::path_at(behavior, &text, at)
        });
        assert!(path.is_some());
        assert!(walks <= 2, "{depth} end tags: {walks} walks");
    }
}

/// P5: the path at a caret (and HCL's suggestion context) on a long HCL
/// line of open lists (`a = [1, [1, …`) grows with the line: the keys on
/// the line look for its end once, not once per entry (`LineEndBytes`).
#[test]
fn reading_a_long_hcl_line_of_open_lists_grows_with_it() {
    super::linear::replay(super::linear::LinearCase {
        language: "HCL",
        shape: super::linear::Shape::LongLine,
        breaker: 0,
        unit: 4,
        carriage_returns: false,
    });
}

/// A JSON line of many strings (`"a", "a", …`, not a valid document): the
/// key under the pointer counted each string's column from the line's
/// start, the line's square for the caret's path (`ColumnRecount`).
#[test]
fn reading_a_long_json_line_of_strings_grows_with_it() {
    super::linear::replay(super::linear::LinearCase {
        language: "JSON",
        shape: super::linear::Shape::LongLine,
        breaker: 0,
        unit: 3,
        carriage_returns: false,
    });
}

/// A cut of the lines between a lone-CR line and an empty LF line, both
/// left alone, makes them neighbours: `\r` then `\n` would read as one CRLF
/// (a line lost), so the empty line is written CRLF, every other line
/// keeping its ending.
#[test]
fn an_empty_line_cut_up_to_a_lone_cr_keeps_every_other_ending() {
    use super::endings::Step;
    super::endings::replay(super::endings::EndingsCase {
        language: "HCL",
        view: None,
        text: "\u{feff}port = [\r  \"a b\",\r\n  null\r]\ré = [\r  true,\n  \"a b\"\n]\n".into(),
        steps: vec![
            Step::Type {
                at: 13630323,
                text: " ",
            },
            Step::Enter { at: 1952466 },
            Step::Type {
                at: 3946099,
                text: " ",
            },
            Step::Cut {
                at: 6181502,
                to: 14302226,
            },
        ],
    });
}

/// P1: a path named while the text does not parse, then the cut undone:
/// once the text parses, the lock is placed by its path (both `<a>`
/// elements), not followed from the half of the broken text it covered.
#[test]
fn a_path_lock_is_placed_by_its_path_once_the_text_parses() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "XML",
        schema: false,
        text: "<a>x\"y</a>\n<a>5e-324</a>\n".into(),
        steps: vec![
            Step::Cut {
                at: 5827886,
                to: 7702808,
            },
            Step::Paths { mask: 21 },
            Step::Undo,
        ],
    });
}

/// P1: Enter undone after a path is named: the HCL list's lock covers the
/// whole list again, as a fresh editor's.
#[test]
fn a_path_lock_covers_its_whole_list_after_an_undo() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "HCL",
        schema: false,
        text: "k = [\n  \"web\"\n]\na = [\n  null,\n]\n".into(),
        steps: vec![
            Step::Enter { at: 6918550 },
            Step::Paths { mask: 29 },
            Step::Undo,
        ],
    });
}

/// P1: a space typed at the end of a locked YAML list: the lock is placed
/// by its path, through the line's end, as in a fresh editor.
#[test]
fn a_lock_grows_with_a_space_typed_at_its_end() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "YAML",
        schema: true,
        text: "port:\n  - é😀\n  - a b\n".into(),
        steps: vec![Step::Type {
            at: 10305700,
            text: " ",
        }],
    });
}

/// A cut makes `port` a root key (marked read-only there), and the undo
/// puts it back under its table: the root field, which the host's text
/// never had, is not held.
#[test]
fn a_field_reading_as_in_the_hosts_text_is_not_held() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "TOML",
        schema: true,
        text: "\"a b\" = [\"\"]\nk = [true]\n\n[name]\n\"é\" = \"a b\"\n\"a b\" = 34452\n\n[name.name]\nk = 40687\na = false\nport = \"\"\"\nline\nnext\"\"\"\n".into(),
        steps: vec![
            Step::Type {
                at: 15146974,
                text: "1",
            },
            Step::Cut {
                at: 9030959,
                to: 4907816,
            },
            Step::Undo,
        ],
    });
}

/// New ranges from the host on the same text cover what they cover: the
/// field the ranges before covered is no longer locked.
#[test]
fn new_ranges_on_the_same_text_lock_only_what_they_cover() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "HCL",
        schema: true,
        text: "a = \"line\\nnext\"\n".into(),
        steps: vec![
            Step::Enter { at: 7601730 },
            Step::Ranges {
                line: Some(15505329),
            },
            Step::Type {
                at: 3697051,
                text: "x",
            },
            Step::Ranges {
                line: Some(8559509),
            },
        ],
    });
}

/// The host's ranges taken away while the text does not parse: once it
/// parses again, the field they covered is not locked.
#[test]
fn ranges_taken_away_leave_no_covered_field_locked() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "JSON",
        schema: true,
        text: "{\n  \"a\": 35082,\n  \"x_1\": \"a b\"\n}\n".into(),
        steps: vec![
            Step::Ranges {
                line: Some(15868982),
            },
            Step::Type {
                at: 14162372,
                text: "x",
            },
            Step::Ranges { line: None },
            Step::Undo,
        ],
    });
}

/// P1: Enter between a JSON key and its scalar: the path lock covers the
/// key and the value below it, as in a fresh editor.
#[test]
fn a_path_lock_covers_a_json_scalar_moved_below_its_key() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "JSON",
        schema: false,
        text: "{\n  \"a b\": [\n    \"\"\n  ],\n  \"x_1\": 95658\n}\n".into(),
        steps: vec![Step::Paths { mask: 11 }, Step::Enter { at: 13400940 }],
    });
}

/// P1: Enter between a JSON key and the `:` of its list: the path lock
/// covers the key, the `: [` line below it and the items, as in a fresh
/// editor.
#[test]
fn a_path_lock_covers_a_json_colon_moved_below_its_key() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "JSON",
        schema: true,
        text: "{\n  \"k\": \"web\",\n  \"port\": [\n    null,\n    \"web\"\n  ]\n}\n".into(),
        steps: vec![Step::Enter { at: 3757294 }],
    });
}

/// P2: cuts that leave a string's U+FEFF first in the text: it is text,
/// heard as on screen, and a file with a byte order mark keeps the mark
/// before it.
#[test]
fn a_feff_cut_to_the_start_is_text_not_a_byte_order_mark() {
    use super::endings::Step;
    for text in ["k := \"\u{feff}\"\n", "\u{feff}k := \"\u{feff}\"\n"] {
        super::endings::replay(super::endings::EndingsCase {
            language: "ODD",
            view: None,
            text: text.into(),
            steps: vec![
                Step::Cut {
                    at: 4232130,
                    to: 153915,
                },
                Step::Cut {
                    at: 1556216,
                    to: 5670085,
                },
                Step::Save,
            ],
        });
    }
}

/// A cut that leaves a string's U+FEFF first in a live editor's text: it
/// is a character of the document there, and a fresh editor on the same
/// document reads the same text and problems.
#[test]
fn a_feff_cut_to_the_start_of_a_live_editor_is_text_as_in_a_fresh_one() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "TOML",
        schema: false,
        text: "\"é\" = \"\u{feff}\"\n".into(),
        steps: vec![Step::Cut {
            at: 1348233,
            to: 11198418,
        }],
    });
}

/// P1: Enter between a JSON key inside a locked map and its scalar: the
/// map's lock covers the scalar below the key and the map's closer, as in
/// a fresh editor.
#[test]
fn a_map_lock_covers_a_scalar_moved_below_its_key_inside_it() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "JSON",
        schema: true,
        text: "{\n  \"é\": \"x\\\"y\",\n  \"port\": {\n    \"k\": [\n      5e-324\n    ],\n    \"name\": null\n  }\n}\n".into(),
        steps: vec![
            Step::Type {
                at: 13417223,
                text: " ",
            },
            Step::Enter { at: 10221065 },
        ],
    });
}

/// P1: a path named while an Enter breaks the YAML text, then the Enter
/// undone: the root `name` is locked, not the nested one, as in a fresh
/// editor.
#[test]
fn a_root_path_named_in_broken_yaml_locks_the_root_field_after_an_undo() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "YAML",
        schema: false,
        text:
            "k:\n  é:\n    x_1: 0.5\n    name: 39051\n    a b: 85951\nname: \"8080\"\nport: 22528\n"
                .into(),
        steps: vec![
            Step::Enter { at: 12341837 },
            Step::Paths { mask: 17 },
            Step::Undo,
        ],
    });
}

/// P1: a path lock given after an Enter, then the Enter undone: the lock
/// covers what a fresh editor's covers.
#[test]
fn a_path_lock_given_after_an_enter_follows_its_undo() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "JSON",
        schema: false,
        text: "{\n  \"k\": {\n    \"name\": 95016,\n    \"a b\": {\n      \"é\": \"a b\"\n    }\n  },\n  \"a\": [\n    true\n  ],\n  \"a b\": {\n    \"name\": false,\n    \"k\": \"\\u000b\",\n    \"a b\": {\n      \"x_1\": null\n    }\n  }\n}\n".into(),
        steps: vec![
            Step::Enter { at: 5308467 },
            Step::Paths { mask: 12 },
            Step::Undo,
        ],
    });
}

/// P1: paths named after an edit lock their fields where a fresh editor
/// does: the edit before is not applied to the new locks a second time.
#[test]
fn paths_named_after_an_edit_lock_where_a_fresh_editor_does() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "HCL",
        schema: true,
        text: "k = false\nport = \"\u{feff}\"\n".into(),
        steps: vec![
            Step::Type {
                at: 15743362,
                text: "1",
            },
            Step::Paths { mask: 0 },
        ],
    });
}

/// P1: ranges given after an edit lock where a fresh editor does.
#[test]
fn ranges_given_after_an_edit_lock_where_a_fresh_editor_does() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "JSON",
        schema: true,
        text: "{\n  \"a b\": 58052,\n  \"port\": [\n    5e-324,\n    true,\n    false\n  ],\n  \"name\": \"\u{a0}\"\n}\n".into(),
        steps: vec![
            Step::Type {
                at: 6305806,
                text: " ",
            },
            Step::Ranges {
                line: Some(1009077),
            },
        ],
    });
}

/// P1: an undo after new paths places the locks of the text it gives back
/// by their paths (YAML, an edit typed into a value first).
#[test]
fn an_undo_after_new_paths_places_the_locks_by_their_paths() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "YAML",
        schema: false,
        text: "a: 5671\na b:\n  - null\n".into(),
        steps: vec![
            Step::Type {
                at: 14008365,
                text: "1",
            },
            Step::Type {
                at: 10296072,
                text: " ",
            },
            Step::Paths { mask: 30 },
            Step::Undo,
        ],
    });
}

/// P1: an undo after new paths (TOML, a cut first).
#[test]
fn an_undo_after_new_paths_keeps_toml_locks_on_their_fields() {
    use super::live::Step;
    super::live::replay(super::live::LiveCase {
        language: "TOML",
        schema: true,
        text: "\n[k]\n\"a b\" = [0.5]\n\n[port]\n".into(),
        steps: vec![
            Step::Cut {
                at: 350795,
                to: 7224758,
            },
            Step::Paths { mask: 0 },
            Step::Undo,
        ],
    });
}
