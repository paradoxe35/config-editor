//! QA `E37-1`/`E37-5`: a key keeps the form its host writes it in through
//! every move of a session (moved again after a pause, its source a key
//! pasted earlier), whichever way it is cut (a whole line, a line's
//! text) or pasted (over a selected line), and so does a copy of it or
//! the second paste of one cut, in every host and view.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::moved_forms::{key_line, line_offset, pause, units};
use crate::editor::save_matrix::report;
use leptos::prelude::*;

/// A key `d` its host writes in a form of its own, in three blocks `a`
/// (holding it), `b` and `c`.
struct Kind {
    host: Language,
    entry: &'static str,
    /// The text the save keeps of it, once.
    kept: &'static str,
}

const KINDS: &[Kind] = &[
    Kind {
        host: Language::TOML,
        entry: "d = 1979-05-27T07:32:00Z",
        kept: "d = 1979-05-27T07:32:00Z\n",
    },
    Kind {
        host: Language::TOML,
        entry: "d = 0xff",
        kept: "d = 0xff\n",
    },
    Kind {
        host: Language::YAML,
        entry: "d: !Ref X",
        kept: "d: !Ref X\n",
    },
    Kind {
        host: Language::HCL,
        entry: "d = var.y",
        kept: "d = var.y\n",
    },
    Kind {
        host: Language::XML,
        entry: "<d xsi:type=\"F\" n=\"1\"/>",
        kept: "<d xsi:type=\"F\" n=\"1\"/>",
    },
];

/// The host's text, each block's entries as listed (`None` is `d`).
fn host_text(kind: &Kind, blocks: [&[Option<&str>]; 3]) -> String {
    let entry = |name: Option<&str>| match name {
        None => kind.entry.to_string(),
        Some(name) if kind.host == Language::YAML => format!("{name}: 1"),
        Some(name) if kind.host == Language::XML => format!("<{name}>1</{name}>"),
        Some(name) => format!("{name} = 1"),
    };
    let block = |name: &str, entries: &[Option<&str>]| -> String {
        let body = |indent: &str| -> String {
            entries
                .iter()
                .map(|at| format!("{indent}{}\n", entry(*at)))
                .collect()
        };
        if kind.host == Language::YAML {
            format!("{name}:\n{}", body("  "))
        } else if kind.host == Language::XML {
            format!("  <{name}>\n{}  </{name}>\n", body("    "))
        } else if kind.host == Language::HCL {
            format!("{name} {{\n{}}}\n", body("  "))
        } else {
            format!("[{name}]\n{}", body(""))
        }
    };
    let [a, b, c] = blocks;
    let blocks = [block("a", a), block("b", b), block("c", c)];
    if kind.host == Language::XML {
        format!(
            "<r xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n{}</r>\n",
            blocks.concat()
        )
    } else if kind.host == Language::TOML {
        blocks.join("\n")
    } else {
        blocks.concat()
    }
}

/// The edit replacing the view's bytes `from..to` with `typed`.
pub(super) fn replace(state: EditorState, (from, to): (usize, usize), typed: &str) {
    let before = state.doc().text.get_untracked();
    let text = format!("{}{typed}{}", &before[..from], &before[to..]);
    let start = units(&before[..from]);
    state.commit(Edit::new(
        before.clone(),
        (start, units(&before[..to])),
        text,
        start + units(typed),
    ));
}

/// Bytes `from..to` of the view's text copied (as Ctrl+C does): the
/// clipboard's record keeps their items' links.
pub(super) fn copy(state: EditorState, (from, to): (usize, usize)) {
    let text = state.doc().text.get_untracked();
    state.note_copied(&text[from..to], (units(&text[..from]), units(&text[..to])));
}

/// How the key's line is cut.
#[derive(Clone, Copy, Debug)]
enum Cut {
    /// The whole line.
    Line,
    /// Its text (End, Shift+Home, Ctrl+X), its indent and line break left:
    /// then the blank line it leaves is deleted.
    Text,
    /// Copied: the line stays.
    Copy,
}

/// The byte range of the lines of the entry `name` in a view's `text`:
/// its key's line, the lines indented deeper, and the line closing it at
/// its indent (`}`).
pub(super) fn entry_range(text: &str, name: &str) -> (usize, usize) {
    let first = key_line(text, name);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let indent = |line: &str| line.len() - line.trim_start().len();
    let own = indent(lines[first]);
    let mut last = first + 1;
    while last < lines.len() && indent(lines[last]) > own && !lines[last].trim().is_empty() {
        last += 1;
    }
    if last > first + 1
        && last < lines.len()
        && indent(lines[last]) == own
        && lines[last].trim_start().starts_with(['}', ']'])
    {
        last += 1;
    }
    (line_offset(text, first), line_offset(text, last))
}

/// The lines of the entry `name` cut (or copied) from the view as `how`
/// says: the text then pasted.
fn cut(state: EditorState, name: &str, how: Cut) -> String {
    let before = state.doc().text.get_untracked();
    let (from, to) = entry_range(&before, name);
    let lines = before[from..to].to_string();
    match how {
        Cut::Line => replace(state, (from, to), ""),
        Cut::Copy => copy(state, (from, to)),
        Cut::Text => {
            let indent = lines.len() - lines.trim_start().len();
            let end = from + lines.trim_end().len();
            replace(state, (from + indent, end), "");
            return lines.trim().to_string();
        }
    }
    lines
}

/// How the cut text is pasted on the line of `name`.
#[derive(Clone, Copy, Debug)]
enum Paste {
    /// Before it.
    Before,
    /// Over a blank line typed before it, selected whole.
    OverLine,
}

fn paste(state: EditorState, text: &str, name: &str, how: Paste) {
    let before = state.doc().text.get_untracked();
    let line = key_line(&before, name);
    let at = line_offset(&before, line);
    let indent: String = before[at..].chars().take_while(|c| *c == ' ').collect();
    // A line's text pasted at the line's indent.
    let text = if text.ends_with('\n') {
        text.to_string()
    } else {
        format!("{indent}{text}\n")
    };
    match how {
        Paste::Before => replace(state, (at, at), &text),
        Paste::OverLine => {
            replace(state, (at, at), &format!("{indent}x\n"));
            let end = at + indent.len() + 2;
            replace(state, (at, end), &text);
        }
    }
}
/// What is done to the key `d` of block `a`.
#[derive(Clone, Copy, Debug)]
enum Moves {
    /// Moved to `b`, a pause, then from `b` to `c`, cut and pasted as told
    /// (with a pause between the second cut and its paste, or not).
    Twice(Cut, Paste, bool),
    /// Copied to `b`, then (a pause between, or not) to `c`.
    Copied(bool),
    /// Cut, pasted in `b`, then (a pause between, or not) in `c` too.
    PastedTwice(bool),
}

/// The host's text once `moves` are made in `view` is the host's text
/// with the key moved or copied, written in its form; `Err` with what
/// went wrong.
fn moved(kind: &Kind, view: Language, moves: Moves) -> Result<(), String> {
    let host = host_text(kind, [&[None, Some("k")], &[Some("z")], &[Some("w")]]);
    let mut editor = TestEditor::new(kind.host, &host);
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let pause_if = |pausing: bool| {
        if pausing {
            pause(state);
        }
    };
    let expected = match moves {
        Moves::Twice(how, pasting, pausing) => {
            let lines = cut(state, "d", how);
            pause(state);
            paste(state, &lines, "z", pasting);
            pause(state);
            let lines = cut(state, "d", how);
            pause_if(pausing);
            paste(state, &lines, "w", pasting);
            host_text(kind, [&[Some("k")], &[Some("z")], &[None, Some("w")]])
        }
        Moves::Copied(pausing) => {
            let lines = cut(state, "d", Cut::Copy);
            paste(state, &lines, "z", Paste::Before);
            pause_if(pausing);
            paste(state, &lines, "w", Paste::Before);
            host_text(
                kind,
                [&[None, Some("k")], &[None, Some("z")], &[None, Some("w")]],
            )
        }
        Moves::PastedTwice(pausing) => {
            let lines = cut(state, "d", Cut::Line);
            pause(state);
            paste(state, &lines, "z", Paste::Before);
            pause_if(pausing);
            paste(state, &lines, "w", Paste::Before);
            host_text(kind, [&[Some("k")], &[None, Some("z")], &[None, Some("w")]])
        }
    };
    pause(state);
    let saved = state.sync().host_text.get_value();
    if !state.sync().in_sync.get_untracked() {
        return Err(format!(
            "the save waits ({:?}); view:\n{}",
            state.sync().conversion_problems.get_untracked(),
            state.doc().text.get_untracked()
        ));
    }
    let read = |text: &str| kind.host.behavior().parse(text).ok();
    if read(&saved) != read(&expected)
        || saved.matches(kind.kept).count() != expected.matches(kind.kept).count()
    {
        return Err(format!("saved:\n{saved}"));
    }
    Ok(())
}

const VIEWS: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

#[test]
fn a_key_moved_again_copied_or_pasted_twice_keeps_its_hosts_form_in_every_view() {
    let mut failures = Vec::new();
    for kind in KINDS {
        // An element with attributes is a map over lines in a view: a
        // TOML view writes it as a table, which moves with its header (the
        // round-36 matrix's), and has no one line of text.
        let element = kind.host == Language::XML;
        let views = VIEWS
            .into_iter()
            .filter(|view| *view != kind.host && !(element && *view == Language::TOML));
        for view in views {
            let mut all = Vec::new();
            for pausing in [false, true] {
                // A JSON view's line cut leaves commas to fix: its whole
                // lines are the round-36 matrix's.
                if view != Language::JSON {
                    for (how, pasting) in [
                        (Cut::Line, Paste::Before),
                        (Cut::Line, Paste::OverLine),
                        (Cut::Text, Paste::Before),
                    ] {
                        if !(element && matches!(how, Cut::Text)) {
                            all.push(Moves::Twice(how, pasting, pausing));
                        }
                    }
                }
                all.extend([Moves::Copied(pausing), Moves::PastedTwice(pausing)]);
            }
            for moves in all {
                if let Err(error) = moved(kind, view, moves) {
                    failures.push(format!(
                        "{:?} `{}` as {view:?}, {moves:?}: {error}",
                        kind.host, kind.entry
                    ));
                }
            }
        }
    }
    report(failures);
}
