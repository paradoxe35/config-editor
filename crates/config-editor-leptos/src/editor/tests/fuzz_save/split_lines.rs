//! P2: a value the user left alone keeps its host text when the user
//! splits a line around its item, at any column, in any view: an XML
//! view takes the line break and indentation a split puts around an
//! element's text into its text, which a schema wanting a number there
//! reads as that number all the same, so the value is still the one the
//! editor wrote. The hosts and schemas of `joined_lines`: the second
//! item's value the string `3` in a form of the host's.

use leptos::prelude::*;

use super::ALL;
use super::joined_lines::{HOSTS, SCHEMAS, schema};
use super::ops::{self, Op};
use crate::editor::bridge::Bridge;
use crate::editor::languages::Language;
use crate::editor::schema::Schema;
use crate::editor::test_state::TestEditor;

/// The value left alone, in each host.
const POINTER: &str = "/spec/env/1/value";

/// The host's text once Enter is pressed at `column` of `line` of the
/// view of `text` in `view`, and a pause saves.
fn saved_after_enter(
    (host, text): (Language, &str),
    view: Language,
    value: &str,
    (line, column): (usize, usize),
) -> String {
    let schema = Schema::compile(&schema(value)).expect("the schema compiles");
    let held = std::sync::Arc::new(std::sync::Mutex::new(text.to_string()));
    let mut editor = TestEditor::new(host, text);
    editor.schema = Some(schema.bundle.clone());
    let told = held.clone();
    editor.on_change = Some(Callback::new(move |text: String| {
        *told.lock().unwrap() = text;
    }));
    editor.edit_as = Some(view);
    let (_owner, state) = editor.mount();
    let (mut clipboard, mut time) = (None, 0.0);
    ops::apply(
        state,
        host,
        &Op::Enter { line, column },
        &mut clipboard,
        &mut time,
    );
    ops::pause(state);
    held.lock().unwrap().clone()
}

#[test]
fn a_value_left_alone_keeps_its_host_text_when_a_line_of_its_item_is_split_in_any_view() {
    for (host, text) in HOSTS {
        let started = host.parse(text).expect("the host reads");
        assert_eq!(started.pointer(POINTER), Some(&serde_json::json!("3")));
        // The host's line holding the value, kept as written.
        let written = text
            .lines()
            .rfind(|line| line.contains('3'))
            .expect("the value's line");
        for value in SCHEMAS {
            let bundle = Schema::compile(&schema(value)).expect("compiles").bundle;
            for view in ALL.into_iter().filter(|view| view != host) {
                let Some(bridge) = Bridge::new(*host, view, Some(bundle.clone())) else {
                    continue;
                };
                let Ok(shown) = bridge.to_edit(text) else {
                    continue;
                };
                let shown = shown.text;
                let lines: Vec<&str> = shown.lines().collect();
                // The second item's value's line, and the one before it.
                let Some(at) = lines.iter().rposition(|line| line.contains('3')) else {
                    continue;
                };
                let first = at.saturating_sub(1);
                for (line, written_line) in lines.iter().enumerate().take(at + 1).skip(first) {
                    for column in 0..=written_line.chars().count() {
                        let saved = saved_after_enter((*host, text), view, value, (line, column));
                        let read = host.parse(&saved).ok();
                        let now = read.as_ref().and_then(|read| read.pointer(POINTER));
                        let context = || {
                            format!(
                                "{host} host, {view} view, value {value}, Enter at {line}:{column} \
                                 of\n{shown}\nsaved:\n{saved}"
                            )
                        };
                        // Text the user put in the value is theirs; the
                        // value as it was keeps its host text.
                        assert!(
                            now.is_none_or(serde_json::Value::is_string),
                            "type changed: {}",
                            context()
                        );
                        if now == Some(&serde_json::json!("3")) {
                            assert!(saved.contains(written), "form lost: {}", context());
                        }
                    }
                }
            }
        }
    }
}
