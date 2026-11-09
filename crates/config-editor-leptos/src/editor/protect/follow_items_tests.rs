//! A placeholder in a list item follows its item (a Kubernetes-style
//! `env` list): deleting or inserting an item before it, or moving it,
//! leaves it real, unflagged, and saved in its item's new place; pasted
//! into another item it is still plain text there. In every host language,
//! shown in its own language and as every other one, for two unrelated
//! patterns.

use leptos::prelude::*;
use serde_json::{Value, json};

use super::super::Language;
use super::super::edit::Edit;
use super::super::save_matrix::{ALL, report};
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{HOSTS, Tokens, problems, real_texts, utf16};

fn document(token: &str) -> Value {
    json!({"env": [
        {"name": "A", "value": "plain"},
        {"name": "TOKEN", "value": token},
        {"name": "C", "value": "c"},
    ]})
}

/// A live editor on the host's `env` list in `host`, shown as `view`, with
/// what `on_change` heard.
fn mount(
    host: Language,
    view: Language,
    tokens: &Tokens,
) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let heard = RwSignal::new(Vec::<String>::new());
    let text = host.serialize(&document(tokens.same)).unwrap();
    let mut editor = TestEditor::new(host, &text);
    if view != host {
        editor.edit_as = Some(view);
    }
    editor.secrets = Some(tokens.options());
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text))
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

/// The text on screen made to read as `change` of what it reads, by one
/// edit of the lines that differ (as a cut, paste or typing would), then
/// the typing pause.
fn edit_value(
    state: EditorState,
    view: Language,
    change: &dyn Fn(&mut Value),
) -> Result<(), String> {
    let before = state.doc().text.get_untracked();
    let mut value = view.parse(&before).map_err(|error| error.message)?;
    change(&mut value);
    let after = super::super::codec::serialize(view.behavior(), &value, Some(&before))
        .map_err(|error| error.to_string())?;
    let prefix = before
        .bytes()
        .zip(after.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    let prefix = before[..prefix].rfind('\n').map_or(0, |at| at + 1);
    let suffix = before[prefix..]
        .bytes()
        .rev()
        .zip(after[prefix..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (end, with) = (before.len() - suffix, &after[prefix..after.len() - suffix]);
    let (start, stop) = (utf16(&before, prefix), utf16(&before, end));
    let caret = start + with.encode_utf16().count();
    let text = format!("{}{with}{}", &before[..prefix], &before[end..]);
    if !state.commit(Edit::new(before, (start, stop), text, caret)) {
        return Err("refused".to_string());
    }
    state.recheck_tokens();
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
    Ok(())
}

type Change = fn(&mut Value);

fn items(value: &mut Value) -> &mut Vec<Value> {
    value["env"].as_array_mut().unwrap()
}

#[test]
fn a_placeholder_follows_its_item_when_items_shift() {
    let mut failures = Vec::new();
    let changes: [(&str, Change, usize); 4] = [
        (
            "the item before it deleted",
            |value| {
                items(value).remove(0);
            },
            0,
        ),
        (
            "an item inserted before it",
            |value| items(value).insert(0, json!({"name": "Z", "value": "z"})),
            2,
        ),
        (
            "it moved up",
            |value| {
                let item = items(value).remove(1);
                items(value).insert(0, item);
            },
            0,
        ),
        (
            "it moved down",
            |value| {
                let item = items(value).remove(1);
                items(value).push(item);
            },
            2,
        ),
    ];
    for tokens in &HOSTS {
        for host in ALL {
            for view in ALL {
                for (step, change, index) in &changes {
                    let case = format!("{host} host as {view} ({}), {step}", tokens.pattern);
                    let (_owner, state, heard) = mount(host, view, tokens);
                    if let Err(error) = edit_value(state, view, change) {
                        failures.push(format!("{case}: {error}"));
                        continue;
                    }
                    if real_texts(state) != [tokens.same] {
                        failures.push(format!("{case}: real {:?}", real_texts(state)));
                    }
                    let flagged = problems(state, view, None);
                    if !flagged.is_empty() {
                        failures.push(format!("{case}: flagged {flagged:?}"));
                    }
                    if view != host {
                        let saved = heard.get_untracked().last().cloned().unwrap_or_default();
                        let read = host.parse(&saved).ok();
                        let at = format!("/env/{index}/value");
                        if read.as_ref().and_then(|read| read.pointer(&at))
                            != Some(&json!(tokens.same))
                        {
                            failures.push(format!("{case}: host heard {saved:?}"));
                        }
                    }
                }
            }
        }
    }
    report(failures);
}

/// Copied into another item, the placeholder is that item's plain text,
/// and its own item's still real.
#[test]
fn a_placeholder_copied_into_another_item_is_plain_text_there() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for host in ALL {
            let case = format!("{host} ({})", tokens.pattern);
            let (_owner, state, _) = mount(host, host, tokens);
            let same = tokens.same;
            let copied = move |value: &mut Value| {
                items(value).remove(0);
                items(value)[1]["value"] = json!(same);
            };
            if let Err(error) = edit_value(state, host, &copied) {
                failures.push(format!("{case}: {error}"));
                continue;
            }
            if real_texts(state) != [tokens.same] {
                failures.push(format!("{case}: real {:?}", real_texts(state)));
            }
            if problems(state, host, None).len() != 1 {
                failures.push(format!("{case}: flagged {:?}", problems(state, host, None)));
            }
        }
    }
    report(failures);
}
