//! Infinities and NaN through every language pair: parsed as numbers,
//! written back as numbers where the language has them, reported as losses
//! (never `null`) where it has not, kept by Edit-as saves, and numbers to
//! schema validation.

use serde_json::Value;

use super::super::bridge::Bridge;
use super::super::codec::{self, float_of};
use super::super::diagnostics::build_schema;
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
                    let read = host.parse(&saved.text).expect("reads");
                    if !non_finite(&read) || read["n"] != 2 {
                        failures.push(format!(
                            "{host} as {edit}: {edited:?} saved {:?}",
                            saved.text
                        ));
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
