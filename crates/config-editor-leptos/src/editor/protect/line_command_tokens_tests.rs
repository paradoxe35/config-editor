//! The line commands next to secret placeholders write exactly the text
//! they mean. Two lines swapped share characters at their ends (`»`, a
//! token's last digits), so the change's diff starts or ends inside a
//! token the command wrote again whole; the edit must not be read as one
//! cutting it (which took the shared characters away: `«secret 3f2a`).
//! And the placeholders stay real where they moved.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::keys::line_edit::{Folded, LineCommand, line_edit};
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{HOSTS, sample};
use super::secret_tests::documents;
use super::tests::Random;

const COMMANDS: [LineCommand; 5] = [
    LineCommand::Move { down: false },
    LineCommand::Move { down: true },
    LineCommand::Copy { down: false },
    LineCommand::Copy { down: true },
    LineCommand::Delete,
];

fn real(state: EditorState) -> usize {
    state.protect().secrets.tokens.with_untracked(Vec::len)
}

/// Runs `command` on `selection` as the key does: `None` when it wrote the
/// text it meant (or, unless `strict`, was refused, the text kept), else
/// what it wrote.
fn run(
    state: EditorState,
    selection: (usize, usize),
    command: LineCommand,
    strict: bool,
) -> Option<String> {
    let before = state.doc().text.get_untracked();
    let none = Folded {
        folds: &[],
        units: &[],
    };
    let edit = line_edit(&before, selection, command, none)?;
    let (anchor, caret) = edit.selection;
    state.commit(
        Edit::new(before.clone(), selection, edit.text.clone(), caret)
            .selecting_from(anchor)
            .folding(edit.folds),
    );
    let written = state.doc().text.get_untracked();
    let refused = !strict && written == before;
    (written != edit.text && !refused)
        .then(|| format!("meant {:?}\n  wrote {written:?}", edit.text))
}

/// Carets on each line of `text` (UTF-16): at its end, two before it and
/// in its middle (inside a token, as a click on it can leave it).
fn carets(text: &str) -> Vec<(usize, usize)> {
    let mut carets = Vec::new();
    let mut units = 0;
    for (line, content) in text.split('\n').enumerate() {
        let length = content.encode_utf16().count();
        for column in [length, length.saturating_sub(2), length / 2] {
            carets.push((line, units + column));
        }
        units += length + 1;
    }
    carets
}

/// Every line command on every line of `text` (see [`carets`]) writes the text it means, or is refused. With `fields_kept` (each
/// line a field of its own, so moving lines keeps every value in its
/// field), none is refused and a move keeps every placeholder real.
fn every_command_on_every_line(
    label: &str,
    (language, text): (Language, &str),
    options: Option<SecretOptions>,
    fields_kept: bool,
    failures: &mut Vec<String>,
) {
    for (line, caret) in carets(text) {
        for command in COMMANDS {
            let mut editor = TestEditor::new(language, text);
            editor.secrets = options.clone();
            let (_owner, state) = editor.mount();
            let tokens = real(state);
            if let Some(failure) = run(state, (caret, caret), command, fields_kept) {
                failures.push(format!(
                    "{label} {language}: {command:?} line {line} at {caret}: {failure}"
                ));
                continue;
            }
            if fields_kept && matches!(command, LineCommand::Move { .. }) && real(state) != tokens {
                failures.push(format!(
                    "{label} {language}: {command:?} line {line}: {} of {tokens} placeholders real:\n{}",
                    real(state),
                    state.doc().text.get_untracked()
                ));
            }
        }
    }
}

#[test]
fn line_commands_next_to_placeholders_write_the_text_they_mean_in_every_language() {
    let mut failures = Vec::new();
    for (language, text) in documents() {
        // JSON's last field has no comma: moved, the text no longer parses.
        let fields_kept = language != Language::JSON;
        every_command_on_every_line(
            "default",
            (language, &text),
            None,
            fields_kept,
            &mut failures,
        );
    }
    for tokens in HOSTS {
        for &language in Language::ALL {
            let text = sample(language, &tokens);
            let options = Some(tokens.options());
            every_command_on_every_line(
                tokens.pattern,
                (language, &text),
                options,
                false,
                &mut failures,
            );
        }
    }
    // Tokens sharing their last digits; a plain value ending as a token
    // does.
    let pattern = Some(SecretOptions::default().with_placeholder("REDACTED-[0-9]+"));
    let shared = "a: REDACTED-11\nb: REDACTED-21\nc: REDACTED-31\n";
    every_command_on_every_line(
        "digits",
        (Language::YAML, shared),
        pattern,
        true,
        &mut failures,
    );
    let plain = "x: a»\ny: «secret 3f2a»\nz: b»\n";
    every_command_on_every_line("plain", (Language::YAML, plain), None, true, &mut failures);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Random lines of placeholders, look-alikes and plain values, random
/// selections, every line command, in a row: each writes the text it
/// means.
#[test]
fn random_line_commands_write_the_text_they_mean() {
    const VALUES: &[&str] = &[
        "«secret 3f2a»",
        "«secret 0b1c»",
        "«secret 3f2a»",
        "app",
        "app»",
        "«secret 3f2",
        "a»",
        "x «secret 0b1c» y",
        "",
    ];
    let mut random = Random(0x2914_a1c3);
    let mut failures = Vec::new();
    for document in 0..40 {
        let lines = 2 + random.below(6);
        let text: String = (0..lines)
            .map(|line| {
                let value = VALUES[random.below(VALUES.len())];
                format!(
                    "{}k{line}: {value}\n",
                    "  ".repeat(usize::from(line > 0 && random.below(3) == 0))
                )
            })
            .collect();
        let (_owner, state) = TestEditor::new(Language::YAML, &text).mount();
        for step in 0..12 {
            let shown = state.doc().text.get_untracked();
            let length = shown.encode_utf16().count();
            let start = random.below(length + 1);
            let end = (start + random.below(30) * random.below(2)).min(length);
            let command = COMMANDS[random.below(COMMANDS.len())];
            if let Some(failure) = run(state, (start, end), command, false) {
                failures.push(format!(
                    "document {document} step {step}: {command:?} {start}..{end} on {shown:?}: {failure}"
                ));
                break;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Which diffs cutting into a token are a token written whole: lines
/// swapped, or deleted next to a like one, are; a character typed or
/// deleted inside a token, or a token pasted over part of one, are not
/// (the token then goes whole, see `tokens::widen`).
#[test]
fn a_token_written_whole_is_told_from_one_cut() {
    use super::rewritten::rewritten_region;
    let units = |text: &str| text.encode_utf16().count();
    // The diff of `before` and `after` (UTF-16 in `before`).
    let diff = |before: &str, after: &str| {
        let old: Vec<u16> = before.encode_utf16().collect();
        let new: Vec<u16> = after.encode_utf16().collect();
        let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let suffix = old[prefix..]
            .iter()
            .rev()
            .zip(new[prefix..].iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        prefix..old.len() - suffix
    };
    let tokens_of = |text: &str| {
        let mut tokens = Vec::new();
        let mut from = 0;
        while let Some(at) = text[from..].find('«') {
            let start = from + at;
            let end = start + text[start..].find('»').unwrap() + '»'.len_utf8();
            tokens.push(units(&text[..start])..units(&text[..end]));
            from = end;
        }
        tokens
    };
    let whole = [
        (
            "a: «secret 3f2a»\nb: «secret 0b1c»\n",
            "b: «secret 0b1c»\na: «secret 3f2a»\n",
        ),
        (
            "k3: app»\nk4: «secret 3f2a»\n",
            "k4: «secret 3f2a»\nk3: app»\n",
        ),
        (
            "l:\n  - «secret 11aa»\n  - «secret 22bb»\n",
            "l:\n  - «secret 22bb»\n",
        ),
    ];
    for (before, after) in whole {
        let region = diff(before, after);
        assert!(
            rewritten_region(&tokens_of(before), before, after, &region).is_some(),
            "{before:?} -> {after:?}"
        );
    }
    let cut = [
        ("a: «secret 3f2a»\n", "a: «secret 3f2»\n"),
        ("a: «secret 3f2a»\n", "a: «secrxet 3f2a»\n"),
        ("a: «secret 3f2a»\n", "a:«secret 3f2a»ret 3f2a»\n"),
    ];
    for (before, after) in cut {
        let region = diff(before, after);
        assert!(
            rewritten_region(&tokens_of(before), before, after, &region).is_none(),
            "{before:?} -> {after:?}"
        );
    }
}
