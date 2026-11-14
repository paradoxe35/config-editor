//! Infinities and NaN through every language pair: parsed as numbers,
//! written back as numbers where the language has them, reported as losses
//! (never `null`) where it has not, kept by Edit-as saves, and numbers to
//! schema validation.

use serde_json::Value;

use super::super::bridge::Bridge;
use super::super::codec::{self, float_of};
use super::Language;

/// `rate` holds each non-finite number in the languages that have them.
fn sources() -> Vec<(Language, &'static str)> {
    vec![
        (Language::YAML, "rate: .inf\nlow: -.inf\nodd: .nan\nn: 1\n"),
        (Language::YAML, "rate: .Inf\nlow: -.INF\nodd: .NaN\nn: 1\n"),
        (Language::TOML, "rate = inf\nlow = -inf\nodd = nan\nn = 1\n"),
        (
            Language::TOML,
            "rate = +inf\nlow = -inf\nodd = -nan\nn = 1\n",
        ),
    ]
}

fn targets() -> [Language; 5] {
    [
        Language::YAML,
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::XML,
    ]
}

/// The three values, as numbers.
fn numbers(value: &Value) -> Option<(f64, f64, f64)> {
    Some((
        float_of(value.pointer("/rate")?)?,
        float_of(value.pointer("/low")?)?,
        float_of(value.pointer("/odd")?)?,
    ))
}

fn non_finite(value: &Value) -> bool {
    numbers(value).is_some_and(|(rate, low, odd)| {
        rate == f64::INFINITY && low == f64::NEG_INFINITY && odd.is_nan()
    })
}

#[test]
fn non_finite_numbers_survive_every_conversion_or_are_reported_lost() {
    let mut failures = Vec::new();
    for (from, source) in sources() {
        let parsed = from.parse(source).expect("parses");
        if !non_finite(&parsed) {
            failures.push(format!("{from} {source:?} parsed as {parsed}"));
            continue;
        }
        for to in targets() {
            let converted = match from.convert(source, to) {
                Ok(converted) => converted,
                Err(error) => {
                    failures.push(format!("{from} → {to}: {error}"));
                    continue;
                }
            };
            let text = &converted.text;
            if text.contains('\u{FDD0}') || text.contains("null") {
                failures.push(format!("{from} → {to}: wrote {text:?}"));
            }
            let back = to.parse(text).expect("reads back");
            let holds = matches!(to.label(), "YAML" | "TOML");
            if holds {
                if !non_finite(&back) || !converted.losses.is_empty() {
                    failures.push(format!(
                        "{from} → {to}: {text:?} reads {back}, losses {:?}",
                        converted.losses
                    ));
                }
            } else if converted.losses != ["/rate", "/low", "/odd"] {
                failures.push(format!(
                    "{from} → {to}: {text:?} losses {:?}",
                    converted.losses
                ));
            }
            if to.serialize(&parsed).is_ok() != holds {
                failures.push(format!("{from} → {to}: serialize disagrees with convert"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A host holding infinities, edited as every language: an edit elsewhere
/// keeps them, and an infinity typed where the host has no number for it
/// waits instead of saving text.
#[test]
fn edit_as_saves_keep_non_finite_numbers() {
    let mut failures = Vec::new();
    for (host, source) in sources() {
        for edit in targets() {
            let Some(bridge) = Bridge::new(host, edit, None) else {
                continue;
            };
            let shown = bridge.to_edit(source).expect("shows").text;
            let edited = shown.replacen('1', "2", 1);
            match bridge.to_host(&edited, source) {
                Ok(saved) => {
                    let read = host.parse(&saved).expect("reads");
                    if !non_finite(&read) || read["n"] != 2 {
                        failures.push(format!("{host} as {edit}: {edited:?} saved {:?}", saved));
                    }
                }
                Err(error) => failures.push(format!("{host} as {edit}: {error:?}")),
            }
        }
    }
    // Typed in YAML for a host without infinities: the save waits.
    for host in [Language::JSON, Language::HCL, Language::XML] {
        let source = host.serialize(&serde_json::json!({"n": 1})).unwrap();
        let bridge = Bridge::new(host, Language::YAML, None).unwrap();
        if bridge.to_host("n: .inf\n", &source).is_ok() {
            failures.push(format!("{host}: an infinity saved"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[cfg(feature = "schema-validation")]
#[test]
fn schema_validation_reads_non_finite_numbers_as_numbers() {
    use super::super::diagnostics::build_schema;
    let numbers = build_schema(
        r#"{"properties": {"rate": {"type": "number"}, "low": {"type": "number"}, "odd": {"type": "number"}}}"#,
    )
    .unwrap();
    let strings = build_schema(r#"{"properties": {"rate": {"type": "string"}}}"#).unwrap();
    let mut failures = Vec::new();
    for (language, source) in sources() {
        let problems = super::super::diagnostics::validate(
            source,
            Some(&numbers),
            None,
            true,
            language.behavior(),
        );
        if !problems.is_empty() {
            failures.push(format!("{language} as numbers: {problems:?}"));
        }
        let problems = super::super::diagnostics::validate(
            source,
            Some(&strings),
            None,
            true,
            language.behavior(),
        );
        let worded = problems.first().map(|problem| problem.message.clone());
        if worded.as_deref() != Some("/rate: inf is not of type \"string\"") {
            failures.push(format!("{language} as a string: {worded:?}"));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn non_finite_numbers_are_never_null_in_the_data_model() {
    for (language, source) in sources() {
        let value = codec::read(language.behavior(), source, None).unwrap();
        assert!(
            codec::first_pointer(&value, &Value::is_null).is_none(),
            "{language}: {value}"
        );
    }
}

/// Real strings spelled like the reserved form, in every language: one
/// mark, two marks, and the JSON escape of one.
fn forged() -> Vec<(Language, &'static str)> {
    vec![
        (
            Language::YAML,
            "s: \"\u{FDD0}inf\"\nt: \u{FDD0}\u{FDD0}nan\nn: 1\n",
        ),
        (
            Language::JSON,
            "{\n  \"s\": \"\\ufdd0inf\",\n  \"t\": \"\u{FDD0}\u{FDD0}nan\",\n  \"n\": 1\n}\n",
        ),
        (
            Language::TOML,
            "s = \"\u{FDD0}inf\"\nt = \"\u{FDD0}\u{FDD0}nan\"\nn = 1\n",
        ),
        (
            Language::HCL,
            "s = \"\u{FDD0}inf\"\nt = \"\u{FDD0}\u{FDD0}nan\"\nn = 1\n",
        ),
        (
            Language::XML,
            "<s>\u{FDD0}inf</s>\n<t>\u{FDD0}\u{FDD0}nan</t>\n<n>1</n>\n",
        ),
    ]
}

/// Whether `value` holds the two forged strings as strings.
fn holds_forged(value: &Value) -> bool {
    let text = |pointer| value.pointer(pointer).and_then(codec::string_of);
    float_of(&value["s"]).is_none()
        && text("/s") == Some("\u{FDD0}inf")
        && text("/t") == Some("\u{FDD0}\u{FDD0}nan")
}

/// A real string spelled like the reserved form of a non-finite number
/// stays that string through parse, convert, serialize, Edit-as saves and
/// validation, in every language pair.
#[test]
fn strings_spelled_like_the_reserved_form_stay_strings() {
    let mut failures = Vec::new();
    for (from, source) in forged() {
        let parsed = from.parse(source).expect("parses");
        if !holds_forged(&parsed) {
            failures.push(format!("{from}: parsed as {parsed}"));
            continue;
        }
        for to in targets() {
            match from.convert(source, to) {
                Ok(converted) => {
                    let back = to.parse(&converted.text).expect("reads back");
                    if !converted.losses.is_empty() || !holds_forged(&back) {
                        failures.push(format!(
                            "{from} → {to}: {:?} reads {back}, losses {:?}",
                            converted.text, converted.losses
                        ));
                    }
                }
                Err(error) => failures.push(format!("{from} → {to}: {error}")),
            }
            match to.serialize(&parsed) {
                Ok(text) if to.parse(&text).is_ok_and(|back| holds_forged(&back)) => {}
                other => failures.push(format!("{from} → {to}: serialize gave {other:?}")),
            }
            if let Some(bridge) = Bridge::new(from, to, None) {
                let shown = bridge.to_edit(source).expect("shows").text;
                match bridge.to_host(&shown.replacen('1', "2", 1), source) {
                    Ok(saved) => {
                        let read = from.parse(&saved).expect("reads");
                        if !holds_forged(&read) || read["n"] != 2 {
                            failures.push(format!("{from} as {to}: saved {:?}", saved));
                        }
                    }
                    Err(error) => failures.push(format!("{from} as {to}: {error:?}")),
                }
            }
        }
        #[cfg(feature = "schema-validation")]
        {
            use super::super::diagnostics::build_schema;
            let strings = build_schema(
                r#"{"properties": {"s": {"type": "string", "maxLength": 4}, "t": {"type": "string", "maxLength": 5}}}"#,
            )
            .unwrap();
            let problems = super::super::diagnostics::validate(
                source,
                Some(&strings),
                None,
                true,
                from.behavior(),
            );
            if !problems.is_empty() {
                failures.push(format!("{from}: validation {problems:?}"));
            }
        }
    }
    // Built with `string_value`, a host's string is written as it is.
    let value = serde_json::json!({
        "s": codec::string_value("\u{FDD0}inf"),
        "t": codec::string_value("\u{FDD0}\u{FDD0}nan"),
    });
    for to in targets() {
        match to.serialize(&value) {
            Ok(text)
                if text.contains("\u{FDD0}inf") && !text.contains("\u{FDD0}\u{FDD0}\u{FDD0}") => {}
            other => failures.push(format!("{to}: serialize of string_value gave {other:?}")),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Infinities and NaN against each numeric keyword: an infinity is beyond
/// every bound, NaN beyond every bound on both sides, and neither is an
/// integer or a multiple of anything (round 16 let them pass `integer`,
/// `multipleOf` and, for NaN, `minimum`). Finite numbers keep their results.
#[cfg(feature = "schema-validation")]
#[test]
fn non_finite_numbers_fail_integer_multiple_of_and_bounds() {
    use super::super::diagnostics::build_schema;
    let keywords = [
        (r#"{"type": "integer"}"#, [true, true, true]),
        (r#"{"multipleOf": 5}"#, [true, true, true]),
        (r#"{"multipleOf": 0.5}"#, [true, true, true]),
        (r#"{"minimum": 0}"#, [false, true, true]),
        (r#"{"maximum": 100}"#, [true, false, true]),
        (r#"{"exclusiveMinimum": 0}"#, [false, true, true]),
        (r#"{"exclusiveMaximum": 100}"#, [true, false, true]),
        (r#"{"type": "number"}"#, [false, false, false]),
    ];
    let mut failures = Vec::new();
    for (language, source) in sources() {
        for (keyword, fails) in keywords {
            let schema = build_schema(&format!(
                r#"{{"properties": {{"rate": {keyword}, "low": {keyword}, "odd": {keyword}}}}}"#
            ))
            .unwrap();
            let problems = super::super::diagnostics::validate(
                source,
                Some(&schema),
                None,
                true,
                language.behavior(),
            );
            for (field, fails) in ["rate", "low", "odd"].into_iter().zip(fails) {
                let found: Vec<_> = problems
                    .iter()
                    .filter(|problem| problem.message.starts_with(&format!("/{field}: ")))
                    .collect();
                let worded = found.iter().all(|problem| {
                    problem.message.contains("inf") || problem.message.contains("nan")
                });
                if found.is_empty() == fails || found.len() > 1 || !worded {
                    failures.push(format!("{language} {keyword} /{field}: {found:?}"));
                }
            }
            // `n: 1` keeps the result it always had.
            let one = problems
                .iter()
                .filter(|problem| problem.message.starts_with("/n: "))
                .count();
            if one != 0 {
                failures.push(format!("{language} {keyword}: /n flagged"));
            }
        }
    }
    // Without a non-finite number, validation is as before.
    let schema = build_schema(r#"{"properties": {"n": {"multipleOf": 5, "maximum": 3}}}"#).unwrap();
    let problems = super::super::diagnostics::validate(
        "n: 10\n",
        Some(&schema),
        None,
        true,
        Language::YAML.behavior(),
    );
    if problems.len() != 1 || !problems[0].message.contains("10") {
        failures.push(format!("finite: {problems:?}"));
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// A number too large to hold (a float past the largest, an integer past
/// 64 bits, either sign) is underlined whole in every language that reads
/// it as a number, wherever the parser stopped in it (QA 32: JSON's
/// `1e400` only on its last `0`, HCL's on `e400`).
#[test]
fn a_number_too_large_is_underlined_whole_in_every_language() {
    let mut failures = Vec::new();
    let literals = [
        "1e400",
        "-1e400",
        "1.5e999",
        "99999999999999999999999",
        "-99999999999999999999999",
    ];
    for language in [
        Language::JSON,
        Language::TOML,
        Language::HCL,
        Language::YAML,
    ] {
        for literal in literals {
            // YAML reads an overflowing float as an infinity.
            if language == Language::YAML && literal.contains('e') {
                continue;
            }
            let (source, line) = match language.label() {
                "JSON" => (format!("{{\n  \"a\": 1,\n  \"b\": {literal}\n}}\n"), 3),
                "YAML" => (format!("a: 1\nb: {literal}\n"), 2),
                _ => (format!("a = 1\nb = {literal}\n"), 2),
            };
            let problems =
                super::super::diagnostics::validate(&source, None, None, true, language.behavior());
            let text: Vec<char> = source.split('\n').nth(line - 1).unwrap().chars().collect();
            let marked = problems.iter().find_map(|problem| {
                let (column, end) = (problem.column?, problem.end_column?);
                (problem.line == Some(line)).then(|| {
                    text[column - 1..(end - 1).min(text.len())]
                        .iter()
                        .collect::<String>()
                })
            });
            if marked.as_deref() != Some(literal) {
                failures.push(format!("{language} {literal}: {marked:?} ({problems:?})"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failing:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// A YAML plain number too large for a float (`1e400`) is an infinity, as
/// the float syntax says, not a string; quoted it stays a string.
#[test]
fn an_overflowing_yaml_float_is_an_infinity() {
    let value = Language::YAML
        .parse("a: 1e400\nb: -1E+400\nc: \"1e400\"\nd: '1e400'\n")
        .unwrap();
    assert_eq!(float_of(&value["a"]), Some(f64::INFINITY), "{value:?}");
    assert_eq!(float_of(&value["b"]), Some(f64::NEG_INFINITY), "{value:?}");
    assert_eq!(value["c"], Value::String("1e400".into()));
    assert_eq!(value["d"], Value::String("1e400".into()));
    // TOML and JSON read it as a number too (one too large: an error).
    assert!(Language::TOML.parse("a = 1e400\n").is_err());
    assert!(Language::JSON.parse("{\"a\": 1e400}").is_err());
}

/// M23-E3: an integer past 64 bits is a syntax error in every language
/// with integers, JSON too (it read it as a float, silently not the number
/// written), on the number's line; 64-bit ones are exact. XML text holds
/// it as text.
#[test]
fn an_integer_past_64_bits_is_a_syntax_error_in_every_language() {
    let mut failures = Vec::new();
    for (language, past, line, at_edge) in [
        (
            Language::JSON,
            "{\n  \"n\": 1,\n  \"id\": 18446744073709551616\n}\n",
            3,
            "{\"id\": 18446744073709551615, \"low\": -9223372036854775808}",
        ),
        (
            Language::JSON,
            "[\"18446744073709551616\",\n -9223372036854775809]",
            2,
            "[\"x\\\"18446744073709551616\", 1e30, 1.5, 18446744073709551616.0]",
        ),
        (
            Language::YAML,
            "n: 1\nid: 18446744073709551616\n",
            2,
            "id: 9223372036854775807\n",
        ),
        (
            Language::TOML,
            "n = 1\nid = 18446744073709551616\n",
            2,
            "id = 9223372036854775807\n",
        ),
        (
            Language::HCL,
            "n = 1\nid = 18446744073709551616\n",
            2,
            "id = 9223372036854775807\n",
        ),
    ] {
        match language.parse(past) {
            Err(error) if language != Language::JSON || error.line == Some(line) => {}
            other => failures.push(format!("{language} {past:?}: {other:?}")),
        }
        if let Err(error) = language.parse(at_edge) {
            failures.push(format!("{language} {at_edge:?}: {error:?}"));
        }
    }
    assert_eq!(
        Language::JSON
            .parse("{\"id\": 18446744073709551615}")
            .unwrap()["id"]
            .as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(
        Language::XML
            .parse("<id>18446744073709551616</id>")
            .unwrap()["id"],
        Value::String("18446744073709551616".into())
    );
    assert!(failures.is_empty(), "{failures:#?}");
}
