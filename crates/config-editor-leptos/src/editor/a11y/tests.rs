use super::caret_note::{caret_position, covers, note};
use super::focus::{Action, Context, Modifiers, decide};
use super::texts::problem_announcement;
use crate::editor::diagnostics::{Diagnostic, DiagnosticOrigin, Severity};
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
use crate::editor::languages::Language;

fn none() -> Modifiers {
    Modifiers::default()
}

fn shift() -> Modifiers {
    Modifiers {
        shift: true,
        ..Modifiers::default()
    }
}

fn ctrl() -> Modifiers {
    Modifiers {
        ctrl: true,
        ..Modifiers::default()
    }
}

/// Runs `keys` from a fresh editor (popups as `popup`, the list as `list`),
/// returning each key's action.
fn run(keys: &[(&str, Modifiers)], tab_moves_focus: bool, list: bool, popup: bool) -> Vec<Action> {
    let mut escaped = false;
    keys.iter()
        .map(|(key, modifiers)| {
            let context = Context {
                tab_moves_focus,
                toggles: true,
                escaped,
                list_open: list,
                popup_open: popup || list,
            };
            let (action, next) = decide(key, *modifiers, context);
            escaped = next;
            action
        })
        .collect()
}

#[test]
fn tab_leaves_only_in_the_tab_mode_or_right_after_an_idle_escape() {
    use Action::{Leave, Pass, Toggle};
    // (name, keys, Tab mode, list open, popup open, expected actions)
    type Case<'a> = (
        &'a str,
        Vec<(&'a str, Modifiers)>,
        bool,
        bool,
        bool,
        Vec<Action>,
    );
    let cases: Vec<Case<'_>> = vec![
        (
            "Tab indents",
            vec![("Tab", none())],
            false,
            false,
            false,
            vec![Pass],
        ),
        (
            "Shift+Tab outdents",
            vec![("Tab", shift())],
            false,
            false,
            false,
            vec![Pass],
        ),
        (
            "Ctrl+M switches",
            vec![("m", ctrl()), ("M", ctrl())],
            false,
            false,
            false,
            vec![Toggle, Toggle],
        ),
        (
            "Ctrl+Shift+M is not it",
            vec![(
                "M",
                Modifiers {
                    ctrl: true,
                    shift: true,
                    ..none()
                },
            )],
            false,
            false,
            false,
            vec![Pass],
        ),
        (
            "mode: Tab leaves",
            vec![("Tab", none()), ("Tab", shift())],
            true,
            false,
            false,
            vec![Leave, Leave],
        ),
        (
            "mode: Tab accepts a suggestion",
            vec![("Tab", none())],
            true,
            true,
            false,
            vec![Pass],
        ),
        (
            "mode: Ctrl+Tab is the browser's",
            vec![("Tab", ctrl())],
            true,
            false,
            false,
            vec![Pass],
        ),
        (
            "Escape, Tab leaves",
            vec![("Escape", none()), ("Tab", none())],
            false,
            false,
            false,
            vec![Pass, Leave],
        ),
        (
            "Escape, Shift, Shift+Tab leaves",
            vec![("Escape", none()), ("Shift", shift()), ("Tab", shift())],
            false,
            false,
            false,
            vec![Pass, Pass, Leave],
        ),
        (
            "only once",
            vec![("Escape", none()), ("Tab", none()), ("Tab", none())],
            false,
            false,
            false,
            vec![Pass, Leave, Pass],
        ),
        (
            "a key between disarms",
            vec![("Escape", none()), ("a", none()), ("Tab", none())],
            false,
            false,
            false,
            vec![Pass, Pass, Pass],
        ),
        (
            "an arrow between disarms",
            vec![("Escape", none()), ("ArrowDown", none()), ("Tab", none())],
            false,
            false,
            false,
            vec![Pass, Pass, Pass],
        ),
        (
            "Escape closing a popup does not arm",
            vec![("Escape", none()), ("Tab", none())],
            false,
            false,
            true,
            vec![Pass, Pass],
        ),
        (
            "Escape closing the list does not arm",
            vec![("Escape", none()), ("Tab", none())],
            false,
            true,
            false,
            vec![Pass, Pass],
        ),
    ];
    let mut failures = Vec::new();
    for (name, keys, mode, list, popup, expected) in cases {
        let found = run(&keys, mode, list, popup);
        if found != expected {
            failures.push(format!("{name}: expected {expected:?}, found {found:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// QA 32, A32-1: where `Ctrl+M` does not work (a read-only editor), the
/// key is not taken: no switch, no announcement.
#[test]
fn ctrl_m_switches_nothing_where_it_does_not_work() {
    let context = Context {
        toggles: false,
        ..Context::default()
    };
    for key in ["m", "M"] {
        assert_eq!(decide(key, ctrl(), context).0, Action::Pass, "{key}");
    }
    let context = Context {
        toggles: true,
        ..Context::default()
    };
    assert_eq!(decide("m", ctrl(), context).0, Action::Toggle);
}

#[test]
fn escape_after_closing_arms_the_next_escape() {
    // The first Escape closes the list; the second has nothing to close.
    let mut escaped = false;
    let mut actions = Vec::new();
    for (popup, key) in [(true, "Escape"), (false, "Escape"), (false, "Tab")] {
        let context = Context {
            escaped,
            popup_open: popup,
            ..Context::default()
        };
        let (action, next) = decide(key, none(), context);
        escaped = next;
        actions.push(action);
    }
    assert_eq!(actions, vec![Action::Pass, Action::Pass, Action::Leave]);
}

fn problem(line: usize, column: Option<usize>, end: Option<usize>) -> Diagnostic {
    Diagnostic {
        line: Some(line),
        column,
        end_column: end,
        message: format!("problem on {line}"),
        ..Diagnostic::default()
    }
}

#[test]
fn a_problem_covers_its_underline_and_the_caret_just_past_it() {
    let word = problem(2, Some(3), Some(7));
    let point = problem(2, Some(5), None);
    let line = problem(4, None, None);
    // QA 21: a host's range over several lines covered no caret past its
    // first line (a tap there showed nothing, the note said nothing).
    let range = Diagnostic {
        end_line: Some(6),
        ..problem(3, Some(4), Some(2))
    };
    let cases = [
        (&range, 3, 3, false),
        (&range, 3, 4, true),
        (&range, 4, 1, true),
        (&range, 5, 80, true),
        (&range, 6, 2, true),
        (&range, 6, 3, false),
        (&range, 7, 1, false),
        (&word, 2, 2, false),
        (&word, 2, 3, true),
        (&word, 2, 5, true),
        (&word, 2, 7, true),
        (&word, 2, 8, false),
        (&word, 3, 4, false),
        (&point, 2, 5, true),
        (&point, 2, 6, true),
        (&point, 2, 7, false),
        (&line, 4, 1, true),
        (&line, 4, 40, true),
        (&line, 5, 1, false),
    ];
    let failures: Vec<String> = cases
        .iter()
        .filter(|(problem, line, column, expected)| covers(problem, *line, *column) != *expected)
        .map(|(problem, line, column, expected)| {
            format!(
                "{:?} at {line}:{column}: expected {expected}",
                (problem.line, problem.column, problem.end_column)
            )
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_caret_position_counts_characters_not_units() {
    let text = "a: 1\nb: \"é😀x\"\n";
    // UTF-16 offset of `x`: line 2 is `b: "é😀x"`.
    let x = text[..text.find('x').unwrap()].encode_utf16().count();
    assert_eq!(caret_position(text, 0), (1, 1));
    assert_eq!(caret_position(text, 5), (2, 1));
    assert_eq!(caret_position(text, x), (2, 7));
}

/// A broken document per language, and where its syntax error is: the
/// caret note there names it, the note at the document's start does not.
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
#[test]
fn the_caret_note_names_the_syntax_error_under_it_in_every_language() {
    let strings = crate::EditorStrings::default();
    let cases = [
        (Language::YAML, "a: 1\nb: [1, 2\nc: 3\n"),
        (Language::JSON, "{\n  \"a\": 1,\n  \"b\": tru\n}\n"),
        (Language::TOML, "a = 1\nb = \nc = 3\n"),
        (Language::HCL, "a = 1\nb = {\nc = 3\n"),
        (Language::XML, "<a>1</a>\n<b>2</c>\n"),
    ];
    let mut failures = Vec::new();
    for (language, text) in cases {
        let problems = crate::editor::diagnostics::check_syntax(language, text);
        let Some(first) = problems.iter().find(|problem| problem.line.is_some()) else {
            failures.push(format!("{language:?}: no placed problem in {text:?}"));
            continue;
        };
        let line = first.line.unwrap();
        let column = first.column.unwrap_or(1);
        let found = note(&strings, &problems, line, column, None);
        if !found.contains(&first.message) || !found.starts_with("Error") {
            failures.push(format!(
                "{language:?}: at {line}:{column} the note is {found:?}"
            ));
        }
        let elsewhere = (1..=text.lines().count()).find(|candidate| {
            problems
                .iter()
                .all(|problem| problem.line != Some(*candidate))
        });
        if let Some(other) = elsewhere {
            let quiet = note(&strings, &problems, other, 1, None);
            if !quiet.is_empty() {
                failures.push(format!(
                    "{language:?}: line {other} has no problem, note {quiet:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_note_lists_every_problem_and_then_the_protected_text() {
    let mut host = problem(1, Some(1), Some(4));
    host.origin = DiagnosticOrigin::Host;
    host.source = Some("burrow".to_string());
    host.severity = Severity::Warning;
    host.message = "port is taken".to_string();
    let mut own = problem(1, Some(2), Some(3));
    own.message = "expected a number".to_string();
    let strings = crate::EditorStrings::default();
    let found = note(
        &strings,
        &[host, own],
        1,
        2,
        Some(("Read-only".to_string(), "Managed by the server".to_string())),
    );
    assert_eq!(
        found,
        "Warning from burrow: port is taken. Error: expected a number. Read-only: Managed by the server"
    );
    assert_eq!(
        note(
            &strings,
            &[],
            1,
            1,
            Some((
                "Secret placeholder".to_string(),
                "Type a new value".to_string()
            ))
        ),
        "Secret placeholder: Type a new value"
    );
}

#[test]
fn f8_announces_the_problem_with_its_place_in_the_list() {
    let strings = crate::EditorStrings::default();
    let mut first = problem(3, Some(1), None);
    first.message = "unknown key `lisen`".to_string();
    assert_eq!(
        problem_announcement(&strings, &first, 2, 5),
        "Error 2 of 5: unknown key `lisen`"
    );
    first.origin = DiagnosticOrigin::Host;
    first.source = Some("burrow".to_string());
    first.severity = Severity::Info;
    assert_eq!(
        problem_announcement(&strings, &first, 1, 1),
        "Info 1 of 1 from burrow: unknown key `lisen`"
    );
}

/// A host problem's code and docs link reach the keyboard too: the caret
/// note and the F8 announcement say them, as the pointer sees them under the
/// message.
#[test]
fn a_host_problems_code_and_docs_are_in_the_note_and_the_announcement() {
    let strings = crate::EditorStrings::default();
    let mut port = problem(2, Some(1), Some(7));
    port.message = "port 7835 is taken.".to_string();
    port.origin = DiagnosticOrigin::Host;
    port.source = Some("burrow".to_string());
    port.code = Some("E12".to_string());
    port.docs_url = Some("https://example.com/docs/ports".to_string());
    let said = " Code E12. Documentation: https://example.com/docs/ports (Alt+Enter opens it).";
    assert_eq!(
        problem_announcement(&strings, &port, 1, 1),
        format!("Error 1 of 1 from burrow: port 7835 is taken.{said}")
    );
    assert_eq!(
        note(&strings, std::slice::from_ref(&port), 2, 3, None),
        format!("Error from burrow: port 7835 is taken.{said}")
    );
    // A message without its full stop gets one before the code.
    port.message = "port 7835 is taken".to_string();
    assert_eq!(
        problem_announcement(&strings, &port, 1, 1),
        format!("Error 1 of 1 from burrow: port 7835 is taken.{said}")
    );
}

/// A field the host locks, in every language: the caret on its value (and
/// just past it) is described by the lock's reason, the caret on another
/// field is not.
#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml",
    feature = "protect"
))]
#[test]
fn the_caret_note_says_why_a_locked_value_is_read_only_in_every_language() {
    use super::caret_note::update_note;
    use crate::editor::paths::FieldPath;
    use crate::editor::test_state::TestEditor;
    use leptos::prelude::*;
    let cases = [
        (Language::YAML, "id: srv-42\nname: edge\n"),
        (
            Language::JSON,
            "{\n  \"id\": \"srv-42\",\n  \"name\": \"edge\"\n}\n",
        ),
        (Language::TOML, "id = \"srv-42\"\nname = \"edge\"\n"),
        (Language::HCL, "id = \"srv-42\"\nname = \"edge\"\n"),
        (Language::XML, "<id>srv-42</id>\n<name>edge</name>\n"),
    ];
    let mut failures = Vec::new();
    for (language, text) in cases {
        let (_owner, state) = TestEditor {
            read_only_paths: vec![FieldPath::from("/id")],
            ..TestEditor::new(language, text)
        }
        .mount();
        let note_at = |needle: &str, past: usize| {
            let byte = text.find(needle).unwrap() + past;
            state.doc().caret.set(text[..byte].encode_utf16().count());
            update_note(state);
            state.a11y().caret_note.get_untracked()
        };
        let inside = note_at("srv-42", 2);
        if !inside.starts_with("Read-only: ") {
            failures.push(format!("{language:?}: inside the value, note {inside:?}"));
        }
        let after = note_at("srv-42", 6);
        if !after.starts_with("Read-only: ") {
            failures.push(format!("{language:?}: just past the value, note {after:?}"));
        }
        let other = note_at("edge", 2);
        if !other.is_empty() {
            failures.push(format!("{language:?}: on `name`, note {other:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// QA 20: `Ctrl+K Ctrl+I` read "Port to listen on.. Default: 7835": a
/// description written as a sentence got a second full stop.
#[test]
fn the_docs_announcement_has_one_full_stop_after_each_part() {
    use super::texts::hover_text;
    use crate::editor::completion::KeyHover;
    let hover = KeyHover {
        key: "listen".into(),
        start_column: 0,
        type_label: "integer".into(),
        title: Some("Port".into()),
        description: Some("Port to listen on.".into()),
        default: Some("7835".into()),
        examples: vec!["80".into()],
        required: true,
        deprecated: false,
        deprecation: Some("Use ports instead! ".into()),
        docs_url: None,
        choice: None,
        value: None,
        item: None,
    };
    assert_eq!(
        hover_text(&crate::EditorStrings::default(), &hover, true),
        "listen (integer), required. Port. Use ports instead! Port to listen on. Default: 7835. Examples: 80"
    );
}

/// QA 23: the card's Documentation link was not in its announcement, so a
/// screen reader never heard of it (nor of `Alt+Enter`, which opens it).
#[test]
fn the_docs_announcement_names_the_documentation_link() {
    use super::texts::hover_text;
    use crate::editor::completion::KeyHover;
    let hover = KeyHover {
        key: "mode".into(),
        start_column: 0,
        type_label: "string".into(),
        title: None,
        description: Some("Routing mode.".into()),
        default: None,
        examples: Vec::new(),
        required: false,
        deprecated: false,
        deprecation: None,
        docs_url: Some("https://example.com/docs/mode".into()),
        choice: None,
        value: None,
        item: None,
    };
    let strings = crate::EditorStrings::default();
    assert_eq!(
        hover_text(&strings, &hover, true),
        "mode (string). Routing mode. Documentation: https://example.com/docs/mode (Alt+Enter opens it)."
    );
    // A problem's own link takes Alt+Enter: the card's is only named.
    assert_eq!(
        hover_text(&strings, &hover, false),
        "mode (string). Routing mode. Documentation: https://example.com/docs/mode"
    );
}

#[cfg(all(
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
#[test]
fn the_same_announcement_twice_is_said_twice() {
    use crate::editor::test_state::TestEditor;
    use leptos::prelude::*;
    let (_owner, state) = TestEditor::new(Language::YAML, "a: 1\n").mount();
    let mut said = Vec::new();
    for message in ["one", "one", "one", "two"] {
        state.a11y().announce(message.to_string());
        said.push(state.a11y().announcement.get_untracked());
    }
    assert_eq!(said[0], "one");
    assert_ne!(said[1], said[0]);
    assert_ne!(said[2], said[1]);
    assert_eq!(
        said.iter()
            .map(|text| text.trim_end_matches('\u{a0}'))
            .collect::<Vec<_>>(),
        ["one", "one", "one", "two"]
    );
}

#[cfg(feature = "yaml")]
#[test]
fn a_read_only_editor_says_why_when_an_edit_is_refused() {
    use crate::editor::languages::Language as Lang;
    use crate::editor::test_state::TestEditor;
    use leptos::prelude::*;
    let mut editor = TestEditor::new(Lang::YAML, "a: 1\n");
    editor.read_only = true;
    editor.read_only_reason = Some("Managed by the operator".into());
    let (_owner, state) = editor.mount();
    state.note_read_only();
    assert_eq!(
        state.a11y().announcement.get_untracked(),
        "Cannot edit in a read-only editor: Managed by the operator"
    );
    assert_eq!(
        untrack(|| state.read_only_reason()).as_deref(),
        Some("Managed by the operator")
    );
    // Without a reason (or an empty one), the plain notice.
    for reason in [None, Some("  ".to_string())] {
        let mut editor = TestEditor::new(Lang::YAML, "a: 1\n");
        editor.read_only = true;
        editor.read_only_reason = reason;
        let (_owner, state) = editor.mount();
        state.note_read_only();
        assert_eq!(
            state.a11y().announcement.get_untracked(),
            "Cannot edit in a read-only editor"
        );
        assert_eq!(untrack(|| state.read_only_reason()), None);
    }
    // Editable, a reason says nothing.
    let mut editor = TestEditor::new(Lang::YAML, "a: 1\n");
    editor.read_only_reason = Some("Managed by the operator".into());
    let (_owner, state) = editor.mount();
    assert_eq!(untrack(|| state.read_only_reason()), None);
}

/// QA 26: the editing commands the browser types nothing for (cutting a
/// line, Ctrl+/, Ctrl+Enter, undo) were refused in a read-only editor
/// without a word, while typing said it cannot edit. Every edit the
/// editor refuses there says so, whatever made it.
#[cfg(feature = "yaml")]
#[test]
fn every_edit_refused_in_a_read_only_editor_says_so() {
    use crate::editor::edit::Edit;
    use crate::editor::test_state::TestEditor;
    use leptos::prelude::*;
    for language in crate::editor::languages::Language::ALL {
        let text = "first\nsecond\n";
        let mut editor = TestEditor::new(*language, text);
        editor.read_only = true;
        let (_owner, state) = editor.mount();
        for (selection, updated, caret) in [
            ((0, 0), "second\n", 0),
            ((2, 2), "first\n\nsecond\n", 6),
            ((0, 5), "// first\nsecond\n", 8),
        ] {
            state.a11y().announcement.set(String::new());
            state.hide_read_only_notice();
            let edit = Edit::new(text.into(), selection, updated.into(), caret);
            assert!(!state.commit(edit), "{language:?}");
            assert_eq!(state.doc().text.get_untracked(), text, "{language:?}");
            assert_eq!(
                state.a11y().announcement.get_untracked(),
                "Cannot edit in a read-only editor",
                "{language:?} {updated:?}"
            );
            assert!(
                state.a11y().read_only_notice.get_untracked().is_some(),
                "{language:?}"
            );
        }
    }
}
