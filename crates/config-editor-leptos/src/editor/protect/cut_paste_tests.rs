//! P1 and the README's placeholder rules: one secret's placeholder cut and
//! pasted over another secret's placeholder lands as the clipboard holds
//! it, a whole token that is plain text there and flagged, as a copy and
//! paste does; never a piece of it, in every language and Edit-as view.

#![allow(clippy::string_slice, reason = "the tests cut their own texts")]

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::{Change, Edit};
use super::super::save_matrix::{ALL, report};
use super::super::schema::build_schema;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{problems, utf16};

const KEY: &str = "«secret 3f2a»";
const PIN: &str = "«secret 0b1c»";
const SCHEMA: &str = r#"{"type": "object", "properties": {"tls": {"type": "object",
  "properties": {"cert": {"type": "string"},
  "key": {"type": "string", "writeOnly": true},
  "pin": {"type": "string", "writeOnly": true}}}}}"#;

/// The host's text in `language`.
fn host_text(language: Language) -> &'static str {
    let texts = [
        (
            Language::JSON,
            "{\n  \"tls\": {\n    \"cert\": \"/etc/edge.pem\",\n    \"key\": \"«secret 3f2a»\",\n    \"pin\": \"«secret 0b1c»\"\n  }\n}\n",
        ),
        (
            Language::TOML,
            "[tls]\ncert = \"/etc/edge.pem\"\nkey = \"«secret 3f2a»\"\npin = \"«secret 0b1c»\"\n",
        ),
        (
            Language::HCL,
            "tls {\n  cert = \"/etc/edge.pem\"\n  key = \"«secret 3f2a»\"\n  pin = \"«secret 0b1c»\"\n}\n",
        ),
        (
            Language::XML,
            "<tls>\n  <cert>/etc/edge.pem</cert>\n  <key>«secret 3f2a»</key>\n  <pin>«secret 0b1c»</pin>\n</tls>\n",
        ),
    ];
    texts
        .into_iter()
        .find(|(known, _)| *known == language)
        .map_or(
            "tls:\n  cert: /etc/edge.pem\n  key: «secret 3f2a»\n  pin: «secret 0b1c»\n",
            |(_, text)| text,
        )
}

fn mount(host: Language, view: Language) -> (Owner, EditorState) {
    let mut editor = TestEditor::new(host, host_text(host));
    if view != host {
        editor.edit_as = Some(view);
    }
    editor.schema = Some(std::sync::Arc::new(build_schema(SCHEMA).unwrap()));
    editor.secrets = Some(super::super::secret_options::SecretOptions::default());
    editor.mount()
}

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// Cuts bytes `range` as the editor's own cut of a selection snapped to a
/// placeholder does: one command removing it.
fn cut(state: EditorState, range: std::ops::Range<usize>) {
    let text = shown(state);
    let (start, end) = (utf16(&text, range.start), utf16(&text, range.end));
    let change = Change::replace_units(&text, start, end, "");
    assert!(
        state.commit(Edit::command_units(text, change, (start, end))),
        "the cut is taken"
    );
    state.recheck_tokens();
}

/// Pastes `with` over bytes `range` as the browser's paste does.
fn paste(state: EditorState, range: std::ops::Range<usize>, with: &str) {
    let before = shown(state);
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let (start, end) = (utf16(&before, range.start), utf16(&before, range.end));
    let caret = start + with.encode_utf16().count();
    state.commit(Edit::input_units(
        before,
        (start, end),
        after,
        (caret, caret),
        "insertFromPaste".into(),
        0.0,
    ));
    state.recheck_tokens();
}

#[test]
fn a_placeholder_cut_and_pasted_over_another_is_the_whole_clipboard_text() {
    let mut failures = Vec::new();
    let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
    for host in ALL {
        for view in ALL {
            for copied in [false, true] {
                let label = format!("{host:?} as {view:?}, copied: {copied}");
                let (_owner, state) = mount(host, view);
                let text = shown(state);
                let Some(pin) = text.find(PIN) else {
                    failures.push(format!("{label}: no pin in {text:?}"));
                    continue;
                };
                if !copied {
                    cut(state, pin..pin + PIN.len());
                }
                let now = shown(state);
                let key = now.find(KEY).expect("the key's placeholder");
                paste(state, key..key + KEY.len(), PIN);
                let after = shown(state);
                let expected = format!("{}{PIN}{}", &now[..key], &now[key + KEY.len()..]);
                if after != expected {
                    failures.push(format!("{label}: left {after:?}, not {expected:?}"));
                    continue;
                }
                let found = problems(state, view, Some(&schema));
                if !found.iter().any(|problem| problem.contains("/tls/key")) {
                    failures.push(format!("{label}: not flagged: {found:?}"));
                }
            }
        }
    }
    report(failures);
}
