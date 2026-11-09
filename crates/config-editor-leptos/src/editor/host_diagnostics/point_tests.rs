//! A host problem given at a position with no end (a point) is never
//! widened from the text around it: on a secret placeholder it marks the
//! whole token, elsewhere one character, at a line's end the place just
//! past it, on an empty line its start; past the last line it is
//! unplaced. While editing as another language it marks the field it is
//! in, as a range does.

use super::super::Language;
use super::super::diagnostics::{Diagnostic, Severity};
use super::super::paths::TextPosition;
use super::tests::DOCUMENTS;
use super::{HostDiagnostic, HostTargets, RangeText, place};
use crate::strings::default_strings;

fn at(line: usize, column: usize) -> HostDiagnostic {
    HostDiagnostic::at_position(TextPosition::new(line, column), Severity::Error, "x")
}

/// `(line, column, end_line, end_column)`, `None` unplaced.
fn position(problem: &Diagnostic) -> Option<(usize, usize, Option<usize>, usize)> {
    Some((
        problem.line?,
        problem.column?,
        problem.end_line,
        problem.end_column?,
    ))
}

fn on_screen(
    language: Language,
    text: &str,
    problem: HostDiagnostic,
    placeholders: &[std::ops::Range<usize>],
) -> Option<(usize, usize, Option<usize>, usize)> {
    let placed = place(
        language.behavior(),
        text,
        &[problem],
        RangeText::OnScreen,
        placeholders,
        default_strings(),
    );
    position(placed.first().expect("one problem"))
}

#[test]
fn a_point_on_a_placeholder_marks_the_whole_token_from_any_column_in_it() {
    let mut failures = Vec::new();
    for token in ["«secret 3f2a91c0»", "{{vault:db-pass}}"] {
        for (language, text) in [
            (Language::YAML, format!("a: 1\nkey: \"{token}\" # é\n")),
            (Language::XML, format!("<a>1</a>\n<key>é{token}</key>\n")),
        ] {
            let start = text.find(token).unwrap();
            let span = start..start + token.len();
            let spans = std::slice::from_ref(&span);
            let line_start = text[..start].rfind('\n').unwrap() + 1;
            let column = text[line_start..start].chars().count() + 1;
            let width = token.chars().count();
            for offset in [0, 1, width / 2, width - 1] {
                let placed = on_screen(language, &text, at(2, column + offset), spans);
                if placed != Some((2, column, None, column + width)) {
                    failures.push(format!(
                        "{language} {token} +{offset}: {placed:?}, the token at 2:{column}"
                    ));
                }
            }
            // Just past it: one character, not the token.
            let past = on_screen(language, &text, at(2, column + width), spans);
            if past != Some((2, column + width, None, column + width + 1)) {
                failures.push(format!("{language} {token} past it: {past:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_point_elsewhere_marks_one_character_at_a_line_end_and_on_an_empty_line_too() {
    let text = "a: 1\n\nname: long value\n";
    let one = |line, column| on_screen(Language::YAML, text, at(line, column), &[]);
    // On a character: that one (not the value, not the line).
    assert_eq!(one(3, 7), Some((3, 7, None, 8)));
    // At the line's end: the place just past it.
    assert_eq!(one(3, 17), Some((3, 17, None, 18)));
    // Past the line's end: read as its end.
    assert_eq!(one(1, 40), Some((1, 5, None, 6)));
    // An empty line: its start.
    assert_eq!(one(2, 1), Some((2, 1, None, 2)));
    assert_eq!(one(2, 9), Some((2, 1, None, 2)));
    // The empty line after the final line break is a line.
    assert_eq!(one(4, 1), Some((4, 1, None, 2)));
    // Past the last line: unplaced (the footer lists it).
    assert_eq!(one(9, 1), None);
}

/// While editing as another language a point marks the field written
/// where it is in the host's text, as a range starting there does.
#[test]
fn a_point_marks_its_field_while_editing_as_another_language() {
    let mut failures = Vec::new();
    for (host, host_text) in DOCUMENTS {
        let line = host_text
            .split('\n')
            .position(|text| text.contains("7835"))
            .expect("the listen line")
            + 1;
        let column = host_text
            .split('\n')
            .nth(line - 1)
            .unwrap()
            .find("7835")
            .unwrap()
            + 1;
        let problems = [at(line, column)];
        let ranges = [HostDiagnostic::at_range(
            super::super::paths::TextRange::new(
                TextPosition::new(line, column),
                TextPosition::new(line, column + 4),
            ),
            Severity::Error,
            "x",
        )];
        for (edit, shown) in DOCUMENTS.into_iter().filter(|(edit, _)| *edit != host) {
            let placed_with = |problems: &[HostDiagnostic]| {
                let found = HostTargets::new(host.behavior(), host_text, problems);
                place(
                    edit.behavior(),
                    shown,
                    problems,
                    RangeText::Host(&found),
                    &[],
                    default_strings(),
                )
                .first()
                .and_then(position)
            };
            let (point, range) = (placed_with(&problems), placed_with(&ranges));
            if point.is_none() || point != range {
                failures.push(format!(
                    "{host} as {edit}: point {point:?}, range {range:?}"
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
