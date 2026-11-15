//! A key moved in an Edit-as view and renamed to a new name each time,
//! over and over, keeps no memory: what the editor keeps of an item (its
//! link, its clipboard record, its host form) goes when the item goes, and
//! nothing is kept per name (P5, no leak).

use super::super::edit::{Edit, take_trace};
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::moved_forms::{pause, units};
use crate::live_allocations::live;
use leptos::prelude::*;

/// A host with the key `k0` written in a form of its own, first in block
/// `a` (before `mid`); block `b` holds `other`.
fn host(language: Language) -> &'static str {
    if language == Language::YAML {
        "a:\n  k0: !Ref OwnerParam\n  mid: 1\nb:\n  other: 2\n"
    } else if language == Language::HCL {
        "a {\n  k0 = upper(var.name)\n  mid = 1\n}\nb {\n  other = 2\n}\n"
    } else if language == Language::TOML {
        "[a]\nk0 = 0x1F\nmid = 1\n\n[b]\nother = 2\n"
    } else {
        "<r>\n  <a>\n    <k0 kind=\"x\" name=\"f\"/>\n    <mid>1</mid>\n  </a>\n  <b>\n    <other>2</other>\n  </b>\n</r>\n"
    }
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The byte range of the lines of the entry `name` in a JSON or YAML view.
fn entry(text: &str, name: &str) -> (usize, usize) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let first = lines
        .iter()
        .position(|line| {
            let line = line.trim_start().trim_start_matches('"');
            line.strip_prefix(name)
                .is_some_and(|rest| rest.starts_with(['"', ':']))
        })
        .unwrap_or_else(|| panic!("no {name} in\n{text}"));
    let own = indent(lines[first]);
    let mut last = first + 1;
    while last < lines.len() && indent(lines[last]) >= own {
        let closing = lines[last].trim_start().starts_with(['}', ']']);
        if indent(lines[last]) == own && !closing {
            break;
        }
        last += 1;
        if closing && indent(lines[last - 1]) == own {
            break;
        }
    }
    let from: usize = lines[..first].iter().map(|line| line.len()).sum();
    let to = from
        + lines[first..last]
            .iter()
            .map(|line| line.len())
            .sum::<usize>();
    (from, to)
}

fn replace(state: EditorState, (from, to): (usize, usize), typed: &str) {
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

/// The key `k{cycle}` cut, pasted in the other block before its first
/// key, a pause, renamed `k{cycle + 1}`, a pause, the rename undone and
/// done again, each with a pause.
fn cycle(state: EditorState, cycle: usize) {
    let name = format!("k{cycle}");
    let text = state.doc().text.get_untracked();
    let (from, to) = entry(&text, &name);
    let lines = text[from..to].to_string();
    replace(state, (from, to), "");
    let text = state.doc().text.get_untracked();
    let beside = if cycle.is_multiple_of(2) {
        "other"
    } else {
        "mid"
    };
    let (at, _) = entry(&text, beside);
    replace(state, (at, at), &lines);
    pause(state);
    let text = state.doc().text.get_untracked();
    let (at, _) = entry(&text, &name);
    let at = at + text[at..].find(&name).expect("its name");
    replace(state, (at, at + name.len()), &format!("k{}", cycle + 1));
    pause(state);
    for redo in [false, true] {
        let edit = state.history_edit(redo).expect("a step to take");
        state.commit(edit);
        pause(state);
    }
    // The stages the edits passed, which only tests record.
    take_trace();
}

/// Cycles after which the undo history holds its most steps (each cycle
/// adds a few), so what it keeps stays the same from then on.
const WARM_UP: usize = 200;

/// Cycles measured after the warm-up.
const CYCLES: usize = 300;

/// The live allocations (count, bytes) each of two runs of `CYCLES` more
/// cycles adds to an editor of `host` shown as `view` after the warm-up. A
/// leak adds in both runs; a process-wide cache another test fills or frees
/// meanwhile shows in one run at most.
fn kept(host_language: Language, view: Language) -> [(isize, isize); 2] {
    let mut editor = TestEditor::new(host_language, host(host_language));
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    (0..WARM_UP).for_each(|at| cycle(state, at));
    let start = live();
    (WARM_UP..WARM_UP + CYCLES).for_each(|at| cycle(state, at));
    let middle = live();
    (WARM_UP + CYCLES..WARM_UP + 2 * CYCLES).for_each(|at| cycle(state, at));
    let end = live();
    let saved = state.sync().host_text.get_value();
    let last = format!("k{}", WARM_UP + 2 * CYCLES);
    assert!(saved.contains(&last), "{host_language} as {view}: {saved}");
    [
        (middle.0 - start.0, middle.1 - start.1),
        (end.0 - middle.0, end.1 - middle.1),
    ]
}

#[test]
fn a_key_moved_and_renamed_over_and_over_keeps_no_memory_in_every_host_and_view() {
    let mut failures = Vec::new();
    for host_language in [Language::YAML, Language::HCL, Language::TOML, Language::XML] {
        for view in [Language::JSON, Language::YAML] {
            if view == host_language {
                continue;
            }
            // On a thread of its own: what other tests left in this
            // thread's caches neither counts nor fills them meanwhile.
            let runs = std::thread::Builder::new()
                .stack_size(16 << 20)
                .spawn(move || kept(host_language, view))
                .expect("a thread")
                .join()
                .expect("the cycles ran");
            eprintln!("{host_language} as {view}: kept over {CYCLES} cycles, twice: {runs:?}");
            // Anything kept per cycle is CYCLES allocations or more in each
            // run. A few allocations or bytes in one run are a shared cache
            // (alone this measures 0 allocations, 133 bytes, once).
            if runs
                .iter()
                .all(|(count, bytes)| *count > 3 || *bytes > 1024)
            {
                failures.push(format!(
                    "{host_language} as {view}: kept over {CYCLES} cycles, twice: {runs:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
