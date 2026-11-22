//! The time an Edit-as save of typing takes on a large document (the
//! demo's samples, their lists four times over, under the tunnel schema):
//! it grows with the document no faster than the document (P5). The
//! READMEs' "fast" claim rests on it: a typed value saves in work linear
//! in the document, whatever the host and view.

use super::switch_work::{document, mounted, schema, switch};
use crate::editor::edit::Edit;
use crate::editor::languages::Language;
use crate::editor::state::EditorState;
use crate::editor::sync::convert_now;

/// A letter typed into the public address in the view on screen
/// (`view`), then the save the typing pause makes; whether the host's
/// text holds it after.
fn typed_save(state: EditorState, view: Language) -> bool {
    let shown = state.doc().text.get_untracked();
    // Before the address's `.example.com` (each run types one more).
    let at = shown.find(".example.com").expect("the address");
    let typed = format!("{}x{}", &shown[..at], &shown[at..]);
    let units = shown[..at].encode_utf16().count();
    assert!(state.commit(Edit::input_units(
        shown,
        (units, units),
        typed.clone(),
        (units + 1, units + 1),
        "insertText".into(),
        0.0,
    )));
    convert_now(state, typed, view);
    state.sync().host_text.get_value().contains("tunnelx")
}

/// Timed: a letter typed and saved takes time that grows with the
/// document no faster than the document, in each host edited in another
/// view. Four times the entries take about four times as long; a read of
/// the document for each entry would take sixteen times.
#[test]
fn a_typed_save_takes_time_linear_in_the_document() {
    // As a mounted editor does (`sync::connect`).
    crate::editor::parse_cache::hold();
    let schema = schema();
    let (small, large) = (document(1), document(4));
    let mut failures = Vec::new();
    for &host in Language::ALL.iter() {
        let view = if host == Language::JSON {
            Language::YAML
        } else {
            Language::JSON
        };
        let texts = [&small, &large].map(|value| host.serialize(value).unwrap());
        let editors = texts.each_ref().map(|text| {
            let mounted = mounted(host, text.as_str(), &schema);
            switch(mounted.1, view);
            mounted
        });
        let mut saved = true;
        let growth = crate::editor::test_timing::growth(3, (&texts[0], &texts[1]), |text| {
            let state = editors[usize::from(*text != texts[0])].1;
            saved &= typed_save(state, view);
        });
        if !saved {
            failures.push(format!("{host} as {view}: the typing was not saved"));
        }
        if growth.ratio() > 10.0 {
            failures.push(format!("{host} as {view}, a typed save: {growth}"));
        }
    }
    crate::editor::parse_cache::release();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
