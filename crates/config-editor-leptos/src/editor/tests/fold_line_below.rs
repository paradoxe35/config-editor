//! QA 34, UX34-4: Ctrl+Enter on a folded line opens the new line after the
//! whole folded block (its hidden lines and the closing line a fold leaves
//! shown), at the folded line's indentation, and the fold stays closed, as
//! the other line commands take a fold as one unit. In every language.

use leptos::prelude::*;

use crate::editor::fold_ranges::FoldRange;
use crate::editor::keys::enter::{Pressed, enter_at};
use crate::editor::languages::Language;
use crate::editor::test_state::TestEditor;

/// Per language: the text, the folded line (0-based), and the text after
/// Ctrl+Enter on it.
const CASES: &[(Language, &str, usize, &str)] = &[
    (
        Language::YAML,
        "a: 1\nmetadata:\n  name: web\n  labels:\n    app: web\nspec: {}\n",
        1,
        "a: 1\nmetadata:\n  name: web\n  labels:\n    app: web\n\nspec: {}\n",
    ),
    (
        Language::JSON,
        "{\n  \"name\": \"x\",\n  \"scripts\": {\n    \"build\": \"tsc\"\n  },\n  \"private\": true\n}\n",
        2,
        "{\n  \"name\": \"x\",\n  \"scripts\": {\n    \"build\": \"tsc\"\n  },\n  \n  \"private\": true\n}\n",
    ),
    (
        Language::TOML,
        "[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\ntokio = \"1\"\n[dev]\ninsta = \"1\"\n",
        2,
        "[package]\nname = \"x\"\n[dependencies]\nserde = \"1\"\ntokio = \"1\"\n\n[dev]\ninsta = \"1\"\n",
    ),
    (
        Language::HCL,
        "resource \"x\" \"y\" {\n  tags = {\n    a = 1\n  }\n}\noutput \"o\" {}\n",
        1,
        "resource \"x\" \"y\" {\n  tags = {\n    a = 1\n  }\n  \n}\noutput \"o\" {}\n",
    ),
    (
        Language::XML,
        "<project>\n  <deps>\n    <id>a</id>\n  </deps>\n  <build/>\n</project>\n",
        1,
        "<project>\n  <deps>\n    <id>a</id>\n  </deps>\n  \n  <build/>\n</project>\n",
    ),
];

/// The UTF-16 offset of the end of `line` (0-based).
fn end_of_line(text: &str, line: usize) -> usize {
    text.split('\n')
        .take(line + 1)
        .map(|row| row.len() + 1)
        .sum::<usize>()
        - 1
}

#[test]
fn ctrl_enter_on_a_folded_line_opens_after_the_block_and_keeps_the_fold() {
    for &(language, text, header, expected) in CASES {
        let (_owner, state) = TestEditor::new(language, text).mount();
        assert!(state.fold_line(header), "{language:?}: folds line {header}");
        let folded = state.fold().folded.get_untracked();
        let caret = end_of_line(text, header);
        enter_at(state, (caret, caret), Pressed::LineBelow, None);
        assert_eq!(state.doc().text.get_untracked(), expected, "{language:?}");
        assert_eq!(
            state.fold().folded.get_untracked(),
            folded,
            "{language:?}: the fold stays closed where it was"
        );
        assert!(
            folded.iter().any(|fold: &FoldRange| fold.start == header),
            "{language:?}"
        );
        // The caret on the new line, after its indentation.
        let new_line = expected[..]
            .split('\n')
            .position(|row| row.trim().is_empty())
            .unwrap();
        let indent = expected.split('\n').nth(new_line).unwrap().len();
        let caret = end_of_line(expected, new_line - 1) + 1 + indent;
        assert_eq!(
            state.doc().caret.get_untracked(),
            caret,
            "{language:?}: caret"
        );
    }
}

/// A line not folded: Ctrl+Enter as before, the new line indented into the
/// block the line opens.
#[test]
fn ctrl_enter_on_a_line_not_folded_opens_right_below_it() {
    let text = "metadata:\n  name: web\n";
    let (_owner, state) = TestEditor::new(Language::YAML, text).mount();
    let caret = end_of_line(text, 0);
    enter_at(state, (caret, caret), Pressed::LineBelow, None);
    assert_eq!(
        state.doc().text.get_untracked(),
        "metadata:\n  \n  name: web\n"
    );
}
