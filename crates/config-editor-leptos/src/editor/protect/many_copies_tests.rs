//! A secret's line copied many times (Shift+Alt+Down again and again):
//! its key is written as many times and the text does not parse. Every
//! copy is judged in the value it is written in, however many there are:
//! text typed beside any copy is refused and the host never hears it
//! glued (QA 35: past the fourth copy, typing beside the last reached the
//! host), and the way back stays open on every copy: its line deleted,
//! its mask deleted, the copy undone (QA 35: at five copies and more, all
//! were refused on the last copy). Each copy past the host's is flagged
//! as a copy, and a token two fields share as the secret of the one it
//! left.

use leptos::prelude::*;

use super::super::Language;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run};
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted_all};
use super::glued_tests::{paste, type_at};
use super::problems::unknown_placeholders;
use super::texts;
use super::tokens::SecretMatcher;

const MASK: &str = "«secret 0b1c»";

const LANGUAGES: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
];

/// Block `b` holding the mask under `key`, then `x`.
fn block(language: Language, key: &str) -> String {
    match language.label() {
        "YAML" => format!("b:\n  {key}: {MASK}\n  x: 1\n"),
        "JSON" => format!("{{\n  \"b\": {{\n    \"{key}\": \"{MASK}\",\n    \"x\": 1\n  }}\n}}\n"),
        "TOML" => format!("[b]\n{key} = \"{MASK}\"\nx = 1\n"),
        _ => format!("b {{\n  {key} = \"{MASK}\"\n  x = 1\n}}\n"),
    }
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// The byte where line `index` of `text` starts.
fn line_start(text: &str, index: usize) -> usize {
    text.split_inclusive('\n').take(index).map(str::len).sum()
}

/// The index of the line holding the `nth` mask of the text on screen.
fn mask_line(state: EditorState, nth: usize) -> usize {
    shown(state)
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(MASK))
        .nth(nth)
        .map(|(index, _)| index)
        .expect("the mask")
}

/// `command` run with the caret at the start of line `index`.
fn on_line(state: EditorState, index: usize, command: LineCommand) {
    let text = shown(state);
    let units = text[..line_start(&text, index)].encode_utf16().count();
    run(state, Command::Lines(command), (units, units), false);
}

/// The block with its secret's line copied until it is written `copies`
/// times, and what the host heard.
fn copied(
    language: Language,
    key: &str,
    copies: usize,
) -> Result<(Owner, EditorState, RwSignal<Vec<String>>), String> {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(language, &block(language, key));
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{4}»"));
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    for _ in 1..copies {
        on_line(state, mask_line(state, 0), LineCommand::Copy { down: true });
    }
    let written = shown(state).matches(MASK).count();
    if written != copies {
        return Err(format!("copied {written} times:\n{}", shown(state)));
    }
    Ok((owner, state, heard))
}

/// The texts holding the mask with a letter beside it.
fn glued(texts: &[String]) -> Vec<String> {
    texts
        .iter()
        .filter(|text| text.contains(&format!("{MASK}M")) || text.contains(&format!("M{MASK}")))
        .cloned()
        .collect()
}

/// The keys and numbers of copies tried: a key of one letter has names
/// for fewer copies at once than the key is written.
const CASES: [(&str, usize); 6] = [
    ("key", 2),
    ("key", 5),
    ("key", 8),
    ("key", 20),
    ("k", 8),
    ("k", 30),
];

/// The copies tried: the first, one in the middle, the last.
fn copies_tried(copies: usize) -> Vec<usize> {
    let mut tried = vec![0, copies / 2, copies - 1];
    tried.dedup();
    tried
}

#[test]
fn text_beside_any_copy_of_a_secret_line_is_refused() {
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for (key, copies) in CASES {
            for nth in copies_tried(copies) {
                let case = format!("{language}, `{key}` {copies} times, copy {nth}");
                let (_owner, state, heard) = match copied(language, key, copies) {
                    Ok(mounted) => mounted,
                    Err(error) => {
                        failures.push(format!("{case}: {error}"));
                        continue;
                    }
                };
                let text = shown(state);
                let start = text.match_indices(MASK).nth(nth).expect("the mask").0;
                type_at(state, start + MASK.len(), "MY");
                let mut found = glued(&[shown(state)]);
                found.extend(glued(&heard.get_untracked()));
                if !found.is_empty() {
                    failures.push(format!("{case}: {found:?}"));
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} cases: {failures:#?}",
        failures.len()
    );
}

/// A way back from a copy (`nth`, by its mask).
type Back = (&'static str, fn(EditorState, usize));

const BACK: [Back; 4] = [
    ("Ctrl+Z", |state, _| {
        if let Some(undo) = state.history_edit(false) {
            state.commit(undo);
        }
    }),
    ("Ctrl+Shift+K", |state, nth| {
        on_line(state, mask_line(state, nth), LineCommand::Delete);
    }),
    ("its line selected and deleted", |state, nth| {
        let text = shown(state);
        let line = mask_line(state, nth);
        paste(
            state,
            line_start(&text, line)..line_start(&text, line + 1),
            "",
        );
    }),
    ("its mask deleted", |state, nth| {
        let text = shown(state);
        let start = text.match_indices(MASK).nth(nth).expect("the mask").0;
        paste(state, start..start + MASK.len(), "");
    }),
];

#[test]
fn the_way_back_from_any_copy_of_a_secret_line_stays_open() {
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for (key, copies) in CASES {
            for (back, act) in BACK {
                for nth in copies_tried(copies) {
                    let case = format!("{language}, `{key}` {copies} times, {back} (copy {nth})");
                    let (_owner, state, heard) = match copied(language, key, copies) {
                        Ok(mounted) => mounted,
                        Err(error) => {
                            failures.push(format!("{case}: {error}"));
                            continue;
                        }
                    };
                    let text = shown(state);
                    act(state, nth);
                    if shown(state) == text {
                        failures.push(format!("{case}: refused"));
                    }
                    let found = glued(&heard.get_untracked());
                    if !found.is_empty() {
                        failures.push(format!("{case}: heard {found:?}"));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} cases: {failures:#?}",
        failures.len()
    );
}

/// Each kind of work (by [`Work::ALL`]) an edit of the whole text (Ctrl+A
/// Tab) and its undo cost on the block with its secret's line written
/// `copies` times.
fn whole_edit_work(language: Language, copies: usize) -> Vec<usize> {
    let (_owner, state, _) = copied(language, "key", copies).expect("copied");
    let units = shown(state).encode_utf16().count();
    let ((), counts) = counted_all(|| {
        run(state, Command::Indent { outdent: false }, (0, units), false);
        if let Some(undo) = state.history_edit(false) {
            state.commit(undo);
        }
    });
    counts.to_vec()
}

/// Four times the copies is about four times the work of each kind: every
/// copy is found in a few readings of the text, not one for each.
#[test]
fn every_copy_of_a_secret_line_is_found_in_linear_work() {
    let mut costly = Vec::new();
    for language in LANGUAGES {
        let (small, large) = (whole_edit_work(language, 10), whole_edit_work(language, 40));
        for (index, kind) in Work::ALL.iter().enumerate() {
            if large[index] > 6 * small[index].max(20) {
                costly.push(format!(
                    "{language}: {kind:?} {} for 10 copies, {} for 40",
                    small[index], large[index]
                ));
            }
        }
    }
    assert!(costly.is_empty(), "{costly:#?}");
}

/// The warnings (line, message) on `text` for the tokens the host gave in
/// `host`.
fn flagged(language: Language, host: &str, text: &str) -> Vec<(Option<usize>, String)> {
    let behavior = language.behavior();
    let matcher =
        SecretMatcher::new(&SecretOptions::default().with_placeholder("«secret [0-9a-f]{4}»"));
    let known = matcher.known(behavior, host);
    let strings = crate::strings::default_strings();
    unknown_placeholders(behavior, text, &matcher, &known, &[], None, strings)
        .into_iter()
        .map(|problem| (problem.line, problem.message))
        .collect()
}

/// A copy of a secret's line in its own field is said to be one, never to
/// stand for another field's secret (QA 35: "/b/key: this placeholder
/// stands for the secret of /b/key, not this field's").
#[test]
fn a_copy_of_a_secret_line_is_flagged_as_a_copy() {
    let strings = crate::strings::default_strings();
    let mut failures = Vec::new();
    for language in LANGUAGES {
        let host = block(language, "key");
        let line = host
            .lines()
            .find(|line| line.contains(MASK))
            .expect("the mask");
        for copies in [2, 5, 8] {
            let text = host.replace(&format!("{line}\n"), &format!("{line}\n").repeat(copies));
            let found = flagged(language, &host, &text);
            let extra = texts::extra_placeholder(strings, "/b/key");
            if found.len() != copies - 1 || found.iter().any(|(_, message)| *message != extra) {
                failures.push(format!("{language}, {copies} copies: {found:#?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A token two fields share, written in a third, is said to stand for the
/// secret of the field it is missing from, or of both when neither misses
/// it (QA 35: the first of them was named, also when the other lost it).
#[test]
fn a_moved_token_two_fields_share_names_the_field_it_left() {
    let strings = crate::strings::default_strings();
    let host = format!("a:\n  key: {MASK}\nb:\n  key: {MASK}\nc: x\n");
    let cases = [
        (
            "copied into c",
            format!("a:\n  key: {MASK}\nb:\n  key: {MASK}\nc: {MASK}\n"),
            "/a/key, /b/key",
        ),
        (
            "moved from b into c",
            format!("a:\n  key: {MASK}\nb:\n  key: x\nc: {MASK}\n"),
            "/b/key",
        ),
        (
            "moved from a into c",
            format!("a:\n  key: x\nb:\n  key: {MASK}\nc: {MASK}\n"),
            "/a/key",
        ),
    ];
    let mut failures = Vec::new();
    for (case, text, owner) in cases {
        let found = flagged(Language::YAML, &host, &text);
        let expected = vec![(Some(5), texts::moved_placeholder(strings, "/c", owner))];
        if found != expected {
            failures.push(format!("{case}: {found:#?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
