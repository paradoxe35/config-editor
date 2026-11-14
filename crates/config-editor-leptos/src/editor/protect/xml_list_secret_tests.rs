//! An XML host's list of secrets: the schema says a field is a list of
//! secret strings, and XML writes a list by repeating its element, so one
//! element reads as a lone value. The placeholder guard reads the list as
//! the views do (by the schema): a token is a whole item, never glued, in
//! the host's own view and in every Edit-as view, with one item or two;
//! items are added, and a token replaced, freely.

use leptos::prelude::*;

use super::super::Language;
use super::super::diagnostics::build_schema;
use super::super::edit::Edit;
use super::super::save_matrix::{ALL, report};
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{problems, utf16};

const FIRST: &str = "«secret 0b1c»";
const SECOND: &str = "«secret 3f2a»";
const SCHEMA: &str = r#"{"type": "object", "properties": {"config": {"type": "object",
  "properties": {"secret": {"type": "array", "items": {"type": "string", "x-secret": true}},
  "name": {"type": "string"}}}}}"#;

fn host_text(items: usize) -> String {
    let written: String = [FIRST, SECOND][..items]
        .iter()
        .map(|token| format!("  <secret>{token}</secret>\n"))
        .collect();
    format!("<config>\n{written}  <name>n</name>\n</config>\n")
}

fn mount(view: Language, items: usize) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(Language::XML, &host_text(items));
    if view != Language::XML {
        editor.edit_as = Some(view);
    }
    editor.schema = Some(std::sync::Arc::new(build_schema(SCHEMA).unwrap()));
    editor.secrets = Some(super::super::secret_options::SecretOptions::default());
    editor.mount()
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// Replaces bytes `range` of the text on screen with `with`, a key at a
/// time when `typed`; whether every key was taken.
pub(super) fn write(
    state: EditorState,
    range: std::ops::Range<usize>,
    with: &str,
    typed: bool,
) -> bool {
    let pieces: Vec<String> = if typed {
        with.chars().map(String::from).collect()
    } else {
        vec![with.to_string()]
    };
    let mut range = range;
    for piece in pieces {
        let before = shown(state);
        let after = format!("{}{piece}{}", &before[..range.start], &before[range.end..]);
        let (start, end) = (utf16(&before, range.start), utf16(&before, range.end));
        let caret = start + piece.encode_utf16().count();
        let kind = if piece == "\n" {
            "insertLineBreak"
        } else {
            "insertText"
        };
        let edit = Edit::input(
            before,
            (start, end),
            after,
            (caret, caret),
            kind.into(),
            0.0,
        );
        if !state.commit(edit) {
            return false;
        }
        range = range.start + piece.len()..range.start + piece.len();
    }
    true
}

fn pause(state: EditorState) {
    let language = state.sync().edit_language.get_untracked();
    convert_now(state, shown(state), language);
}

/// Where a new item goes in `view` after the last token, and its text.
fn new_item(view: Language, text: &str) -> (usize, String) {
    let last = text.rfind('»').expect("a token") + '»'.len_utf8();
    let line_start = text[..last].rfind('\n').map_or(0, |at| at + 1);
    let line = &text[line_start..];
    let indent = &line[..line.len() - line.trim_start().len()];
    let line_end = last + text[last..].find('\n').unwrap_or(text.len() - last);
    if view == Language::XML {
        (line_end, format!("\n{indent}<secret>new</secret>"))
    } else if view == Language::YAML {
        (line_end, format!("\n{indent}- new"))
    } else {
        // After the closing quote of the last token.
        (last + 1, ", \"new\"".to_string())
    }
}

fn flagged(state: EditorState, view: Language) -> Vec<String> {
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    problems(state, view, Some(&schema))
}

#[test]
fn a_one_item_xml_list_of_secrets_reads_as_a_list_in_every_view() {
    let mut failures = Vec::new();
    for view in ALL {
        for items in [1, 2] {
            let case = format!("XML host as {view}, {items} item(s)");
            // Mount: nothing flagged.
            let (_owner, state) = mount(view, items);
            let found = flagged(state, view);
            if !found.is_empty() {
                failures.push(format!(
                    "{case}: flagged on mount: {found:?}\n{}",
                    shown(state)
                ));
            }
            // Add an item, typed and pasted.
            for typed in [true, false] {
                let (_owner, state) = mount(view, items);
                let (at, item) = new_item(view, &shown(state));
                if !write(state, at..at, &item, typed) {
                    failures.push(format!(
                        "{case}: adding {item:?} (typed: {typed}) refused at\n{}",
                        shown(state)
                    ));
                    continue;
                }
                pause(state);
                let found = flagged(state, view);
                if !found.is_empty() {
                    failures.push(format!("{case}: flagged after adding: {found:?}"));
                }
                let host = if view == Language::XML {
                    shown(state)
                } else {
                    state.sync().host_text.get_value()
                };
                if !host.contains("<secret>new</secret>") {
                    failures.push(format!("{case}: the host did not hear the new item"));
                }
            }
            // Replace a token with a new value.
            let (_owner, state) = mount(view, items);
            let text = shown(state);
            let at = text.find(FIRST).unwrap();
            if !write(state, at..at + FIRST.len(), "other", false) {
                failures.push(format!("{case}: replacing the token refused"));
            }
        }
    }
    report(failures);
}

/// A field holding one secret, as a string or as a list of them.
fn one_secret_schema(list: bool) -> String {
    let key = if list {
        r#"{"type": "array", "items": {"type": "string", "writeOnly": true}}"#
    } else {
        r#"{"type": "string", "writeOnly": true}"#
    };
    format!(
        r#"{{"type": "object", "properties": {{"tls": {{"type": "object", "properties": {{
          "cert": {{"type": "string"}}, "key": {key}, "pin": {{"type": "string", "writeOnly": true}}}}}}}}}}"#
    )
}

const TLS: &str = "<tls>\n  <cert>/etc/a.pem</cert>\n  <key>«secret 0b1c»</key>\n</tls>\n";

fn mount_tls(schema: Option<&str>) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(Language::XML, TLS);
    editor.schema = schema.map(|schema| std::sync::Arc::new(build_schema(schema).unwrap()));
    editor.secrets = Some(super::super::secret_options::SecretOptions::default());
    editor.mount()
}

/// Replaces the `nth` `needle` of the text on screen (from `skip` bytes
/// into it) with `with`, a key at a time, as typing over a selection does;
/// whether every key was taken.
pub(super) fn retype(
    state: EditorState,
    (needle, skip): (&str, usize),
    nth: usize,
    with: &str,
) -> bool {
    let text = shown(state);
    let Some((at, _)) = text.match_indices(needle).nth(nth) else {
        return false;
    };
    let (at, needle) = (at + skip, &needle[skip..]);
    let mut chars = with.chars();
    let Some(first) = chars.next() else {
        return write(state, at..at + needle.len(), "", false);
    };
    write(state, at..at + needle.len(), &first.to_string(), false)
        && write(
            state,
            at + first.len_utf8()..at + first.len_utf8(),
            chars.as_str(),
            true,
        )
}

fn end_of_key(state: EditorState) -> usize {
    shown(state).find("</key>").unwrap() + "</key>".len()
}

#[test]
fn a_new_element_beside_a_secret_element_is_a_new_field_or_item() {
    let mut failures = Vec::new();
    let line = format!("\n  <key>{FIRST}</key>");
    for (case, schema) in [
        ("no schema", None),
        ("a string", Some(one_secret_schema(false))),
        ("a list", Some(one_secret_schema(true))),
    ] {
        let schema = schema.as_deref();
        // A second element of the secret's name typed after it, then the
        // original token replaced.
        let (_owner, state) = mount_tls(schema);
        let at = end_of_key(state);
        if !write(state, at..at, "\n  <key>abc</key>", true) {
            failures.push(format!(
                "{case}: typing a second <key> refused at\n{}",
                shown(state)
            ));
        }
        if !retype(state, (FIRST, 0), 0, "fresh") {
            failures.push(format!(
                "{case}: replacing the token refused at\n{}",
                shown(state)
            ));
        }
        // The secret's line copied down (Shift+Alt+Down or a paste), then
        // the copy's tags renamed and its value replaced.
        let (_owner, state) = mount_tls(schema);
        let at = end_of_key(state);
        if !write(state, at..at, &line, false) {
            failures.push(format!("{case}: copying the line refused"));
            continue;
        }
        for (step, needle, with) in [
            ("open tag", ("<key", 1), "pin"),
            ("close tag", ("</key", 2), "pin"),
        ] {
            if !retype(state, needle, 1, with) {
                failures.push(format!(
                    "{case}: renaming the copy's {step} refused at\n{}",
                    shown(state)
                ));
            }
        }
        if !retype(state, (FIRST, 0), 1, "pinvalue") {
            failures.push(format!(
                "{case}: replacing the copy's token refused at\n{}",
                shown(state)
            ));
        }
        if !shown(state).contains("<pin>pinvalue</pin>") {
            failures.push(format!("{case}: copy not renamed:\n{}", shown(state)));
        }
    }
    report(failures);
}
