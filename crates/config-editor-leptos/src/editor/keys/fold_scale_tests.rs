//! A line command after "fold all" costs what it costs with a few folds:
//! the closed folds' units (each one's lines and the line closing it) are
//! found from one reading of the text for all of them (each fold read the
//! whole text again: 7.5 s for one Alt+Down on a 6,210-line file after
//! `foldAll()`).

use leptos::prelude::*;

use super::super::languages::Language;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted};
use super::line_edit::LineCommand;
use super::lines::{Command, run};

/// `blocks` blocks of two fields each, in `language`.
pub(in crate::editor) fn blocks(language: Language, blocks: usize) -> String {
    let each = |index: usize| match language.label() {
        "YAML" => format!("  svc_{index}:\n    image: \"app-{index}\"\n    port: {index}\n"),
        "JSON" => format!(
            "    \"svc_{index}\": {{\n      \"image\": \"app-{index}\",\n      \"port\": {index}\n    }}{}\n",
            if index + 1 == blocks { "" } else { "," }
        ),
        "TOML" => format!("[services.svc_{index}]\nimage = \"app-{index}\"\nport = {index}\n\n"),
        "HCL" => {
            format!("  svc_{index} {{\n    image = \"app-{index}\"\n    port  = {index}\n  }}\n")
        }
        _ => format!(
            "    <svc_{index}>\n      <image>app-{index}</image>\n      <port>{index}</port>\n    </svc_{index}>\n"
        ),
    };
    let body: String = (0..blocks).map(each).collect();
    match language.label() {
        "YAML" => format!("name: demo\nservices:\n{body}"),
        "JSON" => format!("{{\n  \"name\": \"demo\",\n  \"services\": {{\n{body}  }}\n}}\n"),
        "TOML" => format!("name = \"demo\"\n\n{body}"),
        "HCL" => format!("name = \"demo\"\nservices {{\n{body}}}\n"),
        _ => format!("<root>\n  <name>demo</name>\n  <services>\n{body}  </services>\n</root>\n"),
    }
}

/// The whole-text readings for constructs, and the reads of a text from
/// its top for one place in it, one line command makes with every fold of
/// a text of `count` blocks closed but the outermost; and the folds.
fn line_command_scans(language: Language, count: usize, command: Command) -> ([usize; 2], usize) {
    let text = blocks(language, count);
    let (_owner, state) = TestEditor::new(language, &text).mount();
    let all = state.fold().fold_ranges.get_untracked();
    // The blocks closed, the document's own outer blocks open.
    let closed: Vec<_> = all.into_iter().filter(|fold| fold.start > 2).collect();
    let folds = closed.len();
    state.set_folds(closed);
    let line = text.find("svc_1").expect("the second block");
    let at = text[..text[..line].rfind('\n').map_or(0, |at| at + 1)]
        .encode_utf16()
        .count();
    let (((), scans), seeks) = counted(Work::LineSeek, || {
        counted(Work::ConstructScan, || run(state, command, (at, at), false))
    });
    assert_ne!(
        state.doc().text.get_untracked(),
        text,
        "{language}: changed"
    );
    ([scans, seeks], folds)
}

#[test]
fn a_line_command_reads_the_text_for_its_constructs_once_whatever_the_folds() {
    for language in Language::ALL.iter().copied() {
        for (name, command) in [
            ("Alt+Down", Command::Lines(LineCommand::Move { down: true })),
            ("Alt+Up", Command::Lines(LineCommand::Move { down: false })),
            (
                "Shift+Alt+Down",
                Command::Lines(LineCommand::Copy { down: true }),
            ),
            ("Ctrl+Shift+K", Command::Lines(LineCommand::Delete)),
            ("Ctrl+]", Command::Indent { outdent: false }),
        ] {
            let (few, few_folds) = line_command_scans(language, 20, command);
            let (many, many_folds) = line_command_scans(language, 200, command);
            assert!(many_folds >= 150, "{language}: {many_folds} folds");
            assert!(
                many == few && many[0] <= 4,
                "{language} {name}: (construct readings, seeks) {few:?} with {few_folds} folds, {many:?} with {many_folds}"
            );
        }
    }
}
