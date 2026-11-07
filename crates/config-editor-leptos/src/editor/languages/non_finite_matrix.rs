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
                        let read = from.parse(&saved.text).expect("reads");
                        if !holds_forged(&read) || read["n"] != 2 {
                            failures.push(format!("{from} as {to}: saved {:?}", saved.text));
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
