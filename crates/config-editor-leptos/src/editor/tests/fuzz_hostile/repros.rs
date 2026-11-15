//! The smallest cases of the classes `known` lists, each as a test of the
//! behaviour the fix should give: ignored while its finding is open (run
//! them with `--ignored`); a fix removes the `#[ignore]` and the class's
//! entry in `known`.

use leptos::prelude::*;

use crate::editor::edit::Edit;
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

/// HS38-1: once every CRLF line of a mixed file (mostly CRLF) is cut, the next edit
/// wrote every line with CRLF (the file's most common ending once), the
/// lines left alone included.
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
        state.commit(Edit::input(
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

/// HS38-3: an empty line after a lone CR, given an LF, was handed back as
/// one CRLF with the CR before it: a line lost in what the host heard.
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

/// HS38-4: a JSON or HCL syntax error after characters of several bytes on its
/// line was placed by bytes (serde's column), past where it is.
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

/// HS38-5: the suggestions' reading of an XML start tag holding many
/// tokens (a long line after `<a `) tokenized the tag again for each of
/// them: time growing with the square of the line.
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

/// HS38-6: HCL's tokenizer found the end of the line again for each
/// string on it: a long line of strings (after an open `"""`) cost its
/// square in every reading.
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

/// HS38-2: the path at a caret after a run of XML end tags on one line
/// walked back one end tag at a time, reading the text from its top for
/// each (and nesting a call for each: a long run could exhaust the stack).
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
