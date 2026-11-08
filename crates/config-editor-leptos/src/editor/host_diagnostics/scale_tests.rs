//! Thousands of host problems on a long document, in every language: placed
//! and merged with the editor's own in one split of the text and in time
//! growing with the text and the problems, not with their product, as this
//! runs again after each pause in typing (QA 20: 3000 ranges on 6000 lines
//! took a quarter of a second per edit, each range splitting the text again
//! and each problem compared with every other).

use std::time::Duration;

use super::super::Language;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::paths::{TextPosition, TextRange};
use super::super::test_timing::best_of;
use super::super::work_count::{Work, counted};
use super::{HostDiagnostic, RangeText, merge, place};
use crate::strings::default_strings;

/// Items in the document (4500 to 6000 lines).
const ITEMS: usize = 1500;

/// Host problems on it: enough that comparing each with every other (or
/// each of the editor's own with every host problem) takes many times what
/// placing them in one pass does.
const PROBLEMS: usize = 20_000;

/// A long document in `language`: `items` blocks of two fields.
fn document(language: Language, items: usize) -> String {
    let items = 0..items;
    if language == Language::YAML {
        "items:\n".to_string()
            + &items
                .map(|i| format!("  i{i}:\n    n: {i}\n    s: \"x{i}\"\n"))
                .collect::<String>()
    } else if language == Language::JSON {
        format!(
            "{{\n  \"items\": {{\n{}\n  }}\n}}\n",
            items
                .map(|i| format!(
                    "    \"i{i}\": {{\n      \"n\": {i},\n      \"s\": \"x{i}\"\n    }}"
                ))
                .collect::<Vec<_>>()
                .join(",\n")
        )
    } else if language == Language::TOML {
        items
            .map(|i| format!("[items.i{i}]\nn = {i}\ns = \"x{i}\"\n"))
            .collect::<Vec<_>>()
            .join("\n")
    } else if language == Language::HCL {
        format!(
            "items {{\n{}}}\n",
            items
                .map(|i| format!("  i{i} {{\n    n = {i}\n    s = \"x{i}\"\n  }}\n"))
                .collect::<String>()
        )
    } else {
        format!(
            "<items>\n{}</items>\n",
            items
                .map(|i| format!("  <i{i}>\n    <n>{i}</n>\n    <s>x{i}</s>\n  </i{i}>\n"))
                .collect::<String>()
        )
    }
}

/// How long placing `count` problems over three lines (on every other line
/// of a document of `items`, around again) and merging them with a schema
/// problem for every four takes, how many are shown, and how many times the
/// text was split into lines.
fn placed_and_merged(language: Language, items: usize, count: usize) -> (Duration, usize, usize) {
    let severities = [
        Severity::Error,
        Severity::Warning,
        Severity::Info,
        Severity::Hint,
    ];
    let text = document(language, items);
    let lines = text.split('\n').count();
    let problems: Vec<HostDiagnostic> = (0..count)
        .map(|i| {
            let line = 2 + (i * 2) % (lines - 5);
            HostDiagnostic::at_range(
                TextRange::new(TextPosition::new(line, 1), TextPosition::new(line + 2, 1)),
                severities[i % 4],
                format!("host {i}"),
            )
        })
        .collect();
    let own: Vec<Diagnostic> = (0..count / 4)
        .map(|i| Diagnostic {
            line: Some(3 + (i * 6) % (lines - 5)),
            column: Some(5),
            end_column: Some(9),
            message: "expected an integer".into(),
            ..Diagnostic::default()
        })
        .collect();
    let behavior = language.behavior();
    let run = || {
        let host = place(
            behavior,
            &text,
            &problems,
            RangeText::OnScreen,
            default_strings(),
        );
        merge(&own, &host)
    };
    let (shown, splits) = counted(Work::LineSplit, run);
    let (took, _) = best_of(5, run);
    (
        took,
        shown.iter().filter(|shown| shown.line.is_some()).count(),
        splits,
    )
}

#[test]
fn thousands_of_host_ranges_are_placed_in_one_split_and_linear_time_in_every_language() {
    let mut failures = Vec::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        let (took, shown, splits) = placed_and_merged(language, ITEMS, PROBLEMS);
        // One split for every range (it was one per range), and the rest
        // linear: about 20 ms here, where comparing each problem with every
        // other took 400 ms, each of the editor's with every host one 280.
        if splits > 1 || took > Duration::from_millis(100) || shown < PROBLEMS {
            failures.push(format!(
                "{}: {shown} shown in {took:?}, the text split {splits} times",
                language.info().label
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
