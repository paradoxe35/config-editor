//! A placeholder the host gave as a field's whole value stays the whole
//! value whatever way it gets back into its field, and its field stays a
//! string:
//! - made plain for a moment (its key renamed, its line commented out or
//!   moved out of its block), written beside, then made real again (the
//!   key renamed back, the line uncommented or moved back): the edit
//!   making it real with text beside it is refused, in every language;
//! - its field made a list or map holding it (an XML element given a child
//!   or an attribute, a sibling moved in, the token put in brackets) is
//!   refused as text beside it is.
//!
//! Never does the host hear its field holding the token with anything
//! beside it.

use leptos::prelude::*;
use serde_json::Value;

use super::super::Language;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run};
use super::super::save_matrix::{ALL, report};
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::glued_tests::{paste, type_at};
use super::guarantee_tests::utf16;

const TOKEN: &str = "«secret 3f2a»";

/// The sample in `language`: the token is `/tls/key`'s whole value
/// (`/r/tls/key` in XML), between two siblings.
fn sample(language: Language) -> String {
    match language.label() {
        "YAML" => format!("id: srv-42\ntls:\n  cert: x\n  key: {TOKEN}\n  pin: 5\n"),
        "JSON" => format!(
            "{{\n  \"id\": \"srv-42\",\n  \"tls\": {{\n    \"cert\": \"x\",\n    \"key\": \"{TOKEN}\",\n    \"pin\": 5\n  }}\n}}\n"
        ),
        "TOML" => format!("id = \"srv-42\"\n\n[tls]\ncert = \"x\"\nkey = \"{TOKEN}\"\npin = 5\n"),
        "HCL" => format!(
            "id = \"srv-42\"\n\ntls {{\n  cert = \"x\"\n  key = \"{TOKEN}\"\n  pin = 5\n}}\n"
        ),
        _ => format!(
            "<r>\n<id>srv-42</id>\n<tls>\n  <cert>x</cert>\n  <key>{TOKEN}</key>\n  <pin>5</pin>\n</tls>\n</r>\n"
        ),
    }
}

/// The token's field in `language`'s sample.
fn field(language: Language) -> &'static str {
    if language.label() == "XML" {
        "/r/tls/key"
    } else {
        "/tls/key"
    }
}

/// A live editor on `text` in `language`, with what `on_change` heard.
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

/// The typing pause.
fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
}

/// Whether `field` of `text` (in `language`) holds the token with anything
/// beside it: text, or a list or map made of it.
fn glued(language: Language, text: &str, field: &str) -> bool {
    let Ok(document) = language.parse(text) else {
        return false;
    };
    match document.pointer(field) {
        Some(Value::String(value)) => value.contains(TOKEN) && value.trim() != TOKEN,
        Some(value @ (Value::Array(_) | Value::Object(_))) => value.to_string().contains(TOKEN),
        _ => false,
    }
}

/// Replaces the first `from` after byte `after` of the text on screen with
/// `to`.
fn replace(state: EditorState, from: &str, to: &str, after: usize) {
    let text = state.doc().text.get_untracked();
    if let Some(at) = text[after..].find(from) {
        let at = after + at;
        paste(state, at..at + from.len(), to);
    }
}

/// Runs a line command `times` times with the caret on the token's line.
fn move_line(state: EditorState, down: bool, times: usize) {
    for _ in 0..times {
        let text = state.doc().text.get_untracked();
        let Some(at) = text.find(TOKEN) else {
            return;
        };
        let caret = utf16(&text, at);
        run(
            state,
            Command::Lines(LineCommand::Move { down }),
            (caret, caret),
            false,
        );
    }
}

/// Byte where the token's line starts in the text on screen.
fn token_line(state: EditorState) -> Option<(usize, usize)> {
    let text = state.doc().text.get_untracked();
    let at = text.find(TOKEN)?;
    let start = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let end = text[at..].find('\n').map_or(text.len(), |found| at + found);
    Some((start, end))
}

/// Comments the token's line out (`back`: in again), as `language` writes a
/// comment.
fn comment(state: EditorState, language: Language, back: bool) {
    let Some((start, end)) = token_line(state) else {
        return;
    };
    let text = state.doc().text.get_untracked();
    let line = &text[start..end];
    let indent = &line[..line.len() - line.trim_start().len()];
    let code = line.trim_start();
    let new = match (language.label(), back) {
        ("XML", false) => format!("{indent}<!-- {code} -->"),
        ("XML", true) => format!(
            "{indent}{}",
            code.trim_start_matches("<!-- ").trim_end_matches(" -->")
        ),
        (_, false) => format!("{indent}# {code}"),
        (_, true) => format!("{indent}{}", code.trim_start_matches("# ")),
    };
    paste(state, start..end, &new);
}

/// Renames the token's key `key` to `key2` (`back`: to `key` again), each
/// place the language writes it.
fn rename(state: EditorState, language: Language, back: bool) {
    let Some((start, _)) = token_line(state) else {
        return;
    };
    let (from, to) = if back {
        ("key2", "key")
    } else {
        ("key", "key2")
    };
    if language.label() == "XML" {
        // The end tag first going back: the text reads again only once
        // both tags are renamed.
        let tags = [format!("</{from}>"), format!("<{from}>")];
        let order: Vec<&String> = if back {
            tags.iter().collect()
        } else {
            tags.iter().rev().collect()
        };
        for tag in order {
            let renamed = tag.replace(from, to);
            replace(state, tag, &renamed, start);
        }
    } else {
        replace(state, from, to, start);
    }
}

/// A way to make the token plain (`false`) and real again (`true`).
type Route = Box<dyn Fn(EditorState, bool)>;

#[test]
fn a_placeholder_made_real_again_with_text_beside_it_is_refused() {
    let mut failures = Vec::new();
    for language in ALL {
        let ups = match language.label() {
            "TOML" | "HCL" => 3,
            _ => 2,
        };
        let routes: [(&str, Route); 3] = [
            (
                "its key renamed",
                Box::new(move |state, back| rename(state, language, back)),
            ),
            (
                "its line commented out",
                Box::new(move |state, back| comment(state, language, back)),
            ),
            (
                "its line moved out of its block",
                Box::new(move |state, back| move_line(state, back, ups)),
            ),
        ];
        for (route, make) in &routes {
            if language.label() == "JSON" && route.contains("commented") {
                continue;
            }
            let case = format!("{language}, {route}");
            let (_owner, state, heard) = mount(language, &sample(language));
            make(state, false);
            pause(state);
            let plain = state.doc().text.get_untracked();
            let Some(at) = plain.find(TOKEN) else {
                failures.push(format!("{case}: no token in {plain:?}"));
                continue;
            };
            type_at(state, at + TOKEN.len(), "MY");
            pause(state);
            if !state
                .doc()
                .text
                .get_untracked()
                .contains(&format!("{TOKEN}MY"))
            {
                failures.push(format!(
                    "{case}: typing beside the plain token refused: {:?}",
                    state.doc().text.get_untracked()
                ));
                continue;
            }
            make(state, true);
            pause(state);
            let now = state.doc().text.get_untracked();
            if glued(language, &now, field(language)) {
                failures.push(format!("{case}: made real again glued: {now:?}"));
            }
            if let Some(text) = heard
                .get_untracked()
                .iter()
                .find(|text| glued(language, text, field(language)))
            {
                failures.push(format!("{case}: the host heard {text:?}"));
            }
        }
    }
    report(failures);
}

/// What wraps the token's written value in brackets in `language` (its
/// quotes kept), with another item when `more`.
fn bracketed(language: Language, more: bool) -> (String, String) {
    let quoted = match language.label() {
        "YAML" | "XML" => TOKEN.to_string(),
        _ => format!("\"{TOKEN}\""),
    };
    let other = match language.label() {
        "YAML" | "XML" => "x",
        _ => "\"x\"",
    };
    let wrapped = if more {
        format!("[{quoted}, {other}]")
    } else {
        format!("[{quoted}]")
    };
    (quoted, wrapped)
}

#[test]
fn a_placeholder_field_made_a_list_or_map_is_refused() {
    let mut failures = Vec::new();
    // The token selected and put in brackets (auto-surround), or a list
    // holding it typed over it, in every language.
    for language in ALL {
        for more in [false, true] {
            let case = format!("{language}, put in brackets (more: {more})");
            let (_owner, state, heard) = mount(language, &sample(language));
            let (quoted, wrapped) = bracketed(language, more);
            replace(state, &quoted, &wrapped, 0);
            pause(state);
            let now = state.doc().text.get_untracked();
            if glued(language, &now, field(language)) {
                failures.push(format!("{case}: let through: {now:?}"));
            }
            if let Some(text) = heard
                .get_untracked()
                .iter()
                .find(|text| glued(language, text, field(language)))
            {
                failures.push(format!("{case}: the host heard {text:?}"));
            }
        }
    }
    // An XML element holding the token given a child or an attribute.
    let one_line = sample(Language::XML);
    let multi = format!("<tls>\n  <key>\n    {TOKEN}\n  </key>\n  <pin>5</pin>\n</tls>\n");
    type Step = Box<dyn Fn(EditorState)>;
    let steps: [(&str, &str, &str, Step); 6] = [
        (
            "a sibling moved in",
            &multi,
            "/tls/key",
            Box::new(|state| {
                let text = state.doc().text.get_untracked();
                let caret = utf16(&text, text.find("<pin>").unwrap_or_default());
                run(
                    state,
                    Command::Lines(LineCommand::Move { down: false }),
                    (caret, caret),
                    false,
                );
            }),
        ),
        (
            "its end tag moved past a sibling (Replace All)",
            &multi,
            "/tls/key",
            Box::new(|state| replace(state, "</key>\n  <pin>5</pin>", "<pin>5</pin>\n  </key>", 0)),
        ),
        (
            "a child typed after the token",
            &one_line,
            "/r/tls/key",
            Box::new(|state| {
                let text = state.doc().text.get_untracked();
                let at = text.find(TOKEN).unwrap_or_default() + TOKEN.len();
                type_at(state, at, "<z/>");
            }),
        ),
        (
            "a child typed after the token over lines",
            &multi,
            "/tls/key",
            Box::new(|state| {
                let text = state.doc().text.get_untracked();
                let at = text.find(TOKEN).unwrap_or_default() + TOKEN.len();
                type_at(state, at, "<z/>");
            }),
        ),
        (
            "an attribute typed on its tag",
            &one_line,
            "/r/tls/key",
            Box::new(|state| {
                let text = state.doc().text.get_untracked();
                let at = text.find("<key>").unwrap_or_default() + "<key".len();
                type_at(state, at, " a=\"1\"");
            }),
        ),
        (
            "xsi:nil typed on its tag",
            &one_line,
            "/r/tls/key",
            Box::new(|state| {
                let text = state.doc().text.get_untracked();
                let at = text.find("<key>").unwrap_or_default() + "<key".len();
                type_at(state, at, " xsi:nil=\"true\"");
            }),
        ),
    ];
    for (step, text, field, act) in &steps {
        let case = format!("XML, {step}");
        let (_owner, state, heard) = mount(Language::XML, text);
        act(state);
        pause(state);
        let now = state.doc().text.get_untracked();
        if glued(Language::XML, &now, field) {
            failures.push(format!("{case}: let through: {now:?}"));
        }
        if let Some(text) = heard
            .get_untracked()
            .iter()
            .find(|text| glued(Language::XML, text, field))
        {
            failures.push(format!("{case}: the host heard {text:?}"));
        }
    }
    report(failures);
}
