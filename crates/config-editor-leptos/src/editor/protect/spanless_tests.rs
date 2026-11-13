//! Protected fields in a language that cannot say where a field is written
//! (only `parse` and `serialize`, no spans): every locked value still holds
//! (I1), checked by value on every edit whose text parses, and edits that
//! keep every locked value stay free (I2).

use leptos::prelude::*;
use serde_json::{Map, Value, json};

use super::super::Language;
use super::super::edit::Edit;
use super::super::host_editing::SaveRequest;
use super::super::language::{ConfigLanguage, LanguageInfo, ParseError};
use super::super::paths::FieldPath;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::problems;
use super::guard::check;
use super::locks::{Protected, Sources, protect};
use super::save::locked_pointers;

/// `KEY=value` lines, `#` comments, `\\` at a line's end going on on the
/// next line; nothing else.
pub(super) struct Env;

impl ConfigLanguage for Env {
    fn info(&self) -> LanguageInfo {
        LanguageInfo {
            label: "Env",
            icon: "E",
            extensions: &["env"],
        }
    }

    fn parse(&self, source: &str) -> Result<Value, ParseError> {
        let mut map = Map::new();
        // A line ending in `\` goes on on the next one.
        let joined = source.replace("\\\n", " ");
        for (index, line) in joined.split('\n').enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(ParseError {
                    message: "expected KEY=value".to_string(),
                    line: Some(index + 1),
                    column: Some(1),
                });
            };
            let value = value.trim();
            let value = value
                .parse::<i64>()
                .map_or_else(|_| json!(value), |number| json!(number));
            map.insert(key.trim().to_string(), value);
        }
        Ok(Value::Object(map))
    }

    fn serialize(&self, value: &Value, _previous: Option<&str>) -> Result<String, String> {
        let Value::Object(map) = value else {
            return Err("not an object".to_string());
        };
        Ok(map
            .iter()
            .map(|(key, value)| match value {
                Value::String(text) => format!("{key}={text}\n"),
                other => format!("{key}={other}\n"),
            })
            .collect())
    }
}

pub(super) const TEXT: &str = "# c\nID=srv-1\nNAME=a\nPORT=80\n";

fn schema() -> Value {
    json!({"type": "object", "properties": {
        "ID": {"type": "string", "readOnly": true},
        "PORT": {"type": "integer"}
    }})
}

fn locks(text: &str, previous: Option<&Protected>) -> Protected {
    let schema = schema();
    let paths = [FieldPath::from("/PORT")];
    protect(
        &Env,
        text,
        Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            paths: &paths,
            previous,
            ..Sources::default()
        },
    )
}

fn allowed(protected: &Protected, before: &str, after: &str) -> bool {
    check(&Env, Some(&schema()), protected, before, after).is_ok()
}

#[test]
fn every_edit_changing_a_locked_value_is_refused_without_spans() {
    let protected = locks(TEXT, None);
    let cases = [
        (
            "type in a locked value",
            "# c\nID=srv-19\nNAME=a\nPORT=80\n",
        ),
        (
            "type in a host-locked value",
            "# c\nID=srv-1\nNAME=a\nPORT=809\n",
        ),
        ("delete a locked line", "# c\nNAME=a\nPORT=80\n"),
        ("rename a locked key", "# c\nXD=srv-1\nNAME=a\nPORT=80\n"),
        (
            "duplicate overriding",
            "# c\nID=srv-1\nNAME=a\nPORT=80\nID=evil\n",
        ),
        ("comment a locked line", "# c\n#ID=srv-1\nNAME=a\nPORT=80\n"),
        ("replace all", "ID=srv-2\nPORT=81\n"),
    ];
    let failures: Vec<&str> = cases
        .iter()
        .filter(|(_, after)| allowed(&protected, TEXT, after))
        .map(|(name, _)| *name)
        .collect();
    assert!(failures.is_empty(), "accepted: {failures:?}");
}

#[test]
fn edits_keeping_every_locked_value_stay_free_without_spans() {
    let protected = locks(TEXT, None);
    let cases = [
        ("edit a comment", "# comment\nID=srv-1\nNAME=a\nPORT=80\n"),
        ("edit a free value", "# c\nID=srv-1\nNAME=abc\nPORT=80\n"),
        ("add a key", "# c\nID=srv-1\nNAME=a\nPORT=80\nNEW=1\n"),
        ("delete a free line", "# c\nID=srv-1\nPORT=80\n"),
        ("comment a free line", "# c\nID=srv-1\n#NAME=a\nPORT=80\n"),
        (
            "space after a locked value",
            "# c\nID=srv-1 \nNAME=a\nPORT=80\n",
        ),
    ];
    let failures: Vec<&str> = cases
        .iter()
        .filter(|(_, after)| !allowed(&protected, TEXT, after))
        .map(|(name, _)| *name)
        .collect();
    assert!(failures.is_empty(), "refused: {failures:?}");
}

#[test]
fn a_text_broken_on_the_way_parses_again_only_with_the_locked_values() {
    let protected = locks(TEXT, None);
    let broken = "# c\nID=srv-1\nNAMEa\nPORT=80\n";
    assert!(
        allowed(&protected, TEXT, broken),
        "breaking a free line is free"
    );
    let held = locks(broken, Some(&protected));
    assert!(
        !allowed(&held, broken, "# c\nID=srv-2\nNAME=a\nPORT=80\n"),
        "repaired with another value"
    );
    assert!(
        !allowed(&held, broken, "# c\nNAME=a\nPORT=80\n"),
        "repaired by deleting the locked line"
    );
    assert!(
        allowed(&held, broken, TEXT),
        "repaired with the locked value"
    );
}

#[test]
fn the_save_check_names_the_fields_the_language_cannot_locate() {
    let schema = schema();
    let paths = [FieldPath::from("/PORT")];
    let mut pointers = locked_pointers(
        &Env,
        TEXT,
        Sources {
            marking_schema: Some(&schema),
            reading_schema: Some(&schema),
            paths: &paths,
            ..Sources::default()
        },
    );
    pointers.sort();
    assert_eq!(pointers, ["/ID", "/PORT"]);
}

/// Each step from `TEXT`, every one checked against the locks of the text
/// before it (as the editor reads them): the index of the first refused.
fn first_refused(steps: &[&str]) -> Option<usize> {
    let mut protected = locks(TEXT, None);
    let mut before = TEXT.to_string();
    for (index, after) in steps.iter().enumerate() {
        if !allowed(&protected, &before, after) {
            return Some(index);
        }
        protected = locks(after, Some(&protected));
        before = after.to_string();
    }
    None
}

#[test]
fn a_locked_value_holds_through_broken_text_without_spans() {
    let broken = "# c\nID=srv-1\nNAMEa\nPORT=80\n";
    let cases: [(&str, &[&str]); 8] = [
        (
            "edit a locked value while another line is broken",
            &[broken, "# c\nID=srv-19\nNAMEa\nPORT=80\n"],
        ),
        (
            "edit a host-locked value while another line is broken",
            &[broken, "# c\nID=srv-1\nNAMEa\nPORT=8\n"],
        ),
        (
            "replace a locked line with broken text",
            &["# c\nz\nNAME=a\nPORT=80\n"],
        ),
        ("break a locked line", &["# c\nIDsrv-1\nNAME=a\nPORT=80\n"]),
        (
            "delete a locked line while another is broken",
            &[broken, "# c\nNAMEa\nPORT=80\n"],
        ),
        (
            "a space after a locked value, then a new value",
            &[
                broken,
                "# c\nID=srv-1 \nNAMEa\nPORT=80\n",
                "# c\nID=srv-2 \nNAMEa\nPORT=80\n",
            ],
        ),
        (
            "a duplicate of a locked key while broken",
            &[broken, "# c\nID=srv-1\nNAMEa\nPORT=80\nID=evil\n"],
        ),
        ("replace everything with broken text", &["x"]),
    ];
    let failures: Vec<&str> = cases
        .iter()
        .filter(|(_, steps)| first_refused(steps) != Some(steps.len() - 1))
        .map(|(name, _)| *name)
        .collect();
    assert!(
        failures.is_empty(),
        "not refused at the last step: {failures:?}"
    );
}

#[test]
fn edits_keeping_every_locked_value_stay_free_in_broken_text_without_spans() {
    let broken = "# c\nID=srv-1\nNAMEa\nPORT=80\n";
    let cases: [(&str, &[&str]); 6] = [
        ("break a free line", &[broken]),
        (
            "edit the broken line",
            &[broken, "# c\nID=srv-1\nNAMEab\nPORT=80\n"],
        ),
        (
            "edit a comment while broken",
            &[broken, "# comment\nID=srv-1\nNAMEa\nPORT=80\n"],
        ),
        (
            "add a key while broken",
            &[broken, "# c\nID=srv-1\nNAMEa\nPORT=80\nNEW=1\n"],
        ),
        (
            "a space after a locked value while broken",
            &[broken, "# c\nID=srv-1 \nNAMEa\nPORT=80\n"],
        ),
        (
            "a new line after a locked value while broken",
            &[broken, "# c\nID=srv-1\n\nNAMEa\nPORT=80\n"],
        ),
    ];
    let failures: Vec<String> = cases
        .iter()
        .filter_map(|(name, steps)| first_refused(steps).map(|at| format!("{name} (step {at})")))
        .collect();
    assert!(failures.is_empty(), "refused: {failures:?}");
}

pub(super) static ENV: Env = Env;

/// The editor on `TEXT` in `Env`, `/PORT` and `/ID` read-only by path,
/// with the saves it sends.
fn mounted() -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let saves = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(Language::new(&ENV), TEXT);
    editor.read_only_paths = vec![FieldPath::from("/PORT"), FieldPath::from("/ID")];
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        saves.update(|all| all.push(request.text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, saves)
}

/// Replaces the first `old` of the text on screen with `new`.
fn change(state: EditorState, old: &str, new: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let at = before.find(old).expect("the text to change");
    let after = format!("{}{new}{}", &before[..at], &before[at + old.len()..]);
    state.commit(Edit::new(
        before,
        (at, at + old.len()),
        after,
        at + new.len(),
    ))
}

#[test]
fn a_save_waits_for_a_text_that_parses_without_spans() {
    let (_owner, state, saves) = mounted();
    assert!(state.save(), "a text that parses is saved");
    assert!(
        change(state, "NAME=a", "NAMEa"),
        "breaking a free line is free"
    );
    assert!(!state.save(), "a broken text holding a locked field waits");
    assert_eq!(
        saves.get_untracked(),
        [TEXT],
        "only the valid text was sent"
    );
    let tip = state.protect().tip.get_untracked().map(|tip| tip.message);
    assert!(
        tip.as_deref()
            .is_some_and(|tip| tip.contains("/PORT") || tip.contains("/ID")),
        "the reason shows: {tip:?}"
    );
    assert!(!change(state, "ID=srv-1", "ID=srv-2"), "the lock holds");
    assert!(
        change(state, "NAMEa", "NAME=b"),
        "the repair keeps the locks"
    );
    assert!(state.save(), "valid again, it is saved");
}

#[test]
fn a_refusal_takes_back_only_the_typing_it_is_about() {
    // A word typed at the end of a free line, then `\` joining the locked
    // line below to it: the `\` is refused, and the word, which the
    // refusal does not need, stays.
    let (_owner, state, _) = mounted();
    let end = TEXT.find("NAME=a").unwrap() + "NAME=a".len();
    for (offset, typed) in ["b", "c", "\\"].iter().enumerate() {
        let before = state.doc().text.get_untracked();
        let at = end + offset;
        let after = format!("{}{typed}{}", &before[..at], &before[at..]);
        let edit = Edit::input(
            before,
            (at, at),
            after,
            (at + 1, at + 1),
            "insertText".into(),
            0.0,
        );
        assert_eq!(state.commit(edit), *typed != "\\", "{typed}");
    }
    assert_eq!(
        state.doc().text.get_untracked(),
        TEXT.replace("NAME=a", "NAME=abc"),
        "only the refused input is undone"
    );
}

/// A placeholder given as a field's whole value that gets text beside it
/// (typed while the text did not parse) is a problem the host hears and
/// the status shows, though the language cannot say where it is written:
/// the save waits for it (it used to wait with no problem shown).
#[test]
fn a_glued_placeholder_is_a_problem_without_spans() {
    let token = "«secret 3f2a91c0»";
    let text = format!("KEY={token}\nNAME=a\n");
    let mut editor = TestEditor::new(Language::new(&ENV), &text);
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{8}»"));
    let (_owner, state) = editor.mount();
    assert!(change(state, "NAME=a", "NAMEa"), "breaking a line is free");
    assert!(
        change(state, token, &format!("{token}Z")),
        "typed while broken"
    );
    assert!(change(state, "NAMEa", "NAME=a"), "the repair elsewhere");
    assert!(!state.save(), "the save waits");
    let found = problems(state, Language::new(&ENV), None);
    assert!(
        found
            .iter()
            .any(|problem| problem.contains("/KEY: a secret placeholder stands for the whole")),
        "{found:?}"
    );
}
