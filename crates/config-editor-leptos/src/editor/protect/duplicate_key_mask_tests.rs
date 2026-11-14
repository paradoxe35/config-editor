//! One mask in two fields of the same key, a line moved into the other
//! field's block (Alt+Down, Alt+Up, cut and paste): the block writes the
//! key twice and the text does not parse. Each copy is judged in the value
//! it is written in: text typed or pasted beside either is refused from
//! the first character, and the host never hears it glued (QA 34: the
//! copy the language did not find took the first letter, then its line was
//! kept whole: no Backspace, undo, nor renaming or deleting the other).
//! The way back stays open: undoing the move, deleting or renaming either
//! copy, and, where text was written beside a copy before (the host's
//! text), taking it out, renaming or deleting the other.

use leptos::prelude::*;

use super::super::Language;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run};
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted_all};
use super::glued_tests::{paste, type_at};

const MASK: &str = "«secret 0b1c»";

const LANGUAGES: [Language; 4] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
];

/// Clients `a` and `b`, each holding the mask under `secret`, then a name.
fn clients(language: Language) -> String {
    match language.label() {
        "YAML" => format!(
            "clients:\n  a:\n    secret: {MASK}\n    name: alpha\n  b:\n    secret: {MASK}\n    name: beta\n"
        ),
        "JSON" => format!(
            "{{\n  \"clients\": {{\n    \"a\": {{\n      \"secret\": \"{MASK}\",\n      \"name\": \"alpha\"\n    }},\n    \"b\": {{\n      \"secret\": \"{MASK}\",\n      \"name\": \"beta\"\n    }}\n  }}\n}}\n"
        ),
        "TOML" => format!(
            "[clients.a]\nsecret = \"{MASK}\"\nname = \"alpha\"\n\n[clients.b]\nsecret = \"{MASK}\"\nname = \"beta\"\n"
        ),
        _ => format!(
            "clients {{\n  a {{\n    secret = \"{MASK}\"\n    name = \"alpha\"\n  }}\n  b {{\n    secret = \"{MASK}\"\n    name = \"beta\"\n  }}\n}}\n"
        ),
    }
}

/// The line opening client `b`'s block.
fn b_header(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "  b:",
        "JSON" => "    \"b\": {",
        "TOML" => "[clients.b]",
        _ => "  b {",
    }
}

fn mount(language: Language, text: &str) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(language, text);
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{4}»"));
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
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

/// Alt+Up or Alt+Down on line `index`.
fn move_line(state: EditorState, index: usize, down: bool) {
    let text = shown(state);
    let at = line_start(&text, index);
    let units = text[..at].encode_utf16().count();
    run(
        state,
        Command::Lines(LineCommand::Move { down }),
        (units, units),
        false,
    );
}

/// A way a secret line gets into the other client's block, leaving it
/// with the key twice.
type Route = (
    &'static str,
    fn(Language, EditorState) -> Result<(), String>,
);

const ROUTES: [Route; 3] = [
    ("Alt+Down", |language, state| {
        // Client a's line, down until it is the first in b's block.
        for _ in 0..8 {
            let line = mask_line(state, 0);
            let text = shown(state);
            if line > 0 && text.lines().nth(line - 1) == Some(b_header(language)) {
                return Ok(());
            }
            move_line(state, line, true);
        }
        Err(format!("not moved into b's block:\n{}", shown(state)))
    }),
    ("Alt+Up", |_, state| {
        // Client b's line, up until it is the last in a's block.
        for _ in 0..8 {
            let line = mask_line(state, 1);
            let text = shown(state);
            if text
                .lines()
                .nth(line - 1)
                .is_some_and(|line| line.contains("alpha"))
            {
                return Ok(());
            }
            move_line(state, line, false);
        }
        Err(format!("not moved into a's block:\n{}", shown(state)))
    }),
    ("cut and pasted", |language, state| {
        let text = shown(state);
        let line = mask_line(state, 0);
        let (start, end) = (line_start(&text, line), line_start(&text, line + 1));
        let cut = text[start..end].to_string();
        if !paste(state, start..end, "") {
            return Err("the cut refused".into());
        }
        let text = shown(state);
        let header = text.find(b_header(language)).expect("b's block");
        let at = header + text[header..].find('\n').expect("its line") + 1;
        if paste(state, at..at, &cut) {
            Ok(())
        } else {
            Err(format!("the paste refused:\n{text}"))
        }
    }),
];

/// The texts holding the mask with a letter beside it.
fn glued(texts: &[String]) -> Vec<String> {
    texts
        .iter()
        .filter(|text| text.contains(&format!("{MASK}M")) || text.contains(&format!("M{MASK}")))
        .cloned()
        .collect()
}

/// Line `index` of the text on screen deleted.
fn delete_line(state: EditorState, index: usize) -> bool {
    let text = shown(state);
    paste(
        state,
        line_start(&text, index)..line_start(&text, index + 1),
        "",
    )
}

/// The key on line `index` renamed (a letter typed after it).
fn rename_key(state: EditorState, index: usize) -> bool {
    let text = shown(state);
    let start = line_start(&text, index);
    let at = start + text[start..].find("secret").expect("the key") + "secret".len();
    paste(state, at..at, "2")
}

#[test]
fn text_beside_either_copy_of_a_key_holding_a_shared_mask_is_refused() {
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for (route, apply) in ROUTES {
            for nth in 0..2 {
                for typed in [true, false] {
                    let case = format!(
                        "{language}, {route}, {} beside copy {nth}",
                        if typed { "typed" } else { "pasted" }
                    );
                    let (_owner, state, heard) = mount(language, &clients(language));
                    if let Err(error) = apply(language, state) {
                        failures.push(format!("{case}: {error}"));
                        continue;
                    }
                    let text = shown(state);
                    let start = text.match_indices(MASK).nth(nth).expect("the mask").0;
                    if typed {
                        type_at(state, start + MASK.len(), "MY");
                    } else {
                        paste(state, start..start, "MY");
                    }
                    let mut found = glued(&[shown(state)]);
                    found.extend(glued(&heard.get_untracked()));
                    if shown(state) != text {
                        found.push(format!("changed to {:?}", shown(state)));
                    }
                    if !found.is_empty() {
                        failures.push(format!("{case}: {found:?}"));
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

/// A way back from a key written twice, on the copy `nth` (by its mask).
type Back = (&'static str, fn(EditorState, usize) -> bool);

const BACK: [Back; 3] = [
    ("Ctrl+Z", |state, _| {
        state
            .history_edit(false)
            .is_some_and(|undo| state.commit(undo))
    }),
    ("its line deleted", |state, nth| {
        delete_line(state, mask_line(state, nth))
    }),
    ("its key renamed", |state, nth| {
        rename_key(state, mask_line(state, nth))
    }),
];

#[test]
fn the_way_back_from_a_key_written_twice_stays_open() {
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for (route, apply) in ROUTES {
            for (back, act) in BACK {
                for nth in 0..2 {
                    let case = format!("{language}, {route}, {back} (copy {nth})");
                    let (_owner, state, heard) = mount(language, &clients(language));
                    if let Err(error) = apply(language, state) {
                        failures.push(format!("{case}: {error}"));
                        continue;
                    }
                    let text = shown(state);
                    if !act(state, nth) || shown(state) == text {
                        failures.push(format!("{case}: refused:\n{text}"));
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

/// The host's text with b's key written twice, `M` written beside the
/// mask in the second copy: the text beside it was there before the
/// guard held it.
fn glued_host(language: Language) -> String {
    let text = clients(language);
    let header = text.find(b_header(language)).expect("b's block");
    let at = header + text[header..].find('\n').expect("its line") + 1;
    let line_end = at + text[at..].find('\n').expect("b's secret") + 1;
    let line = text[at..line_end].to_string();
    let glued = line.replacen(MASK, &format!("{MASK}M"), 1);
    format!("{}{line}{glued}{}", &text[..at], &text[line_end..])
}

/// A way out of [`glued_host`]'s text, and whether the glued text is gone
/// after it.
type Out = (&'static str, fn(EditorState) -> bool, bool);

const OUT: [Out; 5] = [
    (
        "Backspace on the letter",
        |state| {
            let at = shown(state).find(&format!("{MASK}M")).expect("glued") + MASK.len();
            paste(state, at..at + 1, "")
        },
        true,
    ),
    (
        "the glued line deleted",
        |state| delete_line(state, mask_line(state, 2)),
        true,
    ),
    (
        "the other line deleted",
        |state| delete_line(state, mask_line(state, 1)),
        false,
    ),
    (
        "the glued key renamed",
        |state| rename_key(state, mask_line(state, 2)),
        false,
    ),
    (
        "the other key renamed",
        |state| rename_key(state, mask_line(state, 1)),
        false,
    ),
];

#[test]
fn text_written_beside_a_copy_of_a_key_written_twice_can_be_taken_out() {
    let mut failures = Vec::new();
    for language in LANGUAGES {
        for (out, act, gone) in OUT {
            let case = format!("{language}, {out}");
            let host = glued_host(language);
            let (_owner, state, heard) = mount(language, &host);
            if !act(state) || shown(state) == host {
                let tip = state.protect().tip.get_untracked().map(|tip| tip.message);
                failures.push(format!("{case}: refused ({tip:?}):\n{host}"));
                continue;
            }
            if gone {
                let found = glued(&[shown(state)]);
                if !found.is_empty() {
                    failures.push(format!("{case}: still glued {found:?}"));
                }
                if let Some(last) = heard.get_untracked().last()
                    && !glued(std::slice::from_ref(last)).is_empty()
                {
                    failures.push(format!("{case}: the host last heard {last:?}"));
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

/// `blocks` clients each holding the mask and a name, the middle one's
/// key written twice (the text does not parse), in `language`.
fn many(language: Language, blocks: usize) -> String {
    let block = |index: usize| {
        let (open, line, name, close) = match language.label() {
            "YAML" => (
                format!("  c{index}:\n"),
                format!("    secret: {MASK}\n"),
                format!("    name: n{index}\n"),
                String::new(),
            ),
            "JSON" => (
                format!("    \"c{index}\": {{\n"),
                format!("      \"secret\": \"{MASK}\",\n"),
                format!("      \"name\": \"n{index}\"\n"),
                if index + 1 < blocks {
                    "    },\n"
                } else {
                    "    }\n"
                }
                .to_string(),
            ),
            "TOML" => (
                format!("[clients.c{index}]\n"),
                format!("secret = \"{MASK}\"\n"),
                format!("name = \"n{index}\"\n"),
                "\n".to_string(),
            ),
            _ => (
                format!("  c{index} {{\n"),
                format!("    secret = \"{MASK}\"\n"),
                format!("    name = \"n{index}\"\n"),
                "  }\n".to_string(),
            ),
        };
        let lines = if index == blocks / 2 {
            line.repeat(2)
        } else {
            line
        };
        format!("{open}{lines}{name}{close}")
    };
    let body: String = (0..blocks).map(block).collect();
    match language.label() {
        "YAML" => format!("clients:\n{body}"),
        "JSON" => format!("{{\n  \"clients\": {{\n{body}  }}\n}}\n"),
        "TOML" => body,
        _ => format!("clients {{\n{body}}}\n"),
    }
}

/// Each kind of work (by [`Work::ALL`]) typing a letter at a name far
/// from the key written twice, and beside its second copy (refused),
/// among `blocks` clients.
fn twice_work(language: Language, blocks: usize) -> (Vec<usize>, Vec<usize>) {
    let (_owner, state, _) = mount(language, &many(language, blocks));
    let text = shown(state);
    let at = text
        .find("n1\n")
        .or_else(|| text.find("n1\""))
        .expect("a name")
        + 2;
    let ((), far) = counted_all(|| type_at(state, at, "7"));
    let second = mask_line(state, blocks / 2 + 1);
    let text = shown(state);
    let start = line_start(&text, second);
    let at = start + text[start..].find(MASK).expect("the copy") + MASK.len();
    let ((), beside) = counted_all(|| type_at(state, at, "Z"));
    assert!(
        !shown(state).contains(&format!("{MASK}Z")),
        "{language}: a letter beside the copy is refused"
    );
    (far.to_vec(), beside.to_vec())
}

/// Judging each copy of a key written twice reads the lines around the
/// edit, never the document per copy or per field: a key far from it or
/// beside it costs work at most linear in the clients (a text that does
/// not parse is read once for an edit).
#[test]
fn a_key_written_twice_costs_an_edit_no_pass_per_copy() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for language in LANGUAGES {
        let (small_far, small_beside) = twice_work(language, 100);
        let (large_far, large_beside) = twice_work(language, 400);
        for (kind, work) in Work::ALL.iter().enumerate() {
            if large_far[kind] > 6 * small_far[kind].max(100) {
                costly.push(format!(
                    "{language}: {work:?} {} for a key far from it among 100, {} among 400",
                    small_far[kind], large_far[kind]
                ));
            }
            if large_beside[kind] > 6 * small_beside[kind].max(100) {
                costly.push(format!(
                    "{language}: {work:?} {} for a key beside it among 100, {} among 400",
                    small_beside[kind], large_beside[kind]
                ));
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}
