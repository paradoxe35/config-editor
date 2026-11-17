//! Syntax errors: their wording and places, number bounds, and the byte order
//! mark.

use crate::editor::languages::Language;

#[test]
fn an_xml_start_tag_missing_its_bracket_is_reported_on_its_own_line() {
    let source = "<listen>1</listen>\n<insecure_plaintextfalse</insecure_plaintext>\n<pool>\n  <min_idle>4</min_idle>\n</pool>\n";
    let error = Language::XML.parse(source).unwrap_err();
    assert_eq!(error.line, Some(2), "{error}");
}

#[test]
fn hcl_syntax_errors_read_as_a_sentence() {
    let error = Language::HCL
        .parse("listen = 1\nhealth_check : true\n")
        .unwrap_err();
    assert!(
        !error.message.contains("-->") && !error.message.contains('|'),
        "{}",
        error.message
    );
    assert_eq!(error.line, Some(2));
}

#[test]
fn yaml_tab_indentation_is_a_syntax_error_outside_block_scalars() {
    let error = Language::YAML
        .parse("regions:\n  - a\n\t  - b\n")
        .unwrap_err();
    assert_eq!((error.line, error.column), (Some(3), Some(1)));
    assert!(error.message.contains("tabs"), "{}", error.message);
    // Tabs inside a block scalar are text.
    assert!(
        Language::YAML
            .parse("note: |\n  a\n  \tb\nnext: 1\n")
            .is_ok()
    );
    // A tab after the indentation is fine.
    assert!(Language::YAML.parse("a:\t1\n").is_ok());
}

#[test]
fn every_yaml_syntax_error_has_a_place() {
    let problems = crate::check_syntax(Language::YAML, "listen: 1\n---\nlisten: 2\n");
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].line, Some(2), "{}", problems[0].message);
}

#[test]
fn json_errors_point_at_the_problem_without_repeating_the_position() {
    let error = Language::JSON
        .parse("{\n  \"listen\": 7835,\n}")
        .unwrap_err();
    assert_eq!(
        (error.line, error.column),
        (Some(2), Some(17)),
        "{}",
        error.message
    );
    assert!(!error.message.contains(" at line "), "{}", error.message);
}

#[test]
fn xml_refuses_empty_element_names() {
    assert!(
        Language::XML
            .parse("<services>\n  <>\n  </>\n</services>")
            .is_err()
    );
    assert!(
        Language::YAML
            .convert("services: {\"\": {target: 80}}\n", Language::XML)
            .is_err()
    );
}

/// Every language reads the document after a leading byte order mark, and
/// an error on its line is placed where it is.
#[test]
fn a_leading_byte_order_mark_is_no_part_of_the_document() {
    let mut failures = Vec::new();
    for (language, text, broken) in [
        (Language::JSON, "{\"a\": 1}\n", "{\"a\": }\n"),
        (Language::HCL, "a = 1\n", "a = \n"),
        (Language::YAML, "a: 1\n", "a: [\n"),
        (Language::TOML, "a = 1\n", "a = \n"),
        (Language::XML, "<a>1</a>\n", "<a>1</b>\n"),
    ] {
        let marked = format!("\u{FEFF}{text}");
        if language.parse(&marked).ok() != language.parse(text).ok()
            || language.parse(text).is_err()
        {
            failures.push(format!("{language}: {:?}", language.parse(&marked)));
        }
        let plain = language
            .parse(broken)
            .err()
            .map(|error| (error.line, error.column));
        let with_mark = language
            .parse(&format!("\u{FEFF}{broken}"))
            .err()
            .map(|error| {
                let on_its_line = usize::from(error.line == Some(1));
                (error.line, error.column.map(|column| column - on_its_line))
            });
        if language != Language::XML && (plain.is_none() || plain != with_mark) {
            failures.push(format!("{language} error: {plain:?} vs {with_mark:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A number past what a language holds is said plainly, the same
/// way in every language, at the number.
#[test]
fn numbers_out_of_range_are_said_plainly_in_every_language() {
    let mut failures = Vec::new();
    for (language, text, number) in [
        (
            Language::TOML,
            "a = 99999999999999999999\n",
            "99999999999999999999",
        ),
        (
            Language::TOML,
            "a = -99999999999999999999\n",
            "-99999999999999999999",
        ),
        (Language::TOML, "a = 1e400\n", "1e400"),
        (
            Language::YAML,
            "a: 99999999999999999999\n",
            "99999999999999999999",
        ),
        (
            Language::HCL,
            "a = 99999999999999999999\n",
            "99999999999999999999",
        ),
        (Language::HCL, "a = 1e400\n", "1e400"),
        (
            Language::JSON,
            "{\"a\": 99999999999999999999}\n",
            "99999999999999999999",
        ),
        (Language::JSON, "{\"a\": 1e400}\n", "1e400"),
    ] {
        let expected = if number.contains('e') {
            format!("the number {number} is past the largest float")
        } else if number.starts_with('-') {
            format!(
                "the integer {number} is below -9223372036854775808, the smallest 64-bit integer"
            )
        } else {
            format!(
                "the integer {number} is above 18446744073709551615, the largest 64-bit integer"
            )
        };
        match language.parse(text) {
            Err(error) if error.message == expected && error.line == Some(1) => {}
            other => failures.push(format!("{language} {text:?}: {other:?}")),
        }
    }
    // A number in range beside a syntax error keeps the parser's message.
    for (language, text) in [
        (Language::TOML, "a = 12 3\n"),
        (Language::JSON, "{\"a\": 12 3}\n"),
    ] {
        match language.parse(text) {
            Err(error) if !error.message.contains("64-bit") => {}
            other => failures.push(format!("{language} {text:?}: {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Each language reads the 64-bit integers its parser holds, and the error
/// past them names the bound.
#[test]
fn integers_read_to_their_64_bit_bounds_and_the_error_names_the_bound() {
    let below = "the integer -9223372036854775809 is below -9223372036854775808, the smallest 64-bit integer";
    let above = "the integer 18446744073709551616 is above 18446744073709551615, the largest 64-bit integer";
    let signed = "the integer 9223372036854775808 is above 9223372036854775807, the largest signed 64-bit integer";
    let json = |n: &str| format!("{{\"a\": {n}}}\n");
    let yaml = |n: &str| format!("a: {n}\n");
    let assigned = |n: &str| format!("a = {n}\n");
    type Written<'a> = &'a dyn Fn(&str) -> String;
    let languages: [(Language, Written<'_>); 4] = [
        (Language::JSON, &json),
        (Language::YAML, &yaml),
        (Language::TOML, &assigned),
        (Language::HCL, &assigned),
    ];
    let mut failures = Vec::new();
    for (language, write) in languages {
        let unsigned = language != Language::TOML;
        let reads = [
            ("-9223372036854775808", true),
            ("9223372036854775807", true),
            ("18446744073709551615", unsigned),
        ];
        for (number, ok) in reads {
            let read = language.parse(&write(number));
            if read.is_ok() != ok {
                failures.push(format!("{language} {number}: {read:?}"));
            }
        }
        let mut errors = vec![
            ("-9223372036854775809", below),
            ("18446744073709551616", above),
        ];
        if !unsigned {
            errors.push(("9223372036854775808", signed));
        }
        for (number, expected) in errors {
            match language.parse(&write(number)) {
                Err(error) if error.message == expected => {}
                other => failures.push(format!("{language} {number}: {other:?}")),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
