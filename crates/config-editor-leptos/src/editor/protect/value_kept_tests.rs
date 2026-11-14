//! One promise, whatever edit makes the change: a placeholder the host gave
//! as a field's whole value never has text beside it in anything the host
//! hears, and a read-only field (a path, or a host range over its value)
//! never changes. Checked in every language, for each way a value is
//! written (on one line, quoted, YAML block scalars of each chomping, TOML
//! `'''` and `"""`, HCL heredocs, XML text on one line and over lines), by
//! every route an edit takes (typing beside it, Enter and typing, indenting
//! the next line by spaces, Tab or `Ctrl+]`, outdenting, moving and copying
//! lines, deleting, cutting and selecting away the line between, a closing
//! line deleted, Replace All, a paste, a drag, undo and redo), both while
//! the text parses and while it has a syntax error elsewhere (the error
//! then repaired: the repair is never refused, and what it lets the host
//! hear keeps the promise).
//!
//! The text-level check went by the shape of the edit and only while the
//! text parsed: a line indented into a block scalar or heredoc holding a
//! placeholder, or text typed beside it while the text did not parse, gave
//! the host `«secret 3f2a»\nb: y\n` or `«secret 3f2a»MYPASS`.

use leptos::prelude::*;
use serde_json::Value;

use super::super::Language;
use super::super::codec;
use super::super::edit::Edit;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run};
use super::super::paths::{FieldPath, TextPosition, TextRange};
use super::super::save_matrix::report;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::glued_tests::breaking;
use super::guarantee_tests::utf16;

const TOKEN: &str = "«secret 3f2a91c0»";
const PATTERN: &str = "«secret [0-9a-f]{8}»";

/// One way `/a` is written, holding `TOKEN` alone, with the sibling `b`
/// on the next field's line and a container `c` holding `d` after it.
struct Form {
    language: Language,
    name: &'static str,
    text: String,
}

fn forms() -> Vec<Form> {
    let form = |language, name, text: String| Form {
        language,
        name,
        text,
    };
    let yaml = |name, value: &str| {
        form(
            Language::YAML,
            name,
            format!("name: x\na: {value}\nb: y\nc:\n  d: 1\ne: 2\n"),
        )
    };
    let toml = |name, value: &str| {
        form(
            Language::TOML,
            name,
            format!("name = \"x\"\na = {value}\nb = \"y\"\n\n[c]\nd = 1\n"),
        )
    };
    let hcl = |name, value: &str| {
        form(
            Language::HCL,
            name,
            format!("name = \"x\"\na = {value}\nb = \"y\"\nc {{\n  d = 1\n}}\ne = 2\n"),
        )
    };
    let xml = |name, value: &str| {
        form(
            Language::XML,
            name,
            format!("<name>x</name>\n<a>{value}</a>\n<b>y</b>\n<c>\n  <d>1</d>\n</c>\n<e>2</e>\n"),
        )
    };
    vec![
        yaml("plain", TOKEN),
        yaml("quoted", &format!("\"{TOKEN}\"")),
        yaml("|", &format!("|\n  {TOKEN}")),
        yaml("|-", &format!("|-\n  {TOKEN}")),
        yaml(">", &format!(">\n  {TOKEN}")),
        yaml(">-", &format!(">-\n  {TOKEN}")),
        yaml("|+", &format!("|+\n  {TOKEN}\n")),
        form(
            Language::JSON,
            "string",
            format!(
                "{{\n  \"name\": \"x\",\n  \"a\": \"{TOKEN}\",\n  \"b\": \"y\",\n  \"c\": {{\n    \"d\": 1\n  }}\n}}\n"
            ),
        ),
        toml("string", &format!("\"{TOKEN}\"")),
        toml("'''", &format!("'''\n{TOKEN}\n'''")),
        toml("\"\"\"", &format!("\"\"\"\n{TOKEN}\n\"\"\"")),
        toml(
            "\"\"\" closed on its line",
            &format!("\"\"\"\n{TOKEN}\"\"\""),
        ),
        hcl("string", &format!("\"{TOKEN}\"")),
        hcl("<<-EOT", &format!("<<-EOT\n  {TOKEN}\n  EOT")),
        hcl("<<EOT", &format!("<<EOT\n{TOKEN}\nEOT")),
        xml("text", TOKEN),
        xml("text over lines", &format!("\n  {TOKEN}\n")),
    ]
}

/// What protects `/a`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `TOKEN` is the host's placeholder: nothing beside it.
    Secret,
    /// A read-only path: its value never changes.
    Path,
    /// A host range over the token's text: the field's value never changes.
    Range,
}

const KINDS: [Kind; 3] = [Kind::Secret, Kind::Path, Kind::Range];

/// The text's lines as byte ranges (without their line breaks).
fn lines(text: &str) -> Vec<std::ops::Range<usize>> {
    let mut start = 0;
    text.split('\n')
        .map(|line| {
            let range = start..start + line.len();
            start = range.end + 1;
            range
        })
        .collect()
}

/// The index of the first line of `text` holding `needle`.
fn line_of(text: &str, needle: &str) -> usize {
    text.split('\n')
        .position(|line| line.contains(needle))
        .unwrap_or_else(|| panic!("no {needle:?} in {text:?}"))
}

/// The line holding the sibling `b`, in `language`.
fn sibling(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "b: y",
        "JSON" => "\"b\"",
        "XML" => "<b>",
        _ => "b = ",
    }
}

/// The line holding the nested `d`.
fn nested(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "d: 1",
        "JSON" => "\"d\"",
        "XML" => "<d>",
        _ => "d = ",
    }
}

/// A new field typed on a line of its own.
fn new_field(language: Language) -> &'static str {
    match language.label() {
        "YAML" => "n: 2\n",
        "JSON" => "\"n\": 2,\n  ",
        "XML" => "<n>2</n>\n",
        _ => "n = 2\n",
    }
}

fn position(text: &str, at: usize) -> TextPosition {
    let before = &text[..at];
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    TextPosition::new(before.matches('\n').count() + 1, column)
}

/// A live editor on `text`, `/a` protected as `kind`, with what
/// `on_change` heard.
fn mount(
    language: Language,
    text: &str,
    kind: Kind,
) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(language, text);
    match kind {
        Kind::Secret => {
            editor.secrets = Some(SecretOptions::default().with_placeholder(PATTERN));
        }
        Kind::Path => editor.read_only_paths = vec![FieldPath::from("/a")],
        Kind::Range => {
            let at = text.find(TOKEN).expect("the token");
            editor.read_only_ranges = vec![TextRange::new(
                position(text, at),
                position(text, at + TOKEN.len()),
            )];
        }
    }
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// Replaces bytes `range` of the text on screen with `with`, as an input
/// of `kind`.
fn input(state: EditorState, range: std::ops::Range<usize>, with: &str, kind: &str) -> bool {
    let before = shown(state);
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let (start, end) = (utf16(&before, range.start), utf16(&before, range.end));
    let caret = start + with.encode_utf16().count();
    state.commit(Edit::input(
        before,
        (start, end),
        after,
        (caret, caret),
        kind.into(),
        0.0,
    ))
}

/// Types `typed` at byte `at`, a key at a time, until one is refused;
/// whether all of it was typed.
fn type_at(state: EditorState, mut at: usize, typed: &str) -> bool {
    for character in typed.chars() {
        let kind = if character == '\n' {
            "insertLineBreak"
        } else {
            "insertText"
        };
        if !input(state, at..at, &character.to_string(), kind) {
            return false;
        }
        at += character.len_utf8();
    }
    true
}

/// Runs a line command with the caret at the start of line `line`, or with
/// lines `line..=to` selected.
fn command(state: EditorState, command: Command, line: usize, to: Option<usize>) {
    let text = shown(state);
    let all = lines(&text);
    let start = utf16(&text, all[line].start);
    let end = to.map_or(start, |to| utf16(&text, all[to].end));
    run(state, command, (start, end), false);
}

/// The bytes of line `line` with its line break.
fn whole_line(text: &str, line: usize) -> std::ops::Range<usize> {
    let all = lines(text);
    all[line].start..(all[line].end + 1).min(text.len())
}

/// One route an edit takes, run on the editor.
type Route = fn(EditorState, Language);

fn token_end(state: EditorState) -> usize {
    shown(state).find(TOKEN).expect("the token") + TOKEN.len()
}

/// The line after `/a`'s value: the sibling's.
fn next_line(state: EditorState, language: Language) -> usize {
    line_of(&shown(state), sibling(language))
}

fn routes() -> Vec<(&'static str, Route)> {
    vec![
        ("Z typed after it", |state, _| {
            type_at(state, token_end(state), "Z");
        }),
        ("Z typed before it", |state, _| {
            let at = shown(state).find(TOKEN).unwrap();
            type_at(state, at, "Z");
        }),
        ("MYPASS typed at its line's end", |state, _| {
            let text = shown(state);
            let line = lines(&text)[line_of(&text, TOKEN)].clone();
            type_at(state, line.end, "MYPASS");
        }),
        ("Enter after it and a word", |state, _| {
            type_at(state, token_end(state), "\nabc");
        }),
        (
            "a field typed on an indented line after it",
            |state, language| {
                let typed = format!("\n  {}", new_field(language).trim_end());
                type_at(state, token_end(state), &typed);
            },
        ),
        ("Enter at its line's start and a word", |state, _| {
            let text = shown(state);
            let line = lines(&text)[line_of(&text, TOKEN)].clone();
            if type_at(state, line.start, "\n") {
                type_at(state, line.start, "abc");
            }
        }),
        (
            "two spaces typed before the next line",
            |state, language| {
                let text = shown(state);
                let at = lines(&text)[next_line(state, language)].start;
                type_at(state, at, "  ");
            },
        ),
        (
            "four spaces typed before the next line",
            |state, language| {
                let text = shown(state);
                let at = lines(&text)[next_line(state, language)].start;
                type_at(state, at, "    ");
            },
        ),
        (
            "Tab (a tab stop) before the next line",
            |state, language| {
                let text = shown(state);
                let at = lines(&text)[next_line(state, language)].start;
                let unit = state.doc().indent.get_value();
                let tab = super::super::commands::tab_insert(&text, utf16(&text, at), &unit);
                input(state, at..at, &tab, "insertText");
            },
        ),
        ("Ctrl+] on the next line", |state, language| {
            let line = next_line(state, language);
            command(state, Command::Indent { outdent: false }, line, None);
        }),
        ("Tab over the next lines", |state, language| {
            let line = next_line(state, language);
            let text = shown(state);
            let to = line_of(&text, nested(language));
            command(state, Command::Indent { outdent: false }, line, Some(to));
        }),
        ("Ctrl+] on its line", |state, _| {
            let line = line_of(&shown(state), TOKEN);
            command(state, Command::Indent { outdent: false }, line, None);
        }),
        ("Ctrl+[ on its line", |state, _| {
            let line = line_of(&shown(state), TOKEN);
            command(state, Command::Indent { outdent: true }, line, None);
        }),
        ("Ctrl+[ on the next line", |state, language| {
            let line = next_line(state, language);
            command(state, Command::Indent { outdent: true }, line, None);
        }),
        ("Alt+Up on the nested line", |state, language| {
            let line = line_of(&shown(state), nested(language));
            let up = Command::Lines(LineCommand::Move { down: false });
            command(state, up, line, None);
            let line = line_of(&shown(state), nested(language));
            command(state, up, line, None);
        }),
        ("Alt+Up on the next line", |state, language| {
            let line = next_line(state, language);
            command(
                state,
                Command::Lines(LineCommand::Move { down: false }),
                line,
                None,
            );
        }),
        ("Alt+Down on its line", |state, _| {
            let line = line_of(&shown(state), TOKEN);
            command(
                state,
                Command::Lines(LineCommand::Move { down: true }),
                line,
                None,
            );
        }),
        ("Shift+Alt+Down on its line", |state, _| {
            let line = line_of(&shown(state), TOKEN);
            command(
                state,
                Command::Lines(LineCommand::Copy { down: true }),
                line,
                None,
            );
        }),
        ("Shift+Alt+Up on the next line", |state, language| {
            let line = next_line(state, language);
            command(
                state,
                Command::Lines(LineCommand::Copy { down: false }),
                line,
                None,
            );
        }),
        ("Ctrl+Shift+K on the line after it", |state, _| {
            let line = line_of(&shown(state), TOKEN) + 1;
            command(state, Command::Lines(LineCommand::Delete), line, None);
        }),
        ("Ctrl+Shift+K on the next line", |state, language| {
            let line = next_line(state, language);
            command(state, Command::Lines(LineCommand::Delete), line, None);
        }),
        ("Ctrl+Shift+K above the nested line", |state, language| {
            let line = line_of(&shown(state), nested(language)) - 1;
            command(state, Command::Lines(LineCommand::Delete), line, None);
        }),
        ("the line after it cut", |state, _| {
            let text = shown(state);
            let range = whole_line(&text, line_of(&text, TOKEN) + 1);
            input(state, range, "", "deleteByCut");
        }),
        ("the line after it selected and deleted", |state, _| {
            let text = shown(state);
            let range = whole_line(&text, line_of(&text, TOKEN) + 1);
            input(state, range, "", "deleteContentForward");
        }),
        ("the line break after it deleted", |state, _| {
            let at = token_end(state);
            let text = shown(state);
            let end = text[at..].find('\n').map_or(text.len(), |found| at + found);
            input(state, end..end + 1, "", "deleteContentForward");
        }),
        ("Replace All indenting the next line", |state, language| {
            let before = shown(state);
            let needle = format!("\n{}", sibling(language));
            let replaced = before.replace(&needle, &format!("\n    {}", sibling(language)));
            let caret = utf16(&before, 0);
            state.commit(Edit::new(before, (caret, caret), replaced, caret));
        }),
        ("a line pasted after it", |state, language| {
            let text = shown(state);
            let at = lines(&text)[next_line(state, language)].start;
            input(state, at..at, "    zzz\n", "insertFromPaste");
        }),
        ("the nested line dragged after it", |state, language| {
            let text = shown(state);
            let range = whole_line(&text, line_of(&text, nested(language)));
            let moved = text[range.clone()].to_string();
            if input(state, range, "", "deleteByDrag") {
                let text = shown(state);
                let at = lines(&text)[next_line(state, language)].start;
                input(state, at..at, &moved, "insertFromDrop");
            }
        }),
    ]
}

/// What `/a` reads as in `text` (`None`: the text does not parse).
fn field_a(language: Language, text: &str) -> Option<Option<Value>> {
    let document = codec::read(language.behavior(), text, None).ok()?;
    Some(document.pointer("/a").cloned())
}

/// What breaks the promise in `text`, when it parses.
fn broken_promise(language: Language, kind: Kind, given: &Value, text: &str) -> Option<String> {
    let now = field_a(language, text)?;
    match kind {
        Kind::Secret => {
            let value = now.as_ref().and_then(Value::as_str)?;
            let whole = given.as_str().unwrap_or_default();
            (value.contains(TOKEN) && value != whole && value != TOKEN)
                .then(|| format!("glued {value:?}"))
        }
        Kind::Path | Kind::Range => {
            (now.as_ref() != Some(given)).then(|| format!("changed to {now:?}"))
        }
    }
}

/// Every text the host heard, and the text on screen, keep the promise.
fn check(
    failures: &mut Vec<String>,
    case: &str,
    form: &Form,
    kind: Kind,
    state: EditorState,
    heard: RwSignal<Vec<String>>,
) {
    let given = field_a(form.language, &form.text)
        .flatten()
        .expect("the form parses");
    for text in heard.get_untracked().iter().chain([&shown(state)]) {
        if let Some(broken) = broken_promise(form.language, kind, &given, text) {
            failures.push(format!("{case}: {broken} in {text:?}"));
            return;
        }
    }
}

/// Removes the syntax error appended to the text: never refused.
fn repair(failures: &mut Vec<String>, case: &str, state: EditorState, language: Language) {
    let text = shown(state);
    let error = breaking(language);
    let Some(at) = text.rfind(error) else {
        failures.push(format!("{case}: the syntax error is gone: {text:?}"));
        return;
    };
    if !input(state, at..at + error.len(), "", "deleteContentBackward") {
        failures.push(format!("{case}: the repair was refused: {text:?}"));
    }
}

/// Undo, then redo, of what the route made.
fn undo_redo(state: EditorState) {
    for redo in [false, true] {
        if let Some(edit) = state.history_edit(redo) {
            state.commit(edit);
        }
    }
}

#[test]
fn no_edit_glues_a_placeholder_or_changes_a_locked_value_in_any_form_or_state() {
    let mut failures = Vec::new();
    for form in forms() {
        let language = form.language;
        for kind in KINDS {
            for broken in [false, true] {
                let text = if broken {
                    format!("{}{}", form.text, breaking(language))
                } else {
                    form.text.clone()
                };
                for (route, act) in routes() {
                    let case = format!(
                        "{language} {} {kind:?}{}, {route}",
                        form.name,
                        if broken { " (syntax error below)" } else { "" }
                    );
                    let (_owner, state, heard) = mount(language, &text, kind);
                    act(state, language);
                    undo_redo(state);
                    if broken {
                        repair(&mut failures, &case, state, language);
                    }
                    check(&mut failures, &case, &form, kind, state, heard);
                }
            }
        }
    }
    report(failures);
}

/// The fields around a protected value stay free: a field typed on a new
/// line of its own right after it, a key at a time, is let through, while
/// the text parses and while it has a syntax error elsewhere.
#[test]
fn a_field_typed_on_a_new_line_after_a_protected_value_is_let_through() {
    let mut failures = Vec::new();
    for form in forms() {
        let language = form.language;
        for kind in KINDS {
            for broken in [false, true] {
                let text = if broken {
                    format!("{}{}", form.text, breaking(language))
                } else {
                    form.text.clone()
                };
                let (_owner, state, _) = mount(language, &text, kind);
                let at = lines(&text)[line_of(&text, sibling(language))].start;
                let at = at + text[at..].len() - text[at..].trim_start().len();
                let typed = new_field(language);
                if !type_at(state, at, typed) {
                    failures.push(format!(
                        "{language} {} {kind:?}{}: {typed:?} refused at {:?}",
                        form.name,
                        if broken { " (syntax error below)" } else { "" },
                        shown(state)
                    ));
                }
            }
        }
    }
    report(failures);
}
