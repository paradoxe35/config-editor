//! A map key of an XML host (not a list item) cut in an
//! Edit-as view and pasted back after a pause, a switch or a round trip
//! through the host's language keeps its written name (prefix), its
//! attribute or element form and its markup, as a cut item does, wherever among its siblings it is pasted, at once too
//! (a prefixed key moved in one save used to wait); never is it saved
//! as a new key with that markup lost.

use super::super::edit::Edit;
use super::super::languages::Language;
use super::super::state::EditorState;
use super::super::sync::{convert_now, switch_to};
use super::super::test_state::TestEditor;
use crate::editor::save_matrix::report;
use leptos::prelude::*;

/// How the host writes the key cut, `scan`.
#[derive(Clone, Copy, Debug)]
enum Kind {
    Attributes,
    Prefixed,
    LongPrefix,
    Children,
    OwnNamespace,
}

impl Kind {
    /// The key's element, as the host writes it.
    fn element(self) -> &'static str {
        match self {
            Kind::Attributes => "<scan pkg=\"com.example\" lazy=\"true\"/>",
            Kind::Prefixed => "<c:scan pkg=\"com.example\" lazy=\"true\"/>",
            Kind::LongPrefix => "<context:scan pkg=\"com.example\" lazy=\"true\"/>",
            Kind::Children => {
                "<c:scan pkg=\"com.example\">\n    <c:lazy>true</c:lazy>\n    <n>4</n>\n  </c:scan>"
            }
            Kind::OwnNamespace => "<scan xmlns=\"urn:s\" pkg=\"com.example\" lazy=\"true\"/>",
        }
    }

    /// A host holding no namespace declaration and no prefix where the
    /// key needs none: its attributes are all it holds that no view shows.
    fn host(self) -> String {
        let root = match self {
            Kind::Attributes | Kind::OwnNamespace => "<r>",
            Kind::Prefixed | Kind::Children => "<r xmlns:c=\"urn:c\">",
            Kind::LongPrefix => "<r xmlns:context=\"urn:c\">",
        };
        format!(
            "{root}\n  {}\n  <a>1</a>\n  <b>2</b>\n  <d x=\"1\">\n    <e>3</e>\n  </d>\n</r>\n",
            self.element()
        )
    }
}

/// What happens between the cut and the paste.
#[derive(Clone, Copy, Debug)]
enum Between {
    Nothing,
    Pause,
    OtherView,
    PauseThenOtherView,
    HostLanguage,
}

/// Where the key is pasted among its siblings `a`, `b` and `d`.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    Same,
    Middle,
    End,
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

fn line_offset(text: &str, line: usize) -> usize {
    text.split_inclusive('\n').take(line).map(str::len).sum()
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// The lines (first, past the last) of the entry whose first line holds
/// `name` in a view of `r`'s keys.
fn entry_lines(view: Language, text: &str, name: &str) -> (usize, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let first = lines
        .iter()
        .position(|line| {
            let line = line.trim_start().trim_start_matches(['"', '[']);
            line.starts_with(name)
                || line
                    .strip_prefix("r.")
                    .is_some_and(|rest| rest.starts_with(name))
        })
        .expect("the entry");
    let opens = lines[first].trim_end().ends_with(['{', ':']);
    let last = if view == Language::TOML {
        (first + 1..lines.len())
            .find(|at| lines[*at].trim().is_empty() || lines[*at].starts_with('['))
            .unwrap_or(lines.len())
    } else if !opens {
        first + 1
    } else if view == Language::YAML {
        (first + 1..lines.len())
            .find(|at| indent(lines[*at]) <= indent(lines[first]))
            .unwrap_or(lines.len())
    } else {
        (first + 1..lines.len())
            .find(|at| indent(lines[*at]) == indent(lines[first]))
            .map_or(lines.len(), |at| at + 1)
    };
    (first, last)
}

/// `scan`'s lines cut from the view.
fn cut(state: EditorState, view: Language) -> String {
    let before = state.doc().text.get_untracked();
    let (start, end) = entry_lines(view, &before, "scan");
    let (from, to) = (line_offset(&before, start), line_offset(&before, end));
    let lines = before[from..to].to_string();
    let text = format!("{}{}", &before[..from], &before[to..]);
    let caret = units(&before[..from]);
    state.commit(Edit::new_units(before, (caret, caret), text, caret));
    lines
}

/// `lines` pasted at `place`, `None` where the view has no such place.
fn paste(state: EditorState, view: Language, lines: &str, place: Place) -> Option<()> {
    let before = state.doc().text.get_untracked();
    let toml = view == Language::TOML;
    let (at, inserted) = match place {
        Place::Middle if toml => return None,
        Place::Same if toml => (
            line_offset(&before, entry_lines(view, &before, "d").0),
            lines.to_string(),
        ),
        Place::End if toml => (before.len(), format!("\n{}", lines.trim_end())),
        Place::Same => (
            line_offset(&before, entry_lines(view, &before, "a").0),
            lines.to_string(),
        ),
        Place::Middle => (
            line_offset(&before, entry_lines(view, &before, "b").0),
            lines.to_string(),
        ),
        Place::End if view == Language::JSON => {
            // A comma typed after the last key, the lines pasted after it,
            // and their own comma deleted.
            let close = line_offset(&before, entry_lines(view, &before, "d").1) - 1;
            type_at(state, (close, close), ",");
            let at = close + 2;
            type_at(state, (at, at), lines);
            let comma = at + lines.trim_end().len() - 1;
            type_at(state, (comma, comma + 1), "");
            return Some(());
        }
        Place::End => (
            line_offset(&before, entry_lines(view, &before, "d").1),
            lines.to_string(),
        ),
    };
    let text = format!("{}{inserted}{}", &before[..at], &before[at..]);
    let start = units(&before[..at]);
    let caret = start + units(&inserted);
    state.commit(Edit::new_units(before, (start, start), text, caret));
    Some(())
}

/// Bytes `from..to` of the view's text replaced by `typed`.
fn type_at(state: EditorState, (from, to): (usize, usize), typed: &str) {
    let before = state.doc().text.get_untracked();
    let text = format!("{}{typed}{}", &before[..from], &before[to..]);
    let start = units(&before[..from]);
    let caret = start + units(typed);
    state.commit(Edit::new_units(
        before.clone(),
        (start, units(&before[..to])),
        text,
        caret,
    ));
}

fn between(state: EditorState, view: Language, step: Between) {
    let other = if view == Language::YAML {
        Language::JSON
    } else {
        Language::YAML
    };
    match step {
        Between::Nothing => {}
        Between::Pause => pause(state),
        Between::OtherView => {
            switch_to(state, other);
            pause(state);
            switch_to(state, view);
        }
        Between::PauseThenOtherView => {
            pause(state);
            switch_to(state, other);
            switch_to(state, view);
        }
        Between::HostLanguage => {
            switch_to(state, Language::XML);
            pause(state);
            switch_to(state, view);
        }
    }
}

#[test]
fn a_key_cut_and_pasted_keeps_its_name_form_and_markup() {
    let mut failures = Vec::new();
    for kind in [
        Kind::Attributes,
        Kind::Prefixed,
        Kind::LongPrefix,
        Kind::Children,
        Kind::OwnNamespace,
    ] {
        let host = kind.host();
        for view in [
            Language::YAML,
            Language::JSON,
            Language::TOML,
            Language::HCL,
        ] {
            for step in [
                Between::Nothing,
                Between::Pause,
                Between::OtherView,
                Between::PauseThenOtherView,
                Between::HostLanguage,
            ] {
                for place in [Place::Same, Place::Middle, Place::End] {
                    let mut editor = TestEditor::new(Language::XML, &host);
                    editor.edit_as = Some(view);
                    let (_owner, state) = editor.mount();
                    let lines = cut(state, view);
                    between(state, view, step);
                    if paste(state, view, &lines, place).is_none() {
                        continue;
                    }
                    pause(state);
                    let saved = state.sync().host_text.get_value();
                    let kept = saved.contains(kind.element())
                        && saved.matches("scan").count() == host.matches("scan").count();
                    // In its place among the siblings the view put it in.
                    let order: Vec<usize> = ["scan", "<a>", "<b>", "<d "]
                        .iter()
                        .map(|name| saved.find(name).unwrap_or(0))
                        .collect();
                    // A TOML view writes tables after keys: no place
                    // among them tells the element's.
                    let wanted_order = match place {
                        _ if view == Language::TOML => true,
                        Place::Same => order[0] < order[1],
                        Place::Middle => order[1] < order[0] && order[0] < order[2],
                        Place::End => order[3] < order[0],
                    };
                    if !(kept && wanted_order) {
                        failures.push(format!(
                            "{kind:?} as {view:?}, {step:?}, paste {place:?}:\n{saved}"
                        ));
                    }
                }
            }
        }
    }
    report(failures);
}

/// The pause saving a key pasted back follows every key of the view, yet
/// compares the pasted ones with the cut ones only, and reads the texts
/// as often, at any size.
#[test]
fn a_key_pasted_back_is_found_with_the_same_work_at_any_size() {
    use crate::editor::work_count::{Work, counted};
    let work = |count: usize| {
        let siblings: String = (0..count)
            .map(|at| format!("  <k{at} v=\"{at}\"/>\n"))
            .collect();
        let host = format!(
            "<r>\n  {}\n  <a>1</a>\n  <b>2</b>\n{siblings}  <d x=\"1\">\n    <e>3</e>\n  </d>\n</r>\n",
            Kind::Attributes.element()
        );
        let mut editor = TestEditor::new(Language::XML, &host);
        editor.edit_as = Some(Language::YAML);
        let (_owner, state) = editor.mount();
        let lines = cut(state, Language::YAML);
        pause(state);
        paste(state, Language::YAML, &lines, Place::Middle);
        let ((), compared) = counted(Work::ItemComparison, || pause(state));
        let saved = state.sync().host_text.get_value();
        assert!(saved.contains(Kind::Attributes.element()), "{saved}");
        compared
    };
    let small = work(20);
    assert_eq!(small, work(400));
    assert!(small <= 4, "{small} comparisons");
}
