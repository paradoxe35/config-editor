//! Thousands of host problems on a long document, in every language: placed
//! and merged with the editor's own in time growing with the text and the
//! problems, not with their product, as this runs again after each pause in
//! typing (QA 20: 3000 ranges on 6000 lines took a quarter of a second per
//! edit, each range splitting the text again).

use std::time::Duration;

use super::super::Language;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::paths::{TextPosition, TextRange};
use super::super::test_timing::best_of;
use super::{HostDiagnostic, RangeText, merge, place};
use crate::strings::default_strings;

/// Items in the smaller document (1100 to 1500 lines); the larger has four
/// times as many (4500 to 6000 lines), and four times the problems.
const ITEMS: usize = 375;

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

/// How long placing a problem over three lines every other line of a
/// document of `items` (`items * 2` of them) and merging it with a schema
/// problem on every other item takes, and how many are shown.
fn placed_and_merged(language: Language, items: usize) -> (Duration, usize) {
    let severities = [
        Severity::Error,
        Severity::Warning,
        Severity::Info,
        Severity::Hint,
    ];
    let text = document(language, items);
    let lines = text.split('\n').count();
    let problems: Vec<HostDiagnostic> = (0..items * 2)
        .map(|i| {
            let line = 2 + (i * 2) % (lines - 5);
            HostDiagnostic::at_range(
                TextRange::new(TextPosition::new(line, 1), TextPosition::new(line + 2, 1)),
                severities[i % 4],
                format!("host {i}"),
            )
        })
        .collect();
    let own: Vec<Diagnostic> = (0..items / 2)
        .map(|i| Diagnostic {
            line: Some(3 + i * 6),
            column: Some(5),
            end_column: Some(9),
            message: "expected an integer".into(),
            ..Diagnostic::default()
        })
        .collect();
    let behavior = language.behavior();
    let (took, shown) = best_of(5, || {
        let host = place(
            behavior,
            &text,
            &problems,
            RangeText::OnScreen,
            default_strings(),
        );
        merge(&own, &host)
    });
    (
        took,
        shown.iter().filter(|shown| shown.line.is_some()).count(),
    )
}

#[test]
fn four_times_the_text_and_host_ranges_take_about_four_times_as_long_in_every_language() {
    let mut failures = Vec::new();
    for language in [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ] {
        let (small, _) = placed_and_merged(language, ITEMS);
        let (large, shown) = placed_and_merged(language, ITEMS * 4);
        // Linear: about 4x; it was 16x (each range split the whole text, each
        // problem was compared with every other).
        let ratio = large.as_secs_f64() / small.as_secs_f64().max(1e-6);
        if ratio > 8.0 || shown < ITEMS * 8 {
            failures.push(format!(
                "{}: {small:?} then {large:?} ({ratio:.1}x), {shown} shown",
                language.info().label
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
