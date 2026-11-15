//! QA `E38-3`/`E38-4`: a map or list its host writes on one line (a TOML
//! inline table, an HCL one-line object attribute, a YAML flow map or
//! list) moved or copied to another map in an Edit-as view, or moved in
//! a paste of the whole view, is written on one line where it lands, as
//! it was: the save patches the lines it adds (the host's comments
//! stay), never writes the whole document anew or in another form.

use super::super::languages::Language;
use super::super::test_state::TestEditor;
use super::moved_forms::{key_line, line_offset, pause};
use super::moved_forms_again::{copy, entry_range, replace};
use crate::editor::save_matrix::report;
use crate::editor::work_count::{Work, counted_all};
use leptos::prelude::*;

/// A key `d` its host writes on one line, in block `a`; block `b` holds
/// `z`.
struct Kind {
    host: Language,
    /// The host's text, `{d}` where the key's line goes in `a` and `{b}`
    /// where it goes before `z` in `b`.
    text: &'static str,
    /// The key's line, as written (with the indent it has in `b`).
    line: &'static str,
}

const KINDS: &[Kind] = &[
    Kind {
        host: Language::TOML,
        text: "# deps\n[a]\n{d}k = 1\n\n[b]\n{b}z = 1\n",
        line: "d = { x = 1, y = [\"p\", \"q\"] }\n",
    },
    Kind {
        host: Language::TOML,
        text: "# deps\n[a]\n{d}k = 1\n\n[b]\n{b}z = 1\n",
        line: "d = { version = \"1\", features = [\"full\"] }\n",
    },
    Kind {
        host: Language::TOML,
        text: "# deps\n[a]\n{d}k = 1\n\n[[b]]\nn = 1\n{b}z = 1\n",
        line: "d = { x = 1, y = [\"p\", \"q\"] }\n",
    },
    Kind {
        host: Language::HCL,
        text: "# main\nresource \"r\" \"a\" {\n{d}  k = 1\n}\n\n// other\nresource \"r\" \"b\" {\n{b}  z = 1\n}\n",
        line: "  d = { Name = \"web\", Env = \"prod\" }\n",
    },
    Kind {
        host: Language::YAML,
        text: "# top\na:\n{d}  k: 1\nb:\n{b}  z: 1\n",
        line: "  d: {x: 1, y: [p, q]}\n",
    },
    Kind {
        host: Language::YAML,
        text: "# top\na:\n{d}  k: 1\nb:\n{b}  z: 1\n",
        line: "  d: [p, q]\n",
    },
];

#[derive(Clone, Copy, Debug)]
enum How {
    /// Cut, pasted at once.
    Moved,
    /// Cut, a pause, pasted.
    MovedAfterPause,
    /// Copied.
    Copied,
    /// Copied, a pause, pasted.
    CopiedAfterPause,
    /// The whole view replaced with its text from outside the editor, the
    /// key moved.
    WholePaste,
}

/// The host's text with the key in `a`, in `b`, or both.
fn host(kind: &Kind, in_a: bool, in_b: bool) -> String {
    let line = |there: bool| if there { kind.line } else { "" };
    kind.text
        .replace("{d}", line(in_a))
        .replace("{b}", line(in_b))
}

fn moved(kind: &Kind, view: Language, how: How) -> Result<(), String> {
    let mut editor = TestEditor::new(kind.host, &host(kind, true, false));
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let text = state.doc().text.get_untracked();
    let (from, to) = entry_range(&text, "d");
    let cut_lines = text[from..to].to_string();
    // A JSON view's entry pasted before another one ends in a comma: in
    // a paste of the whole view, as written; else typed after the paste.
    let comma = view == Language::JSON && !cut_lines.trim_end().ends_with(',');
    let lines = if comma && matches!(how, How::WholePaste) {
        format!("{},\n", cut_lines.trim_end())
    } else {
        cut_lines.clone()
    };
    let moving = !matches!(how, How::Copied | How::CopiedAfterPause);
    let cut = |text: &str| -> String {
        let mut text = format!("{}{}", &text[..from], &text[to..]);
        if view == Language::JSON
            && let Some(end) = text[..from].rfind(',')
            && text[end + 1..from].trim().is_empty()
        {
            text.replace_range(end..end + 1, "");
        }
        text
    };
    // Pasted at the indent of the line it goes before.
    let indent = |line: &str| line[..line.len() - line.trim_start().len()].to_string();
    let own = indent(&lines);
    let deeper = |text: &str, at: usize| -> String {
        let target = indent(text[at..].lines().next().unwrap_or_default());
        let by = target.strip_prefix(own.as_str()).unwrap_or_default();
        lines
            .split_inclusive('\n')
            .map(|line| format!("{by}{line}"))
            .collect()
    };
    let pasted = |text: &str| -> String {
        let at = line_offset(text, key_line(text, "z"));
        format!("{}{}{}", &text[..at], deeper(text, at), &text[at..])
    };
    match how {
        How::WholePaste => {
            let all = pasted(&cut(&text));
            replace(state, (0, text.len()), &all);
        }
        _ => {
            if moving {
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
            if !moving {
                copy(state, (from, to));
            }
            if matches!(how, How::MovedAfterPause | How::CopiedAfterPause) {
                pause(state);
            }
            let text = state.doc().text.get_untracked();
            let at = line_offset(&text, key_line(&text, "z"));
            let pasted = deeper(&text, at);
            replace(state, (at, at), &pasted);
            if comma {
                let end = at + pasted.trim_end().len();
                replace(state, (end, end), ",");
            }
        }
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
    let expected = host(kind, !moving, true);
    // A whole text pasted from outside the editor carries no links: the
    // table it moves is a new value, written in the host's own form if
    // that reads back the same; every other value keeps its text.
    if matches!(how, How::WholePaste) {
        let read = |text: &str| kind.host.behavior().parse(text).ok();
        let unmoved = host(kind, false, false);
        let kept = unmoved
            .lines()
            .filter(|line| !line.trim().is_empty())
            .all(|line| saved.contains(line));
        if read(&saved) != read(&expected) || !kept {
            return Err(format!(
                "saved:\n{saved}\nexpected the data of:\n{expected}"
            ));
        }
        return Ok(());
    }
    if saved != expected {
        return Err(format!("saved:\n{saved}\nexpected:\n{expected}"));
    }
    Ok(())
}

#[test]
fn a_one_line_map_or_list_moved_or_copied_stays_on_one_line() {
    let mut failures = Vec::new();
    for kind in KINDS {
        for view in [Language::YAML, Language::JSON, Language::HCL] {
            // An HCL view writes a map in a list item as a block, which a
            // paste cannot put in the item's object.
            if view == kind.host || (view == Language::HCL && kind.text.contains("[[")) {
                continue;
            }
            for how in [
                How::Moved,
                How::MovedAfterPause,
                How::Copied,
                How::CopiedAfterPause,
                How::WholePaste,
            ] {
                if let Err(error) = moved(kind, view, how) {
                    failures.push(format!(
                        "{:?} `{}` in a {view:?} view, {how:?}: {error}",
                        kind.host,
                        kind.line.trim()
                    ));
                }
            }
        }
    }
    report(failures);
}

/// The work of moving one inline table among `size` tables that each
/// hold one, cut and pasted at once in a YAML view (by [`Work::ALL`]).
fn move_work(size: usize) -> Vec<usize> {
    let host: String = (0..size)
        .map(|index| format!("[t{index}]\nd{index} = {{ x = {index}, y = [\"p\"] }}\nk = 1\n\n"))
        .collect();
    let mut editor = TestEditor::new(Language::TOML, &host);
    editor.edit_as = Some(Language::YAML);
    let (_owner, state) = editor.mount();
    let text = state.doc().text.get_untracked();
    let (from, to) = entry_range(&text, "d1");
    let lines = text[from..to].to_string();
    let counts = counted_all(|| {
        replace(state, (from, to), "");
        let text = state.doc().text.get_untracked();
        let at = line_offset(&text, key_line(&text, "k"));
        replace(state, (at, at), &lines);
        pause(state);
    })
    .1;
    let saved = state.sync().host_text.get_value();
    assert!(
        saved.contains("[t0]\nd0 = { x = 0, y = [\"p\"] }\nd1 = { x = 1, y = [\"p\"] }\nk = 1\n"),
        "{saved}"
    );
    counts.to_vec()
}

/// Twice the tables: each kind of work at most about twice (the inline
/// forms of moved items are read once per save, not once per item).
#[test]
fn an_inline_table_moved_costs_work_growing_with_the_document_only() {
    let (small, large) = (move_work(12), move_work(24));
    let grown: Vec<String> = Work::ALL
        .iter()
        .zip(small.iter().zip(&large))
        .filter(|(_, (small, large))| 2 * **large > 5 * (**small).max(16))
        .map(|(work, (small, large))| format!("{work:?}: {small} then {large}"))
        .collect();
    assert!(grown.is_empty(), "{grown:#?}");
}
