//! A placeholder the host gave as a field's whole value takes nothing
//! beside it: typing or pasting next to it in its field is refused (the
//! token stays whole and the host never hears it glued to other text),
//! and text that got there while the text did not parse is flagged at the
//! token, with a save waiting. In every host language, shown in
//! its own language and as every other one, for two unrelated patterns.

use leptos::prelude::*;

use super::super::Language;
use super::super::edit::Edit;
use super::super::edit_as::EditAsText;
use super::super::save_matrix::{ALL, report};
use super::super::state::EditorState;
use super::super::sync::convert_now;
use super::super::test_state::TestEditor;
use super::guarantee_tests::{HOSTS, Tokens, problems, sample, utf16};

const SAYS: &str =
    "/a: a secret placeholder stands for the whole secret, so nothing can be written beside it";

/// A live editor on the host's sample in `host`, shown as `view`, with
/// what `on_change` heard.
fn mount(
    host: Language,
    view: Language,
    tokens: &Tokens,
) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let heard = RwSignal::new(Vec::<String>::new());
    let mut editor = TestEditor::new(host, &sample(host, tokens));
    if view != host {
        editor.edit_as = Some(view);
    }
    editor.secrets = Some(tokens.options());
    editor.on_change = Some(Callback::new(move |text| {
        heard.update(|heard| heard.push(text));
    }));
    let (owner, state) = editor.mount();
    (owner, state, heard)
}

/// Types `typed` at byte `at` of the text on screen, one key at a time.
pub(super) fn type_at(state: EditorState, at: usize, typed: &str) {
    let mut at = utf16(&state.doc().text.get_untracked(), at);
    for character in typed.chars() {
        let before = state.doc().text.get_untracked();
        let byte = super::super::keyboard::utf16_to_byte_index(&before, at);
        let text = format!("{}{character}{}", &before[..byte], &before[byte..]);
        let edit = Edit::input(
            before,
            (at, at),
            text,
            (at + 1, at + 1),
            "insertText".into(),
            0.0,
        );
        if state.commit(edit) {
            at += character.len_utf16();
        }
    }
}

/// Replaces bytes `range` of the text on screen with `with` (a paste).
pub(super) fn paste(state: EditorState, range: std::ops::Range<usize>, with: &str) -> bool {
    let before = state.doc().text.get_untracked();
    let after = format!("{}{with}{}", &before[..range.start], &before[range.end..]);
    let (start, end) = (utf16(&before, range.start), utf16(&before, range.end));
    let caret = start + with.encode_utf16().count();
    state.commit(Edit::new(before, (start, end), after, caret))
}

/// The typing pause: the text on screen converted back to the host.
fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
}

/// The value `/a` reads as in `text`, in `language`.
fn field_a(language: Language, text: &str) -> Option<String> {
    language
        .parse(text)
        .ok()
        .and_then(|value| value.get("a").and_then(|a| a.as_str()).map(str::to_string))
}

/// Text that keeps each language from parsing, appended at the end.
fn breaking(language: Language) -> &'static str {
    match language.label() {
        "JSON" => "\n,",
        "XML" => "\n<z>",
        "YAML" => "\nz: [",
        _ => "\nz = [",
    }
}

#[test]
fn typing_or_pasting_beside_a_whole_placeholder_is_refused() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for host in ALL {
            for view in ALL {
                let case = format!("{host} host as {view} ({})", tokens.pattern);
                let typings: [(&str, bool, &str); 4] = [
                    ("Z after it", true, "Z"),
                    ("Z before it", false, "Z"),
                    ("a space and a word after it", true, " x"),
                    ("a word and a space before it", false, "x "),
                ];
                for (step, after, typed) in typings {
                    let (_owner, state, heard) = mount(host, view, tokens);
                    let shown = state.doc().text.get_untracked();
                    let Some(start) = shown.find(tokens.same) else {
                        failures.push(format!("{case}: no token in\n{shown}"));
                        break;
                    };
                    let at = if after {
                        start + tokens.same.len()
                    } else {
                        start
                    };
                    type_at(state, at, typed);
                    pause(state);
                    let now = state.doc().text.get_untracked();
                    if field_a(view, &now).as_deref() != Some(tokens.same) {
                        failures.push(format!("{case}, {step}: shown {now:?}"));
                    }
                    if !state
                        .protect()
                        .tip
                        .get_untracked()
                        .is_some_and(|tip| tip.message.starts_with(SAYS))
                    {
                        failures.push(format!(
                            "{case}, {step}: refusal {:?}",
                            state.protect().tip.get_untracked()
                        ));
                    }
                    if let Some(glued) = heard
                        .get_untracked()
                        .iter()
                        .find(|text| field_a(host, text).as_deref() != Some(tokens.same))
                    {
                        failures.push(format!("{case}, {step}: the host heard {glued:?}"));
                    }
                }
                // Pasted beside it: refused too.
                let (_owner, state, _) = mount(host, view, tokens);
                let shown = state.doc().text.get_untracked();
                if let Some(start) = shown.find(tokens.same) {
                    let end = start + tokens.same.len();
                    if paste(state, end..end, "pasted") {
                        failures.push(format!(
                            "{case}, a paste after it: let through: {:?}",
                            state.doc().text.get_untracked()
                        ));
                    }
                }
            }
        }
    }
    report(failures);
}

/// Replacing a placeholder whole, removing it, or writing beside a token
/// the host gave with other text in its field stays free.
#[test]
fn replacing_a_whole_placeholder_and_editing_around_it_stay_free() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for host in ALL {
            for view in ALL {
                let case = format!("{host} host as {view} ({})", tokens.pattern);
                let (_owner, state, _) = mount(host, view, tokens);
                let shown = state.doc().text.get_untracked();
                let Some(start) = shown.find(tokens.same) else {
                    failures.push(format!("{case}: no token in\n{shown}"));
                    continue;
                };
                // Over the token, with the new value.
                if !paste(state, start..start + tokens.same.len(), "new")
                    || field_a(view, &state.doc().text.get_untracked()).as_deref() != Some("new")
                {
                    failures.push(format!(
                        "{case}: replacing it refused: {:?}",
                        state.doc().text.get_untracked()
                    ));
                }
            }
        }
    }
    // A token the host wrote with other text in its field (`x«…»`) is
    // the host's to embed: text typed beside it is free.
    for tokens in &HOSTS {
        let text = sample(Language::JSON, tokens).replacen(
            &format!("\"{}\"", tokens.same),
            &format!("\"x{}\"", tokens.same),
            1,
        );
        let mut editor = TestEditor::new(Language::JSON, &text);
        editor.secrets = Some(tokens.options());
        let (_owner, state) = editor.mount();
        let end = text.find(tokens.same).unwrap() + tokens.same.len();
        type_at(state, end, "Z");
        if field_a(Language::JSON, &state.doc().text.get_untracked())
            != Some(format!("x{}Z", tokens.same))
        {
            failures.push(format!(
                "{}: typing beside an embedded token refused",
                tokens.pattern
            ));
        }
    }
    report(failures);
}

/// Text glued on while the text did not parse is flagged at the token
/// once it parses, and an Edit-as save of it waits.
#[test]
fn text_glued_on_while_the_text_did_not_parse_is_flagged_and_not_saved() {
    let mut failures = Vec::new();
    for tokens in &HOSTS {
        for host in ALL {
            for view in ALL {
                let case = format!("{host} host as {view} ({})", tokens.pattern);
                let (_owner, state, heard) = mount(host, view, tokens);
                let shown = state.doc().text.get_untracked();
                let Some(start) = shown.find(tokens.same) else {
                    failures.push(format!("{case}: no token in\n{shown}"));
                    continue;
                };
                let end = shown.trim_end().len();
                let broken = breaking(view);
                paste(state, end..end, broken);
                type_at(state, start + tokens.same.len(), "Z");
                let now = state.doc().text.get_untracked();
                let Some(from) = now.rfind(broken) else {
                    failures.push(format!("{case}: not broken: {now:?}"));
                    continue;
                };
                paste(state, from..from + broken.len(), "");
                pause(state);
                let now = state.doc().text.get_untracked();
                if field_a(view, &now) != Some(format!("{}Z", tokens.same)) {
                    failures.push(format!("{case}: not glued while broken: {now:?}"));
                    continue;
                }
                let found = problems(state, view, None);
                if !found.iter().any(|problem| problem.contains(SAYS)) {
                    failures.push(format!("{case}: not flagged: {found:?}"));
                }
                if view == host {
                    continue;
                }
                if let Some(glued) = heard
                    .get_untracked()
                    .iter()
                    .find(|text| field_a(host, text).as_deref() != Some(tokens.same))
                {
                    failures.push(format!("{case}: the host heard {glued:?}"));
                }
                let waits = state.sync().conversion_problems.get_untracked().iter().any(|problem| {
                    matches!(&problem.text, EditAsText::NotSaved { error, .. } if error.pointer() == "/a")
                });
                if !waits {
                    failures.push(format!(
                        "{case}: the save does not wait: {:?}",
                        state.sync().conversion_problems.get_untracked()
                    ));
                }
            }
        }
    }
    report(failures);
}
