//! Every position the API gives or takes counts lines the same way in a
//! text whose lines end with LF, CRLF or a lone CR: the problems, a
//! field's range, the path at an index, the folds. In every language.

use super::Language;
use super::diagnostics::{Schema, check_syntax};
use super::paths::{FieldPath, TextPosition};

/// A document in `language`, `n` on its third line, its lines ended by
/// `ending`.
fn sample(language: Language, ending: &str) -> String {
    let lines: &[&str] = if language == Language::YAML {
        &["a: 1", "b: 2", "n: 1", ""]
    } else if language == Language::JSON {
        &["{", "\"a\": 1,", "\"n\": 1", "}", ""]
    } else if language == Language::XML {
        &["<r>", "<a>1</a>", "<n>x</n>", "</r>", ""]
    } else {
        &["a = 1", "b = 2", "n = 1", ""]
    };
    lines.join(ending)
}

fn pointer(language: Language) -> &'static str {
    if language == Language::XML {
        "/r/n"
    } else {
        "/n"
    }
}

fn schema(language: Language) -> Schema {
    let text = if language == Language::XML {
        r#"{"properties":{"r":{"properties":{"n":{"type":"integer"}}}}}"#
    } else {
        r#"{"properties":{"n":{"type":"string"}}}"#
    };
    Schema::compile(text).unwrap()
}

#[test]
fn positions_count_lf_crlf_and_lone_cr_alike() {
    let languages = [
        Language::YAML,
        Language::JSON,
        Language::XML,
        Language::TOML,
        Language::HCL,
    ];
    let mut failures = Vec::new();
    for language in languages {
        let lf = sample(language, "\n");
        let problems = |text: &str| -> Vec<(Option<usize>, Option<usize>)> {
            schema(language)
                .validate(language, text)
                .iter()
                .map(|problem| (problem.line, problem.column))
                .collect()
        };
        let expected_problems = problems(&lf);
        let range = |text: &str| {
            language
                .range_of(text, FieldPath::from(pointer(language)))
                .map(|range| (range.start, range.end, range.start.to_offset(text)))
        };
        let expected_range = range(&lf);
        assert!(expected_range.is_some(), "{language}: no range in LF");
        let (start, _, _) = expected_range.unwrap();
        for ending in ["\r\n", "\r"] {
            let text = sample(language, ending);
            let case = format!("{language} {ending:?}");
            let accepted = language.parse(&text).is_ok();
            if accepted && problems(&text) != expected_problems {
                failures.push(format!("{case}: problems at {:?}", problems(&text)));
            }
            if !accepted {
                // A language that rejects a lone CR (TOML, HCL): its
                // reader's error, at the end of the first line one ends.
                let error = language.parse(&text).unwrap_err();
                let syntax = check_syntax(language, &text);
                if error.line != Some(1)
                    || syntax.first().map(|problem| problem.line) != Some(Some(1))
                {
                    failures.push(format!("{case}: rejected at {error:?} / {syntax:?}"));
                }
                continue;
            }
            let got = range(&text).map(|(start, end, _)| (start, end));
            if got != expected_range.map(|(start, end, _)| (start, end)) {
                failures.push(format!("{case}: range {got:?}"));
            }
            let index = start.to_offset(&text);
            if TextPosition::from_offset(&text, index) != start {
                failures.push(format!("{case}: offset {index} reads as another position"));
            }
            let at = language.path_at(&text, start).map(|path| path.to_pointer());
            if at.as_deref() != Some(pointer(language)) {
                failures.push(format!("{case}: path at {start:?} is {at:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
