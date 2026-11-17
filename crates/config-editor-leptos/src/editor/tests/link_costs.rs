//! What the links of an Edit-as view's items cost: a Replace all over
//! nested maps or many small lists grows its work with the document only, a whole text pasted leaves no lasting work for
//! the keys typed after it, and cut, paste and undo over and
//! over keep no more than a fixed few records.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::test_state::TestEditor;
use super::moved_forms::{pause, units};
use crate::editor::work_count::{Work, counted};
use leptos::prelude::*;

/// `from..to` (bytes) of the view's text replaced by `typed`.
fn replace(state: EditorState, (from, to): (usize, usize), typed: &str) {
    let before = state.doc().text.get_untracked();
    let text = format!("{}{typed}{}", &before[..from], &before[to..]);
    let start = units(&before[..from]);
    state.commit(Edit::new_units(
        before.clone(),
        (start, units(&before[..to])),
        text,
        start + units(typed),
    ));
}

/// The work (each kind) of a Replace all of `from` by `to` and its pause,
/// in a YAML view of the JSON host `host`.
fn replace_all_work(host: &str, from: &str, to: &str) -> Vec<usize> {
    let mut editor = TestEditor::new(Language::JSON, host);
    editor.edit_as = Some(Language::YAML);
    let (_owner, state) = editor.mount();
    let before = state.doc().text.get_untracked();
    let after = before.replace(from, to);
    let replaced = before.matches(from).count();
    let ((), counts) = crate::editor::work_count::counted_all(|| {
        let splices = Some(crate::editor::bridge::links::Splice::each(
            before
                .match_indices(from)
                .map(|(at, _)| (at..at + from.len(), to.to_string())),
        ));
        state.commit(Edit::new_units(before.clone(), (0, 0), after, 0).splicing(splices));
        pause(state);
    });
    let saved = state.sync().host_text.get_value();
    assert_eq!(
        saved.matches(": true").count(),
        host.matches(": true").count() - replaced,
        "saved:\n{saved}"
    );
    counts.to_vec()
}

/// The kinds of work growing more than two and a half times from `small`
/// to `large`, at twice its size.
fn grown(small: &[usize], large: &[usize]) -> Vec<String> {
    Work::ALL
        .iter()
        .zip(small.iter().zip(large))
        .filter(|(_, (small, large))| **large > (**small).max(16) * 5 / 2)
        .map(|(work, (small, large))| format!("{work:?}: {small} → {large}"))
        .collect()
}

/// A JSON host of `copies` copies of the large sample, each under a key of
/// its own.
fn sample_copies(copies: usize) -> String {
    let sample: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/samples/tunnel.large.json"
    ))
    .expect("the sample reads");
    let host: serde_json::Map<String, serde_json::Value> = (0..copies)
        .map(|copy| (format!("copy{copy}"), sample.clone()))
        .collect();
    format!(
        "{}\n",
        serde_json::to_string_pretty(&host).expect("written")
    )
}

/// A JSON host of `count` keys, each holding a list of two small maps.
fn small_lists(count: usize) -> String {
    let host: serde_json::Map<String, serde_json::Value> = (0..count)
        .map(|at| {
            (
                format!("s{at}"),
                serde_json::json!([{"en": true}, {"en": true}]),
            )
        })
        .collect();
    format!(
        "{}\n",
        serde_json::to_string_pretty(&host).expect("written")
    )
}

/// P5: `hsts: true` → `false` over copies of the large sample (nested maps
/// holding lists and maps); each item's link was compared with the host's
/// whole text, so twice the copies cost four times the work.
#[test]
fn a_replace_all_over_nested_maps_costs_work_in_proportion() {
    let [small, large] =
        [1, 2].map(|copies| replace_all_work(&sample_copies(copies), "hsts: true", "hsts: false"));
    let grown = grown(&small, &large);
    assert!(
        grown.is_empty(),
        "twice the copies, more than twice the work: {grown:?}"
    );
}

/// P5: a Replace all changing every item of many small lists; each key
/// looked up by its spelling without the list's index scanned the whole
/// text for an alias.
#[test]
fn a_replace_all_over_many_small_lists_costs_work_in_proportion() {
    let [small, large] =
        [200, 400].map(|count| replace_all_work(&small_lists(count), "en: true", "en: false"));
    let grown = grown(&small, &large);
    assert!(
        grown.is_empty(),
        "twice the lists, more than twice the work: {grown:?}"
    );
}

/// The work of a key typed and its pause, in a JSON view of a YAML host
/// of `count` items, after the whole text was pasted over (`pasted`).
fn typed_work(count: usize, pasted: bool) -> (usize, usize) {
    let host: String = (0..count)
        .map(|at| format!("k{at}:\n  v: {at}\n"))
        .collect();
    let mut editor = TestEditor::new(Language::YAML, &host);
    editor.edit_as = Some(Language::JSON);
    let (_owner, state) = editor.mount();
    if pasted {
        // The same text from elsewhere, a value changed.
        let text = state.doc().text.get_untracked();
        let changed = text.replacen("\"v\": 0", "\"v\": 10", 1);
        replace(state, (0, text.len()), &changed);
        pause(state);
    }
    let text = state.doc().text.get_untracked();
    let at = text.rfind('}').expect("the end") - 2;
    let (((), steps), reads) = counted(Work::DocumentRead, || {
        counted(Work::LinkStep, || {
            replace(state, (at, at), "1");
            pause(state);
        })
    });
    (steps, reads)
}

/// A whole text pasted from outside the editor brings no links, yet
/// every value it leaves where it was, as it was, keeps its host text: a
/// save writes only what changed (the lens's GetPut).
#[test]
fn an_external_whole_paste_keeps_every_unchanged_values_form() {
    let host =
        "# top\n[a]\nwhen = 1979-05-27T07:32:00Z\nmask = 0xff\nk = 1 # note\n\n[b]\nz = 'q'\n";
    for view in [Language::YAML, Language::JSON] {
        let mut editor = TestEditor::new(Language::TOML, host);
        editor.edit_as = Some(view);
        let (_owner, state) = editor.mount();
        let text = state.doc().text.get_untracked();
        let pasted = text
            .replacen("k: 1", "k: 2", 1)
            .replacen("\"k\": 1", "\"k\": 2", 1)
            .replacen("q", "r", 1);
        replace(state, (0, text.len()), &pasted);
        pause(state);
        assert_eq!(
            state.sync().host_text.get_value(),
            "# top\n[a]\nwhen = 1979-05-27T07:32:00Z\nmask = 0xff\nk = 2 # note\n\n[b]\nz = 'r'\n",
            "{view:?}"
        );
    }
}

#[test]
fn a_whole_text_pasted_leaves_later_keys_no_more_work() {
    for count in [50, 200] {
        assert_eq!(
            typed_work(count, true),
            typed_work(count, false),
            "{count} items"
        );
    }
}

/// The steps of the leak check: an XML host's prefixed element (`c:in`) cut in a JSON
/// view, pasted twice, a pause, the three edits undone; `cycles`
/// times. What the links and records hold after.
fn cycled(cycles: usize) -> (usize, usize) {
    let host = "<r xmlns:c=\"urn:c\">\n  <c:in a=\"1\"/>\n  <b>2</b>\n  <d>3</d>\n</r>\n";
    let mut editor = TestEditor::new(Language::XML, host);
    editor.edit_as = Some(Language::JSON);
    let (_owner, state) = editor.mount();
    for _ in 0..cycles {
        let text = state.doc().text.get_untracked();
        let line = text
            .find("\"in\"")
            .unwrap_or_else(|| panic!("no element in\n{text}"));
        let from = text[..line].rfind('\n').map_or(0, |at| at + 1);
        let to = line + text[line..].find("},\n").expect("its end") + 3;
        let lines = text[from..to].to_string();
        replace(state, (from, to), "");
        for _ in 0..2 {
            let text = state.doc().text.get_untracked();
            let at = text.find("\"d\"").expect("d");
            let at = text[..at].rfind('\n').map_or(0, |at| at + 1);
            replace(state, (at, at), &lines);
        }
        pause(state);
        for _ in 0..3 {
            if let Some(undo) = state.history_edit(false) {
                state.commit(undo);
            }
        }
        pause(state);
    }
    let records = state
        .sync()
        .clipboard
        .with_value(|clipboard| clipboard.len());
    let links = state.sync().links.with_value(|links| links.bytes(None));
    (records, links)
}

#[test]
fn cut_paste_and_undo_over_and_over_keep_a_fixed_few_records() {
    let (records, links) = cycled(10);
    assert!(records <= 8, "{records} records");
    assert_eq!(cycled(40), (records, links));
}
