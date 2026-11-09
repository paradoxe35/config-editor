//! Thousands of host problems by path or range, placed in the host's own
//! language and through every Edit-as view: each text is read a fixed
//! number of times however many problems there are (QA 21: each path read
//! the whole document again, and each range read the host's text from the
//! top and looked its field up in both texts: 3000 problems took 24 s at
//! mount and seconds after every edit).

use std::time::Duration;

use leptos::prelude::StoredValue;

use serde_json::{Value, json};

use super::super::Language;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::paths::{TextPosition, TextRange};
use super::super::test_timing::best_of;
use super::super::work_count::{Work, counted};
use super::tests::report;
use super::{HostDiagnostic, HostTargets, RangeText, place};
use crate::strings::default_strings;

pub(super) const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// `items` fields of two scalars each, under one root (every language
/// writes it: XML needs the one root).
pub(super) fn document(items: usize) -> Value {
    let mut root = serde_json::Map::new();
    for i in 0..items {
        root.insert(
            format!("key{i}"),
            json!({ "name": format!("value{i}"), "port": 1000 + i }),
        );
    }
    json!({ "root": root })
}

/// A problem at each item's `port`, by path.
fn by_path(items: usize) -> Vec<HostDiagnostic> {
    (0..items)
        .map(|i| {
            HostDiagnostic::at_path(
                format!("/root/key{i}/port"),
                Severity::Error,
                format!("bad {i}"),
            )
        })
        .collect()
}

/// A problem at each item's `port` in `text`, by range: from its key to
/// the line's end, and on its value alone.
fn by_range(text: &str) -> Vec<HostDiagnostic> {
    let mut ranges = Vec::new();
    for (index, line) in text.split('\n').enumerate() {
        let Some(key) = line.find("port") else {
            continue;
        };
        let line_number = index + 1;
        let width = line.chars().count() + 1;
        let value = line[key..]
            .find(|c: char| c.is_ascii_digit())
            .map(|at| key + at);
        ranges.push(HostDiagnostic::at_range(
            TextRange::new(
                TextPosition::new(line_number, key + 1),
                TextPosition::new(line_number, width),
            ),
            Severity::Error,
            format!("key {line_number}"),
        ));
        if let Some(value) = value {
            ranges.push(HostDiagnostic::at_range(
                TextRange::new(
                    TextPosition::new(line_number, value + 1),
                    TextPosition::new(line_number, value + 2),
                ),
                Severity::Warning,
                format!("value {line_number}"),
            ));
        }
    }
    ranges
}

/// Each way problems are placed on `host`'s text: on it, and through each
/// other language's view of it.
fn views(host: Language) -> impl Iterator<Item = Option<Language>> {
    std::iter::once(None).chain(
        LANGUAGES
            .into_iter()
            .filter(move |edit| *edit != host)
            .map(Some),
    )
}

/// The problems placed for `items` items of `host`, shown as `view` (the
/// host's own text when `None`), by path or range: how many were placed,
/// and the reads of the documents, path lookups and line splits it took.
fn placed(
    host: Language,
    view: Option<Language>,
    items: usize,
    ranges: bool,
) -> (usize, [usize; 3]) {
    let value = document(items);
    let host_text = host.serialize(&value).expect("the host's text");
    let shown_text = view.map(|edit| edit.serialize(&value).expect("the view's text"));
    let problems = if ranges {
        by_range(&host_text)
    } else {
        by_path(items)
    };
    let run = || match (view, &shown_text) {
        (Some(edit), Some(shown)) => {
            let found = HostTargets::new(host.behavior(), &host_text, &problems);
            let ranges = RangeText::Host(&found);
            place(
                edit.behavior(),
                shown,
                &problems,
                ranges,
                &[],
                default_strings(),
            )
        }
        _ => place(
            host.behavior(),
            &host_text,
            &problems,
            RangeText::OnScreen,
            &[],
            default_strings(),
        ),
    };
    let mut counts = [0; 3];
    let ((shown, lookups), reads) = counted(Work::DocumentRead, || {
        counted(Work::PathLookup, || {
            let (shown, splits): (Vec<Diagnostic>, usize) = counted(Work::LineSplit, run);
            counts[2] = splits;
            shown
        })
    });
    counts[0] = reads;
    counts[1] = lookups;
    (
        shown.iter().filter(|shown| shown.line.is_some()).count(),
        counts,
    )
}

fn name(host: Language, view: Option<Language>) -> String {
    match view {
        Some(edit) => format!("{host} as {edit}"),
        None => format!("{host}"),
    }
}

/// The same reads for 20 problems as for 200, in every language and view.
fn reads_do_not_grow(ranges: bool) {
    let mut failures = Vec::new();
    for host in LANGUAGES {
        for view in views(host) {
            let (few_shown, few) = placed(host, view, 10, ranges);
            let (many_shown, many) = placed(host, view, 100, ranges);
            // Ranges two per item, paths one.
            let expected = |items: usize| if ranges { 2 * items } else { items };
            if few != many || few_shown < expected(10) || many_shown < expected(100) {
                failures.push(format!(
                    "{}: (reads, path lookups, line splits) {few:?} for {few_shown} placed, {many:?} for {many_shown}",
                    name(host, view)
                ));
            }
        }
    }
    report(failures);
}

#[test]
fn host_paths_read_each_text_a_fixed_number_of_times_in_every_language_and_view() {
    reads_do_not_grow(false);
}

#[test]
fn host_ranges_read_each_text_a_fixed_number_of_times_in_every_language_and_view() {
    reads_do_not_grow(true);
}

/// And placing them stays linear: 3000 items written in two languages and
/// their problems placed in 20 to 200 ms here (a debug build; a shared CI
/// runner took 600), where reading the text again for each took minutes.
/// The counts above guard the repeated reads; this ceiling only catches a
/// pass growing many times slower.
#[test]
fn thousands_of_host_problems_are_placed_in_linear_time_in_every_language_and_view() {
    let mut failures = Vec::new();
    for host in LANGUAGES {
        for view in views(host) {
            for ranges in [false, true] {
                let (took, (shown, _)) = best_of(2, || placed(host, view, 3000, ranges));
                let wanted = if ranges { 6000 } else { 3000 };
                if took > Duration::from_millis(2000) || shown < wanted {
                    let kind = if ranges { "ranges" } else { "paths" };
                    failures.push(format!(
                        "{}: {shown} {kind} placed in {took:?}",
                        name(host, view)
                    ));
                }
            }
        }
    }
    report(failures);
}

/// A problem over each line of `text`: block keys, closers and blank lines
/// as well as scalars.
pub(super) fn on_every_line(text: &str) -> Vec<HostDiagnostic> {
    (1..text.split('\n').count())
        .map(|line| {
            HostDiagnostic::at_range(
                TextRange::new(TextPosition::new(line, 1), TextPosition::new(line + 1, 1)),
                Severity::Error,
                format!("line {line}"),
            )
        })
        .collect()
}

/// An edit in an Edit-as view (a line added at the top) with `items` items
/// and a range problem on every line of the host's text, which the edit
/// leaves as it is: what re-placing the problems costs once their fields
/// were found (reads, path lookups, line splits), and how many are placed.
fn replaced_after_an_edit(host: Language, edit: Language, items: usize) -> (usize, [usize; 3]) {
    let value = document(items);
    let host_text = host.serialize(&value).expect("the host's text");
    let shown = edit.serialize(&value).expect("the view's text");
    let problems = on_every_line(&host_text);
    let cache = StoredValue::new(None);
    let place_on = |shown: &str| {
        let found = HostTargets::cached(cache, host.behavior(), &host_text, &problems);
        place(
            edit.behavior(),
            shown,
            &problems,
            RangeText::Host(&found),
            &[],
            default_strings(),
        )
    };
    let before = place_on(&shown);
    let edited = format!("\n{shown}");
    let mut counts = [0; 3];
    let ((after, lookups), reads) = counted(Work::DocumentRead, || {
        counted(Work::PathLookup, || {
            let (after, splits) = counted(Work::LineSplit, || place_on(&edited));
            counts[2] = splits;
            after
        })
    });
    counts[0] = reads;
    counts[1] = lookups;
    // The same problems, one line down.
    let lines = |placed: &[Diagnostic]| -> Vec<Option<usize>> {
        placed.iter().map(|problem| problem.line).collect()
    };
    let moved: Vec<Option<usize>> = lines(&before)
        .into_iter()
        .map(|line| line.map(|line| line + 1))
        .collect();
    assert_eq!(
        lines(&after),
        moved,
        "{host} as {edit}: placed after the edit"
    );
    (
        after.iter().filter(|shown| shown.line.is_some()).count(),
        counts,
    )
}

/// An edit in an Edit-as view re-places the host's range problems on the
/// text on screen only: the fields they start in, read from the host's
/// text, are kept (5000 ranges read the host's text again after every key,
/// over a second each time), so nothing reads it from the top and the reads
/// do not grow with the problems.
#[test]
fn an_edit_in_a_view_does_not_read_the_hosts_text_again_for_its_ranges() {
    let mut failures = Vec::new();
    for host in LANGUAGES {
        for edit in LANGUAGES.into_iter().filter(|edit| *edit != host) {
            let (few_shown, few) = replaced_after_an_edit(host, edit, 10);
            let (many_shown, many) = replaced_after_an_edit(host, edit, 100);
            if few != many || few[1] != 0 || many_shown <= few_shown {
                failures.push(format!(
                    "{host} as {edit}: (reads, path lookups, line splits) {few:?} for {few_shown} placed, {many:?} for {many_shown}"
                ));
            }
        }
    }
    report(failures);
}
