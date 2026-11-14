//! QA `E34-1`: an item of an XML host cut in an Edit-as view and pasted
//! back after a switch (to another view, or the host's language and back)
//! keeps its markup (`xsi:type`, a namespace declaration, a prefix), as it
//! does across pauses and saves: the session keeps the items cut, and a
//! pasted item holding one's data is it, whatever was saved or switched
//! between the cut and the paste, and wherever it is pasted. (A paste
//! giving back the text saved before, items of the same data moved, saves
//! as that text was saved: the host's text as it was.)

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;
use super::save::xml_item_edits::{Kind, item_lines, line_offset};
use crate::editor::save_matrix::report;
use leptos::prelude::*;

/// What happens between the cut and the paste.
#[derive(Clone, Copy, Debug)]
enum Between {
    Pause,
    PauseTwice,
    OtherView,
    PauseThenOtherView,
    HostLanguage,
    TwoViews,
}

/// Where the cut item is pasted, in the view of items 0, 2, 3 left.
#[derive(Clone, Copy, Debug)]
enum Place {
    Start,
    Same,
    Middle,
    End,
}

impl Place {
    /// The host items in the order saved.
    fn wanted(self) -> [usize; 4] {
        match self {
            Place::Start => [1, 0, 2, 3],
            Place::Same => [0, 1, 2, 3],
            Place::Middle => [0, 2, 1, 3],
            Place::End => [0, 2, 3, 1],
        }
    }
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

fn pause(state: EditorState) {
    convert_now(
        state,
        state.doc().text.get_untracked(),
        state.sync().edit_language.get_untracked(),
    );
}

/// Item 1's lines cut from the view; the lines.
fn cut(state: EditorState, view: Language) -> String {
    let before = state.doc().text.get_untracked();
    let (start, end) = item_lines(view, &before)[1];
    let (from, to) = (line_offset(&before, start), line_offset(&before, end));
    let lines = before[from..to].to_string();
    let text = format!("{}{}", &before[..from], &before[to..]);
    let caret = units(&before[..from]);
    state.commit(Edit::new(before, (caret, caret), text, caret));
    lines
}

/// `lines` pasted at `place`: whole lines before an item, or after the
/// last one (a JSON item's comma moved to the item before it).
fn paste(state: EditorState, view: Language, lines: &str, place: Place) {
    let before = state.doc().text.get_untracked();
    let items = item_lines(view, &before);
    let (at, inserted) = match place {
        Place::Start => (line_offset(&before, items[0].0), lines.to_string()),
        Place::Same => (line_offset(&before, items[1].0), lines.to_string()),
        Place::Middle => (line_offset(&before, items[2].0), lines.to_string()),
        Place::End if view == Language::JSON => {
            let close = line_offset(&before, items[2].1) - 1;
            let last = lines.trim_end().strip_suffix(',').expect("a middle item");
            (close, format!(",\n{last}"))
        }
        Place::End => (line_offset(&before, items[2].1), lines.to_string()),
    };
    let text = format!("{}{inserted}{}", &before[..at], &before[at..]);
    let start = units(&before[..at]);
    let caret = start + units(&inserted);
    state.commit(Edit::new(before, (start, start), text, caret));
}

fn between(state: EditorState, view: Language, step: Between) {
    let other = if view == Language::YAML {
        Language::JSON
    } else {
        Language::YAML
    };
    let views: &[Language] = match step {
        Between::Pause => return pause(state),
        Between::PauseTwice => {
            pause(state);
            switch_to(state, other);
            switch_to(state, view);
            return pause(state);
        }
        Between::OtherView => &[other],
        Between::PauseThenOtherView => {
            pause(state);
            &[other]
        }
        Between::HostLanguage => &[Language::XML],
        Between::TwoViews => &[other, Language::TOML],
    };
    for next in views {
        switch_to(state, *next);
        pause(state);
    }
    switch_to(state, view);
}

#[test]
fn an_item_cut_and_pasted_after_switches_keeps_its_markup() {
    let mut failures = Vec::new();
    for kind in [Kind::Type, Kind::DefaultNamespace, Kind::Prefix] {
        for sizes in [[1u32, 2, 3, 4], [2, 1, 1, 3]] {
            let host = kind.host(&sizes);
            for view in [Language::YAML, Language::JSON] {
                for step in [
                    Between::Pause,
                    Between::PauseTwice,
                    Between::OtherView,
                    Between::PauseThenOtherView,
                    Between::HostLanguage,
                    Between::TwoViews,
                ] {
                    for place in [Place::Start, Place::Same, Place::Middle, Place::End] {
                        let mut editor = TestEditor::new(Language::XML, &host);
                        editor.edit_as = Some(view);
                        let (_owner, state) = editor.mount();
                        let lines = cut(state, view);
                        between(state, view, step);
                        paste(state, view, &lines, place);
                        pause(state);
                        let saved = state.sync().host_text.get_value();
                        let sizes_saved: Vec<u32> =
                            place.wanted().iter().map(|at| sizes[*at]).collect();
                        let wanted = if sizes_saved == sizes {
                            [0, 1, 2, 3]
                        } else {
                            place.wanted()
                        };
                        let written: Vec<u32> = saved
                            .match_indices("<size>")
                            .map(|(at, _)| {
                                saved[at + 6..]
                                    .split('<')
                                    .next()
                                    .unwrap_or("")
                                    .parse()
                                    .unwrap_or(0)
                            })
                            .collect();
                        if kind.order(&saved) != wanted || written != sizes_saved {
                            failures.push(format!(
                                "{kind:?} {sizes:?} as {view:?}, {step:?}, paste {place:?}: items {:?}, wanted {wanted:?}:\n{saved}",
                                kind.order(&saved)
                            ));
                        }
                    }
                }
            }
        }
    }
    report(failures);
}
