//! An HCL object attribute (`tags = { … }`) moved or
//! copied in an Edit-as view into a nested block, or another resource,
//! stays an object attribute (Terraform rejects a `tags` block), in the
//! views that write it over lines of its own, pasted at its own indent or
//! re-indented after.

use super::super::languages::Language;
use super::super::test_state::TestEditor;
use super::moved_forms::{line_offset, pause};
use super::moved_forms_again::{copy, entry_range, replace};
use crate::editor::save_matrix::report;
use leptos::prelude::*;

const HOST: &str = "resource \"aws_instance\" \"a\" {\n  ami = \"ami-1\"\n  tags = {\n    Name = \"a\"\n  }\n}\n\nresource \"aws_instance\" \"b\" {\n  ami = \"ami-2\"\n  root_block_device {\n    size = 8\n  }\n}\n";

/// `lines` with `by` more indent on each line.
fn indented(lines: &str, by: &str) -> String {
    lines
        .split_inclusive('\n')
        .map(|line| format!("{by}{line}"))
        .collect()
}

#[derive(Clone, Copy, Debug)]
enum How {
    /// Cut, pasted at once.
    Moved,
    /// Cut, a pause, pasted.
    MovedAfterPause,
    /// Copied.
    Copied,
}

/// Where it goes.
#[derive(Clone, Copy, Debug)]
enum Target {
    /// Before `size`, in the nested block: pasted at its indent.
    Nested,
    /// The same, pasted at the indent it had, then re-indented (Tab).
    NestedReindented,
    /// Before resource `b`'s `ami`.
    OtherResource,
}

fn moved(view: Language, how: How, into: Target) -> Result<(), String> {
    let mut editor = TestEditor::new(Language::HCL, HOST);
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let text = state.doc().text.get_untracked();
    let (from, to) = entry_range(&text, "tags");
    let lines = text[from..to].to_string();
    if matches!(how, How::Copied) {
        copy(state, (from, to));
    } else {
        replace(state, (from, to), "");
        // The comma before it left over in a JSON view.
        let text = state.doc().text.get_untracked();
        if view == Language::JSON
            && let Some(end) = text[..from].rfind(',')
            && text[end + 1..from].trim().is_empty()
        {
            replace(state, (end, end + 1), "");
        }
    }
    if matches!(how, How::MovedAfterPause) {
        pause(state);
    }
    let text = state.doc().text.get_untracked();
    let (name, nth) = match into {
        Target::OtherResource => ("ami", 1),
        _ => ("size", 0),
    };
    let line = text
        .lines()
        .enumerate()
        .filter(|(_, line)| line.trim_start().trim_start_matches('"').starts_with(name))
        .nth(nth)
        .map(|(at, _)| at)
        .ok_or_else(|| format!("no {name} in\n{text}"))?;
    let at = line_offset(&text, line);
    let indent = |line: &str| line[..line.len() - line.trim_start().len()].to_string();
    let target = indent(text[at..].lines().next().unwrap_or_default());
    let own = indent(&lines);
    let deeper = target
        .strip_prefix(own.as_str())
        .unwrap_or_default()
        .to_string();
    match into {
        Target::NestedReindented => {
            replace(state, (at, at), &lines);
            replace(state, (at, at + lines.len()), &indented(&lines, &deeper));
        }
        _ => replace(state, (at, at), &indented(&lines, &deeper)),
    }
    // A JSON view's last entry has no comma: one is typed after it, as
    // pasted before another entry.
    let pasted = indented(&lines, &deeper);
    if view == Language::JSON && !lines.trim_end().ends_with(',') {
        let end = at + pasted.trim_end().len();
        replace(state, (end, end), ",");
    }
    pause(state);
    let saved = state.sync().host_text.get_value();
    if !state.sync().in_sync.get_untracked() {
        return Err(format!(
            "the save waits ({:?}); view:\n{}",
            state.sync().conversion_problems.get_untracked(),
            state.doc().text.get_untracked()
        ));
    }
    let written = saved.matches("tags = {").count();
    let wanted = if matches!(how, How::Copied) { 2 } else { 1 };
    if written != wanted || saved.contains("tags {") {
        return Err(format!("saved:\n{saved}"));
    }
    Ok(())
}

#[test]
fn an_object_attribute_moved_or_copied_into_a_block_stays_an_attribute() {
    let mut failures = Vec::new();
    for view in [Language::YAML, Language::JSON] {
        for how in [How::Moved, How::MovedAfterPause, How::Copied] {
            for into in [
                Target::Nested,
                Target::NestedReindented,
                Target::OtherResource,
            ] {
                if let Err(error) = moved(view, how, into) {
                    failures.push(format!("{view:?}, {how:?}, {into:?}: {error}"));
                }
            }
        }
    }
    report(failures);
}
