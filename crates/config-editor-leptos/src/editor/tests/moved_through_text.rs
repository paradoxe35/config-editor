//! P3: an HCL expression a view shows as `${…}` text (a template, a
//! reference, a function call) keeps its form when its line is cut or
//! copied and pasted in a YAML, JSON, TOML or XML view: pasted at once as
//! an item, or first into a multi-line string, saved there, and cut from
//! it and pasted as an item again (the text stays the item's across the
//! save that wrote it into the string). Other hosts' forms take the same
//! path.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::moved_forms::{key_line, line_offset, pause, units};
use super::moved_forms_again::copy;
use leptos::prelude::*;

/// The values `sub` holds, as the host writes them.
const VALUES: [&str; 3] = [
    "\"web-${var.env}\"",
    "aws_subnet.main.id",
    "upper(var.name)",
];

/// How the line of `sub` leaves its block.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Take {
    Cut,
    Copy,
}

/// Where the line goes before it is pasted beside `other`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Path {
    /// Pasted there after a pause.
    Direct,
    /// Pasted into the multi-line string `script`, saved there, and cut
    /// from it.
    ThroughText,
}

/// A host text holding `sub` in a form no view shows, and how its save
/// writes it moved before `other` in `b` (and left in `a`, copied).
struct Host {
    language: Language,
    text: String,
    in_b: String,
    in_a: String,
}

fn hcl(value: &str) -> Host {
    Host {
        language: Language::HCL,
        text: format!(
            "a {{\n  sub = {value}\n  mid = 1\n}}\nb {{\n  other = 2\n  script = <<EOT\nline\nEOT\n}}\n"
        ),
        in_b: format!("b {{\n  sub = {value}\n  other = 2\n"),
        in_a: format!("a {{\n  sub = {value}\n"),
    }
}

/// The byte range of line `line` of `text`, its line break included.
fn line_range(text: &str, line: usize) -> (usize, usize) {
    (line_offset(text, line), line_offset(text, line + 1))
}

/// `text[from..to]` replaced by `with`, as a command.
fn replace(state: EditorState, (from, to): (usize, usize), with: &str) {
    let before = state.doc().text.get_untracked();
    let text = format!("{}{with}{}", &before[..from], &before[to..]);
    let start = units(&before[..from]);
    state.commit(Edit::new_units(
        before.clone(),
        (start, units(&before[..to])),
        text,
        start + units(with),
    ));
}

/// The line `line` of the view cut or copied: its text.
fn take(state: EditorState, line: usize, how: Take) -> String {
    let before = state.doc().text.get_untracked();
    let range = line_range(&before, line);
    let lines = before[range.0..range.1].to_string();
    copy(state, range);
    if how == Take::Cut {
        replace(state, range, "");
    }
    lines
}

/// `lines` pasted on line `line`, at the indent of line `like`.
fn paste_at(state: EditorState, (line, like): (usize, usize), lines: &str) {
    let before = state.doc().text.get_untracked();
    let at = line_offset(&before, line);
    let model = &before[line_offset(&before, like)..];
    let indent = &model[..model.len() - model.trim_start_matches(' ').len()];
    replace(state, (at, at), &format!("{indent}{}", lines.trim_start()));
}

/// The line of the multi-line string's text `line`, from 0.
fn text_line(text: &str) -> usize {
    text.lines()
        .position(|line| line.trim() == "line")
        .unwrap_or_else(|| panic!("no text line in\n{text}"))
}

fn session(host: &Host, view: Language, how: Take, path: Path) -> Result<(), String> {
    let mut editor = TestEditor::new(host.language, &host.text);
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let text = state.doc().text.get_untracked();
    let mut lines = take(state, key_line(&text, "sub"), how);
    if path == Path::ThroughText {
        let text = state.doc().text.get_untracked();
        // After the string's text line, at its indent: in the string.
        let line = text_line(&text);
        paste_at(state, (line + 1, line), &lines);
        pause(state);
        if !state.sync().in_sync.get_untracked() {
            return Err(format!(
                "pasted into the string, not saved: {:?}",
                state.sync().conversion_problems.get_untracked()
            ));
        }
        let text = state.doc().text.get_untracked();
        lines = take(state, text_line(&text) + 1, Take::Cut);
    }
    pause(state);
    let text = state.doc().text.get_untracked();
    let line = key_line(&text, "other");
    paste_at(state, (line, line), &lines);
    pause(state);
    let saved = state.sync().host_text.get_value();
    let copied = how == Take::Cut || saved.starts_with(&host.in_a);
    if !state.sync().in_sync.get_untracked() || !saved.contains(&host.in_b) || !copied {
        return Err(format!(
            "saved:\n{saved}\nview:\n{}\nproblems: {:?}",
            state.doc().text.get_untracked(),
            state.sync().conversion_problems.get_untracked()
        ));
    }
    Ok(())
}

#[test]
fn hcl_expressions_cut_or_copied_in_every_view_keep_their_form() {
    let mut failures = Vec::new();
    for view in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::XML,
    ] {
        for value in VALUES {
            for how in [Take::Cut, Take::Copy] {
                if let Err(error) = session(&hcl(value), view, how, Path::Direct) {
                    failures.push(format!("{} {value} {how:?}: {error}", view.label()));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// A YAML block scalar and a TOML multi-line string hold the line as
/// text; saved so, it is still the item's when cut from there.
#[test]
fn hcl_expressions_moved_through_a_multi_line_string_keep_their_form() {
    let mut failures = Vec::new();
    for view in [Language::YAML, Language::TOML] {
        for value in VALUES {
            for how in [Take::Cut, Take::Copy] {
                if let Err(error) = session(&hcl(value), view, how, Path::ThroughText) {
                    failures.push(format!("{} {value} {how:?}: {error}", view.label()));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The line of `sub` joined onto the line of `mid` after it (a JSON
/// `"sub": …, "mid": 1`, XML's two elements on one line), cut or copied,
/// and pasted before `other`, at once or after a save (`saved`): each
/// entry is still its host item, told apart by where its text starts, and
/// keeps its form.
fn joined_session(host: &Host, view: Language, how: Take, saved: bool) -> Result<(), String> {
    let mut editor = TestEditor::new(host.language, &host.text);
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let text = state.doc().text.get_untracked();
    let line = key_line(&text, "sub");
    // The line break alone, as a backspace removes it.
    let end = line_offset(&text, line + 1) - 1;
    replace(state, (end, end + 1), "");
    if saved {
        pause(state);
    }
    let lines = take(state, line, how);
    pause(state);
    let text = state.doc().text.get_untracked();
    let line = key_line(&text, "other");
    paste_at(state, (line, line), &lines);
    pause(state);
    let saved = state.sync().host_text.get_value();
    let copied = how == Take::Cut || saved.starts_with(&host.in_a);
    if !state.sync().in_sync.get_untracked() || !saved.contains(&host.in_b) || !copied {
        return Err(format!(
            "saved:\n{saved}\nview:\n{}\nproblems: {:?}",
            state.doc().text.get_untracked(),
            state.sync().conversion_problems.get_untracked()
        ));
    }
    Ok(())
}

#[test]
fn entries_joined_on_one_line_cut_or_copied_keep_their_forms() {
    let toml = Host {
        language: Language::TOML,
        text: "[a]\nsub = 1979-05-27\nmid = 1\nend = 3\n\n[b]\nother = 2\n".to_string(),
        in_b: "[b]\nsub = 1979-05-27\nmid = 1\nother = 2\n".to_string(),
        in_a: "[a]\nsub = 1979-05-27\n".to_string(),
    };
    let yaml = Host {
        language: Language::YAML,
        text: "a:\n  sub: !Ref Param\n  mid: 1\n  end: 3\nb:\n  other: 2\n".to_string(),
        in_b: "b:\n  sub: !Ref Param\n  mid: 1\n  other: 2\n".to_string(),
        in_a: "a:\n  sub: !Ref Param\n".to_string(),
    };
    let hcl = |value: &str| Host {
        language: Language::HCL,
        text: format!("a {{\n  sub = {value}\n  mid = 1\n  end = 3\n}}\nb {{\n  other = 2\n}}\n"),
        in_b: format!("b {{\n  sub = {value}\n  mid = 1\n  other = 2\n"),
        in_a: format!("a {{\n  sub = {value}\n"),
    };
    let mut hosts = vec![toml, yaml];
    hosts.extend(VALUES.iter().map(|value| hcl(value)));
    let mut failures = Vec::new();
    for host in &hosts {
        for view in [Language::JSON, Language::XML] {
            for (how, saved) in [Take::Cut, Take::Copy]
                .into_iter()
                .flat_map(|how| [(how, false), (how, true)])
            {
                if let Err(error) = joined_session(host, view, how, saved) {
                    failures.push(format!(
                        "{} in {} {how:?} (saved between: {saved}): {error}",
                        host.language.label(),
                        view.label()
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

/// The same path keeps the forms of other hosts: a TOML date through a
/// YAML block scalar, a YAML tag through a TOML multi-line string.
#[test]
fn toml_dates_and_yaml_tags_moved_through_a_multi_line_string_keep_their_form() {
    let toml = Host {
        language: Language::TOML,
        text: "[a]\nsub = 1979-05-27\nmid = 1\n\n[b]\nother = 2\nscript = \"\"\"\nline\n\"\"\"\n"
            .to_string(),
        in_b: "[b]\nsub = 1979-05-27\nother = 2\n".to_string(),
        in_a: "[a]\nsub = 1979-05-27\n".to_string(),
    };
    let yaml = Host {
        language: Language::YAML,
        text: "a:\n  sub: !Ref Param\n  mid: 1\nb:\n  other: 2\n  script: |\n    line\n"
            .to_string(),
        in_b: "b:\n  sub: !Ref Param\n  other: 2\n".to_string(),
        in_a: "a:\n  sub: !Ref Param\n".to_string(),
    };
    let mut failures = Vec::new();
    for (host, view) in [(&toml, Language::YAML), (&yaml, Language::TOML)] {
        for how in [Take::Cut, Take::Copy] {
            if let Err(error) = session(host, view, how, Path::ThroughText) {
                failures.push(format!(
                    "{} in {} {how:?}: {error}",
                    host.language.label(),
                    view.label()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
