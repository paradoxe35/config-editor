//! A field's key written many times by the host (its secret's line copied
//! again and again, then given back): mounting it, typing beside the last
//! copy, a line copied, Ctrl+A Tab and its undo cost work linear in the
//! copies, in every language. The copies are found from the paths of all
//! their places asked at once, and asking one place's path never walks
//! over every copy above it (QA 36: in YAML, each place's hover walked up
//! over all the siblings above it; typing beside the last of 1,600 copies
//! took 580 ms, three times as long for each doubling).

use leptos::prelude::*;

use super::super::Language;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run};
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted_all};
use super::glued_tests::type_at;

const MASK: &str = "«secret 0b1c»";

/// Block `b` holding the mask under `key` `copies` times, then `x`.
fn block(language: Language, copies: usize) -> String {
    match language.label() {
        "YAML" => format!("b:\n{}  x: 1\n", format!("  key: {MASK}\n").repeat(copies)),
        "JSON" => format!(
            "{{\n  \"b\": {{\n{}    \"x\": 1\n  }}\n}}\n",
            format!("    \"key\": \"{MASK}\",\n").repeat(copies)
        ),
        "TOML" => format!(
            "[b]\n{}x = 1\n",
            format!("key = \"{MASK}\"\n").repeat(copies)
        ),
        "HCL" => format!(
            "b {{\n{}  x = 1\n}}\n",
            format!("  key = \"{MASK}\"\n").repeat(copies)
        ),
        _ => format!(
            "<b>\n{}  <x>1</x>\n</b>\n",
            format!("  <key>{MASK}</key>\n").repeat(copies)
        ),
    }
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// Each kind of work (by [`Work::ALL`]) that mounting the text with the
/// key written `copies` times, typing beside the last copy, the first
/// copy's line copied (Shift+Alt+Down), Ctrl+A Tab and its undo cost.
fn work(language: Language, copies: usize) -> Vec<usize> {
    let mut editor = TestEditor::new(language, &block(language, copies));
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{4}»"));
    let ((_owner, state), mounted) = counted_all(|| editor.mount());
    let ((), edited) = counted_all(|| {
        let text = shown(state);
        let last = text.rfind(MASK).expect("the mask") + MASK.len();
        type_at(state, last, "M");
        let text = shown(state);
        let first = text[..text.find(MASK).expect("the mask")]
            .rfind('\n')
            .map_or(0, |at| at + 1);
        let first = units(&text[..first]);
        run(
            state,
            Command::Lines(LineCommand::Copy { down: true }),
            (first, first),
            false,
        );
        let all = units(&shown(state));
        run(state, Command::Indent { outdent: false }, (0, all), false);
        if let Some(undo) = state.history_edit(false) {
            state.commit(undo);
        }
    });
    mounted
        .iter()
        .zip(edited)
        .map(|(mount, edits)| mount + edits)
        .collect()
}

/// Twice the copies cost about twice the work of each kind (a walk over
/// the copies for each copy is four times).
#[test]
fn a_key_written_many_times_costs_work_linear_in_its_copies() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for language in Language::ALL.iter().copied() {
        let counts: Vec<Vec<usize>> = [40, 80, 160]
            .into_iter()
            .map(|copies| work(language, copies))
            .collect();
        for pair in counts.windows(2) {
            for (index, kind) in Work::ALL.iter().enumerate() {
                let (small, large) = (pair[0][index], pair[1][index]);
                if large > 3 * small.max(50) {
                    costly.push(format!(
                        "{language}: {kind:?} {small} then {large} for twice the copies"
                    ));
                }
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}

/// The checks of an edit share one search for the copies of each text
/// they read, never one each (QA 36: up to six searches of one text for
/// each edit): typing beside the last copy, a line copied, Ctrl+A Tab and
/// its undo read a stand-in for each text at most.
#[test]
fn the_checks_of_an_edit_find_the_copies_once_for_each_text() {
    super::super::parse_cache::hold();
    let stand_in = Work::ALL
        .iter()
        .position(|each| *each == Work::StandIn)
        .unwrap();
    let mut costly = Vec::new();
    for language in Language::ALL.iter().copied() {
        let stand_ins = work(language, 40)[stand_in];
        eprintln!("{language}: {stand_ins}");
        if stand_ins > 4 {
            costly.push(format!("{language}: {stand_ins} stand-ins"));
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}
