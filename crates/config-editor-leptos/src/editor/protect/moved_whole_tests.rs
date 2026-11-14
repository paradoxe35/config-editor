//! A placeholder the host gave whole stays whole wherever its field moves:
//! one fixed mask in several fields, its field's parent key renamed (and
//! renamed back), its line moved in its block or into another one: text
//! typed or pasted beside it is refused, in every language, before and
//! after the typing pause, and the host never hears it glued. (Renaming
//! the parent of one of two fields holding the mask let `«…»MY` through:
//! the field no longer at the path the host gave was judged nowhere.)

use leptos::prelude::*;

use super::super::Language;
use super::super::keys::line_edit::LineCommand;
use super::super::keys::lines::{Command, run};
use super::super::secret_options::SecretOptions;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::super::work_count::{Work, counted_all};
use super::glued_tests::{breaking, paste, type_at};

const MASK: &str = "«secret 0b1c»";

/// `blocks` clients written in `language`, client `i` holding the fields
/// `fields(i)` in order.
fn clients(
    language: Language,
    blocks: usize,
    fields: impl Fn(usize) -> Vec<(String, String)>,
) -> String {
    let block = |index: usize| {
        let fields = fields(index);
        let name = format!("c{index}");
        match language.label() {
            "YAML" => {
                let body: String = fields
                    .iter()
                    .map(|(key, value)| format!("    {key}: {value}\n"))
                    .collect();
                format!("  {name}:\n{body}")
            }
            "JSON" => {
                let body: Vec<String> = fields
                    .iter()
                    .map(|(key, value)| format!("      \"{key}\": \"{value}\""))
                    .collect();
                format!("    \"{name}\": {{\n{}\n    }}", body.join(",\n"))
            }
            "TOML" => {
                let body: String = fields
                    .iter()
                    .map(|(key, value)| format!("{key} = \"{value}\"\n"))
                    .collect();
                format!("[clients.{name}]\n{body}\n")
            }
            "HCL" => {
                let body: String = fields
                    .iter()
                    .map(|(key, value)| format!("    {key} = \"{value}\"\n"))
                    .collect();
                format!("  {name} {{\n{body}  }}\n")
            }
            _ => {
                let body: String = fields
                    .iter()
                    .map(|(key, value)| format!("    <{key}>{value}</{key}>\n"))
                    .collect();
                format!("  <{name}>\n{body}  </{name}>\n")
            }
        }
    };
    let blocks: Vec<String> = (0..blocks).map(block).collect();
    match language.label() {
        "YAML" => format!("clients:\n{}", blocks.concat()),
        "JSON" => format!("{{\n  \"clients\": {{\n{}\n  }}\n}}\n", blocks.join(",\n")),
        "TOML" => blocks.concat(),
        "HCL" => format!("clients {{\n{}}}\n", blocks.concat()),
        _ => format!("<r>\n<clients>\n{}</clients>\n</r>\n", blocks.concat()),
    }
}

/// Clients each holding the mask alone under one key: alike, so which
/// one a renamed key is cannot be told by its value.
fn alike(language: Language, blocks: usize) -> String {
    clients(language, blocks, |_| vec![("secret".into(), MASK.into())])
}

/// Clients each holding the mask under a key of its own, then a port.
fn keyed(language: Language, blocks: usize) -> String {
    clients(language, blocks, |index| {
        vec![
            (format!("key{index}"), MASK.into()),
            ("port".into(), format!("{index}")),
        ]
    })
}

/// A live editor on `text`, with what `on_change` heard.
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

fn shown(state: EditorState) -> String {
    state.doc().text.get_untracked()
}

/// Inserts `with` after each of the first client's name (both tags in
/// XML); or takes it out again (`back`).
fn rename_first(state: EditorState, back: bool) {
    for nth in 0..2 {
        let text = shown(state);
        let Some(at) = text.match_indices("c0").nth(nth).map(|(at, _)| at + 2) else {
            return;
        };
        if back {
            if text[at..].starts_with('x') {
                paste(state, at..at + 1, "");
            }
        } else {
            type_at(state, at, "x");
        }
    }
}

/// Alt+Up (or Alt+Down) on the line holding `holding`.
fn move_line(state: EditorState, holding: &str, down: bool) {
    let text = shown(state);
    let at = text.find(holding).expect("the line to move");
    let units = text[..at].encode_utf16().count();
    run(
        state,
        Command::Lines(LineCommand::Move { down }),
        (units, units),
        false,
    );
}

/// The fields of the parsed `text` (in `language`) holding the mask with
/// something beside it.
fn glued(language: Language, text: &str) -> Vec<String> {
    fn walk(value: &serde_json::Value, at: String, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(text) if text.contains(MASK) && text.trim() != MASK => {
                out.push(format!("{at} = {text:?}"));
            }
            serde_json::Value::Array(items) => {
                for (index, item) in items.iter().enumerate() {
                    walk(item, format!("{at}/{index}"), out);
                }
            }
            serde_json::Value::Object(map) => {
                for (key, item) in map {
                    walk(item, format!("{at}/{key}"), out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    if let Ok(value) = language.parse(text) {
        walk(&value, String::new(), &mut out);
    }
    out
}

/// A way the field holding the first placeholder moves, and the text of
/// the line it is on then.
type Move = (&'static str, fn(Language, usize) -> String, fn(EditorState));

const MOVES: [Move; 4] = [
    ("its parent key renamed", alike, |state| {
        rename_first(state, false)
    }),
    ("its parent key renamed and back", alike, |state| {
        rename_first(state, false);
        rename_first(state, true);
    }),
    ("its line moved in its block", keyed, |state| {
        move_line(state, "key0", true)
    }),
    ("its line moved into the block above", keyed, |state| {
        move_line(state, "key1", false)
    }),
];

#[test]
fn text_beside_a_shared_mask_whose_field_moved_is_refused() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        for blocks in [2, 5] {
            for (moved, document, apply) in MOVES {
                for pause in [false, true] {
                    for (beside, after) in [("typed after", true), ("pasted before", false)] {
                        let case = format!(
                            "{language}, {blocks} fields, {moved}{}, {beside} it",
                            if pause { ", a pause" } else { "" }
                        );
                        let text = document(language, blocks);
                        let (_owner, state, heard) = mount(language, &text);
                        apply(state);
                        if pause {
                            state.recheck_tokens();
                        }
                        let now = shown(state);
                        // The mask of the field that moved: the first one,
                        // or the one moved into the block above.
                        let key = if moved.contains("above") { "key1" } else { "" };
                        let from = now.find(key).unwrap_or(0);
                        let Some(start) = now[from..].find(MASK).map(|at| from + at) else {
                            failures.push(format!("{case}: no mask in\n{now}"));
                            continue;
                        };
                        if after {
                            type_at(state, start + MASK.len(), "MY");
                        } else {
                            paste(state, start..start, "MY");
                        }
                        state.recheck_tokens();
                        let end = shown(state);
                        let mut found = glued(language, &end);
                        if end.contains(&format!("{MASK}MY")) || end.contains(&format!("MY{MASK}"))
                        {
                            found.push(format!("shown {end:?}"));
                        }
                        for text in heard.get_untracked() {
                            found.extend(
                                glued(language, &text)
                                    .into_iter()
                                    .map(|field| format!("heard {field}")),
                            );
                        }
                        if !found.is_empty() {
                            failures.push(format!("{case}: {found:?}"));
                        }
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} cases: {failures:#?}",
        failures.len()
    );
}

/// The first client renamed and the typing pause past: its mask is in
/// none of the host's fields.
fn renamed(language: Language, blocks: usize) -> (Owner, EditorState, RwSignal<Vec<String>>) {
    let (owner, state, heard) = mount(language, &alike(language, blocks));
    rename_first(state, false);
    state.recheck_tokens();
    (owner, state, heard)
}

/// Text typed beside a moved mask while the text does not parse (an
/// error elsewhere) is refused too, with the refusal naming the field it
/// is in now; once repaired, nothing glued reaches the host.
#[test]
fn text_beside_a_moved_mask_while_the_text_does_not_parse_is_refused() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        let (_owner, state, heard) = renamed(language, 2);
        let end = shown(state).trim_end().len();
        let broken = breaking(language);
        paste(state, end..end, broken);
        state.protect().tip.set(None);
        let start = shown(state).find(MASK).expect("the mask");
        type_at(state, start + MASK.len(), "MY");
        let now = shown(state);
        if now.contains(&format!("{MASK}M")) {
            failures.push(format!("{language}: typed beside it while broken: {now:?}"));
        }
        let tip = state.protect().tip.get_untracked().map(|tip| tip.message);
        if !tip
            .as_deref()
            .is_some_and(|tip| tip.contains("c0x") && tip.contains("whole secret"))
        {
            failures.push(format!("{language}: refusal {tip:?}"));
        }
        // Repaired.
        let now = shown(state);
        let end = now.rfind(broken).expect("the error");
        paste(state, end..end + broken.len(), "");
        for text in heard.get_untracked() {
            if text.contains(&format!("{MASK}M")) {
                failures.push(format!("{language}: the host heard {text:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// What stays free around a moved mask: renaming its own key, replacing
/// it whole, renaming the next client, while the text parses and while
/// it does not; and a mask the host wrote with other text beside it (a
/// note quoting it) takes more text.
#[test]
fn edits_around_a_moved_mask_stay_free() {
    let mut failures = Vec::new();
    for language in Language::ALL.iter().copied() {
        for broken in [false, true] {
            let case = format!("{language}{}", if broken { " (broken)" } else { "" });
            let (_owner, state, _) = renamed(language, 3);
            if broken {
                let end = shown(state).trim_end().len();
                paste(state, end..end, breaking(language));
            }
            // Its own key renamed (both tags in XML).
            let text = shown(state);
            let line = text.find("secret").expect("its key");
            type_at(state, line + "secret".len(), "x");
            let text = shown(state);
            if let Some(close) = text.find("</secret>") {
                type_at(state, close + "</secret".len(), "x");
            }
            if !shown(state).contains("secretx") {
                failures.push(format!(
                    "{case}: its key renamed refused:\n{}",
                    shown(state)
                ));
            }
            // Replaced whole.
            let text = shown(state);
            let start = text.find(MASK).expect("the mask");
            if !paste(state, start..start + MASK.len(), "new") {
                failures.push(format!("{case}: replacing it refused:\n{}", shown(state)));
            }
            // The next client renamed.
            let text = shown(state);
            let next = text.find("c1").expect("the next client") + 2;
            type_at(state, next, "z");
            if !shown(state).contains("c1z") {
                failures.push(format!("{case}: the next key refused:\n{}", shown(state)));
            }
        }
        // A note quoting the mask, from the host: free to extend.
        let text = clients(language, 2, |_| {
            vec![
                ("secret".into(), MASK.into()),
                ("note".into(), format!("see {MASK} here")),
            ]
        });
        let (_owner, state, _) = mount(language, &text);
        rename_first(state, false);
        state.recheck_tokens();
        let at = shown(state).find(" here").expect("the note");
        type_at(state, at, " too");
        if !shown(state).contains(&format!("see {MASK} too here")) {
            failures.push(format!(
                "{language}: the note's text refused:\n{}",
                shown(state)
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Each kind of work (by [`Work::ALL`]) typing a letter in the middle
/// client's port and beside the first client's mask (refused), its name
/// renamed so the mask is in none of the host's fields, among `blocks`
/// clients each holding the mask and a port.
fn renamed_work(language: Language, blocks: usize) -> (Vec<usize>, Vec<usize>) {
    let text = clients(language, blocks, |index| {
        vec![
            ("secret".into(), MASK.into()),
            ("port".into(), format!("p{index}")),
        ]
    });
    let (_owner, state, _) = mount(language, &text);
    rename_first(state, false);
    state.recheck_tokens();
    let middle = format!("p{}", blocks / 2);
    let at = shown(state).find(&middle).expect("the middle port") + middle.len();
    let ((), far) = counted_all(|| type_at(state, at, "7"));
    let at = shown(state).find(MASK).expect("the moved mask") + MASK.len();
    let ((), beside) = counted_all(|| type_at(state, at, "Z"));
    assert!(
        !shown(state).contains(&format!("{MASK}Z")),
        "{language}: a letter beside the moved mask is refused"
    );
    (far.to_vec(), beside.to_vec())
}

/// Finding the masks in none of their fields near an edit reads a few
/// lines around it, never the document: a key away from them costs no
/// more for more of them, and one beside a moved one work linear in them.
#[test]
fn a_moved_mask_costs_a_key_no_pass_over_the_masks() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for language in Language::ALL.iter().copied() {
        let (small_far, small_beside) = renamed_work(language, 100);
        let (large_far, large_beside) = renamed_work(language, 400);
        for (kind, work) in Work::ALL.iter().enumerate() {
            // Placing the typed key's UTF-16 offset passes the text before
            // it, with or without placeholders.
            let placing = matches!(work, Work::CharacterStep | Work::OffsetSeek);
            if !placing && large_far[kind] > small_far[kind] {
                costly.push(format!(
                    "{language}: {work:?} {} for a key far from 100 masks, {} from 400",
                    small_far[kind], large_far[kind]
                ));
            }
            if large_beside[kind] > 6 * small_beside[kind].max(100) {
                costly.push(format!(
                    "{language}: {work:?} {} for a key beside a moved one of 100 masks, {} of 400",
                    small_beside[kind], large_beside[kind]
                ));
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}

/// Each kind of work (by [`Work::ALL`]) the typing pause after a key in
/// the middle client's port costs (the placeholders' fields checked, then
/// the problems, with a schema marking the masked field secret), among
/// `blocks` clients each holding the one mask.
#[cfg(feature = "schema-validation")]
fn pause_work(language: Language, blocks: usize) -> Vec<usize> {
    let secret = serde_json::json!({"additionalProperties": {"properties": {
        "secret": {"x-secret": true}}}});
    let schema = serde_json::json!({"properties": {
        "clients": secret,
        "r": {"properties": {"clients": secret}}}});
    let text = clients(language, blocks, |index| {
        vec![
            ("secret".into(), MASK.into()),
            ("port".into(), format!("p{index}")),
        ]
    });
    let mut editor = TestEditor::new(language, &text);
    editor.secrets = Some(SecretOptions::default().with_placeholder("«secret [0-9a-f]{4}»"));
    editor.schema = Some(std::sync::Arc::new(
        super::super::diagnostics::build_schema(&schema.to_string()).expect("the schema"),
    ));
    let (_owner, state) = editor.mount();
    let middle = format!("p{}", blocks / 2);
    let at = text.find(&middle).expect("the middle port") + middle.len();
    type_at(state, at, "7");
    let ((), work) = counted_all(|| {
        state.recheck_tokens();
        state.problems().diagnosed_text.set(shown(state));
        state.problems().diagnostics.with_untracked(Vec::len);
    });
    work.to_vec()
}

/// The typing pause after a key costs work linear in the fields sharing
/// one mask, in every language (QA 33: YAML with 500, 1000, 2000 such
/// fields paused 92, 288, 755 ms: each placeholder's field was looked for
/// among every field given its token, to accept it for the schema).
#[cfg(feature = "schema-validation")]
#[test]
fn the_typing_pause_costs_work_linear_in_the_fields_sharing_a_mask() {
    super::super::parse_cache::hold();
    let mut costly = Vec::new();
    for language in Language::ALL.iter().copied() {
        let small = pause_work(language, 100);
        let large = pause_work(language, 400);
        for (kind, work) in Work::ALL.iter().enumerate() {
            if large[kind] > 6 * small[kind].max(100) {
                costly.push(format!(
                    "{language}: {work:?} {} for the pause among 100 masked fields, {} among 400",
                    small[kind], large[kind]
                ));
            }
        }
    }
    super::super::parse_cache::release();
    assert!(costly.is_empty(), "{costly:#?}");
}
