//! A field's parent made one string over its lines by an edit lines away
//! from the field (a YAML item's `- ` typed into `-=`, making the list a
//! plain scalar running on; ` |` or ` >` typed after the parent's key): the
//! field is gone and its value is now text inside the parent's string. A
//! placeholder there would reach the host glued to that text (P1), and a
//! read-only value would reach it changed (P1, the lock holding: P5's
//! "a lock holds" in practices). Each such edit is refused or held, the
//! save waiting, in a YAML view of every host language, for a secret, a
//! read-only path and the schema's `readOnly`, in a list (first, middle or
//! last item) and a map, whether the text before the edit parsed or not.
//!
//! The holder of a placeholder was looked for only at its field, its
//! copies and inside it, and among the lines around the edit, so a parent
//! swallowing it from far away was let through to the host.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;
use serde_json::Value;

use super::super::Language;
use super::super::edit::Edit;
use super::super::host_editing::SaveRequest;
use super::super::paths::FieldPath;
use super::super::schema::build_schema;
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::guarantee_tests::utf16;

/// A placeholder of the default pattern.
const TOKEN: &str = "«secret 0b1c»";
/// A read-only value.
const LOCKED: &str = "hunter2";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Lock {
    Secret,
    Path,
    Schema,
}

/// A YAML text with the protected value (`{v}`) under its parent, the
/// field's pointer, and the lines typed under the parent's key first (to
/// put the value lines below it).
struct Shape {
    label: &'static str,
    text: &'static str,
    pointer: &'static str,
    list: bool,
    above: &'static str,
}

const SHAPES: &[Shape] = &[
    Shape {
        label: "list, first item",
        text: "keys:\n  - {v}\n  - o\nport: 1\n",
        pointer: "/keys/0",
        list: true,
        // The host's first item (`/keys/0`, which a parse also reads at
        // `/keys`) moved four lines down on screen.
        above: "\n  - a1\n  - a2\n  - a3\n  - a4",
    },
    Shape {
        label: "list, last item",
        text: "keys:\n  - a1\n  - a2\n  - a3\n  - a4\n  - o\n  - {v}\nport: 1\n",
        pointer: "/keys/5",
        list: true,
        above: "",
    },
    Shape {
        label: "list, middle item",
        text: "keys:\n  - a1\n  - a2\n  - a3\n  - a4\n  - {v}\n  - o\nport: 1\n",
        pointer: "/keys/4",
        list: true,
        above: "",
    },
    Shape {
        label: "map",
        text: "db:\n  a1: x\n  a2: x\n  a3: x\n  a4: x\n  z: o\n  password: {v}\nport: 1\n",
        pointer: "/db/password",
        list: false,
        above: "",
    },
];

/// The far edits: what is inserted where (after the first match of the
/// text), for the shapes they apply to.
fn far_edits(shape: &Shape) -> Vec<(&'static str, &'static str)> {
    let parent = if shape.list { "keys:" } else { "db:" };
    let mut edits = vec![(parent, " |"), (parent, " >")];
    if shape.list {
        edits.push(("keys:\n  -", "="));
    }
    edits
}

/// A schema marking `pointer` (a number is a list's items).
fn schema(pointer: &str, lock: Lock) -> String {
    let mut schema = if lock == Lock::Secret {
        r#"{"type": "string", "x-secret": true}"#.to_string()
    } else {
        r#"{"type": "string", "readOnly": true}"#.to_string()
    };
    for key in pointer
        .split('/')
        .skip(1)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        schema = if key.bytes().all(|b| b.is_ascii_digit()) {
            format!(r#"{{"type": "array", "items": {schema}}}"#)
        } else {
            format!(r#"{{"type": "object", "properties": {{"{key}": {schema}}}}}"#)
        };
    }
    schema
}

struct Mounted {
    _owner: Owner,
    state: EditorState,
    heard: RwSignal<Vec<String>>,
}

/// A YAML view of `yaml` (written in `host`, or YAML itself), `pointer`
/// protected by `lock`; `None` when `host` cannot write it.
fn mount(yaml: &str, host: Language, pointer: &str, lock: Lock) -> Option<Mounted> {
    let value = protected(lock);
    let host_text = if host == Language::YAML {
        yaml.to_string()
    } else {
        Language::YAML.convert(yaml, host).ok()?.text
    };
    if host_text.matches(value).count() != 1 {
        return None;
    }
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(host, &host_text);
    if host != Language::YAML {
        editor.edit_as = Some(Language::YAML);
    }
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    editor.on_save = Some(Callback::new(move |request: SaveRequest| {
        heard.update(|heard| heard.push(request.text));
    }));
    match lock {
        Lock::Secret => {
            editor.secrets = Some(SecretOptions::default());
            editor.schema = Some(std::sync::Arc::new(
                build_schema(&schema(pointer, lock)).ok()?,
            ));
        }
        Lock::Path => editor.read_only_paths = vec![FieldPath::from(pointer)],
        Lock::Schema => {
            editor.schema = Some(std::sync::Arc::new(
                build_schema(&schema(pointer, lock)).ok()?,
            ));
        }
    }
    let (owner, state) = editor.mount();
    Some(Mounted {
        _owner: owner,
        state,
        heard,
    })
}

fn protected(lock: Lock) -> &'static str {
    if lock == Lock::Secret { TOKEN } else { LOCKED }
}

/// Inserts `with` at byte `at` of the text on screen, as one input.
fn input(state: EditorState, at: usize, with: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..at], &before[at..]);
    let caret = utf16(&before, at);
    let end = caret + with.encode_utf16().count();
    state.commit(Edit::input_units(
        before,
        (caret, caret),
        after,
        (end, end),
        "insertText".into(),
        0.0,
    ))
}

/// Whether some string of `value` holds `needle` with other text.
fn beside(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text.contains(needle) && text.trim() != needle,
        Value::Array(items) => items.iter().any(|item| beside(item, needle)),
        Value::Object(map) => map.values().any(|item| beside(item, needle)),
        _ => false,
    }
}

/// What is wrong with `text` (in `language`): the protected value read
/// with other text, or gone.
fn wrong(language: Language, text: &str, value: &str) -> Option<String> {
    let document = language.parse(text).ok()?;
    if beside(&document, value) {
        return Some(format!("{value:?} read with other text"));
    }
    (!text.contains(value)).then(|| format!("{value:?} gone"))
}

#[test]
fn a_parent_made_one_string_far_from_a_protected_value_never_reaches_the_host() {
    let hosts = [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ];
    let mut failures = Vec::new();
    let mut runs = 0;
    for host in hosts {
        for lock in [Lock::Secret, Lock::Path, Lock::Schema] {
            for shape in SHAPES {
                // The schema's `readOnly` on a list's items locks the line
                // the `{` would break.
                let broken_too = !(shape.list && lock == Lock::Schema);
                for broken in [false, true]
                    .into_iter()
                    .filter(|broken| !broken || broken_too)
                {
                    for (after, inserted) in far_edits(shape) {
                        let value = protected(lock);
                        let yaml = shape.text.replace("{v}", value);
                        let Some(mounted) = mount(&yaml, host, shape.pointer, lock) else {
                            continue;
                        };
                        runs += 1;
                        let state = mounted.state;
                        let case = format!(
                            "{host} host, {lock:?}, {}, {}, {inserted:?} after {after:?}",
                            shape.label,
                            if broken {
                                "broken before"
                            } else {
                                "parsing before"
                            },
                        );
                        if !shape.above.is_empty() {
                            let shown = state.doc().text.get_untracked();
                            let at = shown.find("keys:").unwrap() + "keys:".len();
                            if !input(state, at, shape.above) {
                                failures.push(format!("{case}: the items above refused"));
                                continue;
                            }
                        }
                        // The text broken first, lines from the value: `{`
                        // opening a flow map never closed.
                        if broken {
                            let shown = state.doc().text.get_untracked();
                            let at = shown.find(" o\n").unwrap() + 1;
                            if !input(state, at, "{") {
                                failures.push(format!("{case}: the `{{` refused"));
                                continue;
                            }
                        }
                        let shown = state.doc().text.get_untracked();
                        let at = shown.find(after).unwrap() + after.len();
                        input(state, at, inserted);
                        let screen = state.doc().text.get_untracked();
                        let held = state.changes_held() && state.save_waits().is_some();
                        if let Some(problem) = wrong(Language::YAML, &screen, value)
                            && !held
                        {
                            failures.push(format!("{case}: on screen {problem} in {screen:?}"));
                        }
                        convert_now(state, screen, Language::YAML);
                        state.save();
                        for text in mounted.heard.get_untracked() {
                            if let Some(problem) = wrong(host, &text, value) {
                                failures
                                    .push(format!("{case}: the host heard {problem} in {text:?}"));
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(runs > 100, "only {runs} runs");
    assert!(
        failures.is_empty(),
        "{} of {runs} failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
