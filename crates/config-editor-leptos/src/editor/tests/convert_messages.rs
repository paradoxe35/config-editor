//! Round 25 editing LOWs on reading and converting: plain messages placed
//! where they say, the same error for the same number however it is
//! signed, and the losses a conversion lists agreeing across targets.

use crate::editor::Language;
use crate::editor::save_matrix::{ALL, report};
use serde_json::json;

/// L22 L7: a YAML key that is a list or a map is one plain error, on its
/// `?` (or where the flow key starts), not serde's "invalid type".
#[test]
fn a_yaml_key_that_is_a_list_or_a_map_is_said_plainly() {
    let cases = [
        ("? [a, b]\n: 1\n", 1, 1, "list"),
        ("x:\n  ? [a, b]\n  : 1\n", 2, 3, "list"),
        ("- ? {k: v}\n  : 1\n", 1, 3, "map"),
    ];
    let mut failures = Vec::new();
    for (text, line, column, kind) in cases {
        match Language::YAML.parse(text) {
            Err(error)
                if error.line == Some(line)
                    && error.column == Some(column)
                    && error.message.starts_with(&format!("this key is a {kind}"))
                    && !error.message.contains("invalid type") => {}
            other => failures.push(format!("{text:?}: {other:?}")),
        }
    }
    report(failures);
}

/// A YAML integer past 64 bits is an error negative as positive, each
/// naming its bound (it used to read as the string
/// `-99999999999999999999`); quoted, it is a string.
#[test]
fn a_wide_negative_yaml_integer_is_the_error_a_wide_positive_one_is() {
    let mut failures = Vec::new();
    for text in [
        "a: -99999999999999999999\n",
        "l:\n  - -99999999999999999999\n",
        "f: [1, -99999999999999999999]\n",
    ] {
        let positive =
            Language::YAML.parse(&text.replace("-99999999999999999999", "99999999999999999999"));
        match (Language::YAML.parse(text), positive) {
            (Err(error), Err(expected))
                if error.message
                    == "the integer -99999999999999999999 is below -9223372036854775808, the smallest 64-bit integer"
                    && expected.message.contains("is above 18446744073709551615")
                    && error.line == expected.line =>
            {
                if error.column.is_none() {
                    failures.push(format!("{text:?}: no column"));
                }
            }
            other => failures.push(format!("{text:?}: {other:?}")),
        }
    }
    for text in [
        "a: \"-99999999999999999999\"\n",
        "a: '-99999999999999999999'\n",
    ] {
        match Language::YAML.parse(text) {
            Ok(value) if value == json!({"a": "-99999999999999999999"}) => {}
            other => failures.push(format!("{text:?}: {other:?}")),
        }
    }
    report(failures);
}

/// An XML entity that is neither predefined nor a character reference
/// (one its DTD declares, `&e;`, or none does) reads as its literal name, which another
/// language would write as text: the document does not convert, to any
/// target, rather than lose it silently. Predefined and character
/// references convert.
#[test]
fn an_xml_document_with_a_declared_entity_does_not_convert() {
    let declared = "<!DOCTYPE r [<!ENTITY e \"x\">]>\n<r>\n  <a>&e;</a>\n</r>\n";
    let undeclared = "<r>\n  <s>\n    <a>x &bogus; y</a>\n  </s>\n</r>\n";
    let predefined = "<r>\n  <a>&amp; &lt; &#233; &#x41;</a>\n</r>\n";
    let mut failures = Vec::new();
    for target in ALL.into_iter().filter(|target| *target != Language::XML) {
        for (text, entity, at) in [(declared, "&e;", "/r/a"), (undeclared, "&bogus;", "/r/s/a")] {
            match Language::XML.convert(text, target) {
                Err(error)
                    if error.to_string().contains(entity)
                        && error.to_string().contains(&format!("(at {at})")) => {}
                other => failures.push(format!("{text:?} to {target}: {other:?}")),
            }
        }
        match Language::XML.convert(predefined, target) {
            Ok(conversion) if conversion.losses.is_empty() => {}
            other => failures.push(format!("predefined to {target}: {other:?}")),
        }
    }
    report(failures);
}

/// L25: whole floats (`1.0`, `0.0`) and a float that underflows to zero
/// (`1e-400`, read as `0.0`) convert to every target without a loss: HCL
/// reads a float written `1.0` as the float, as the others do.
#[test]
fn whole_and_underflowing_floats_convert_to_every_target_without_losses() {
    let sources = [
        (
            Language::JSON,
            "{\"a\": 1.0, \"b\": 0.0, \"c\": 1e-400, \"d\": 1.5, \"l\": [2.0, -3.0], \"e\": 1e3}\n",
        ),
        (
            Language::YAML,
            "a: 1.0\nb: 0.0\nc: 1e-400\nd: 1.5\nl: [2.0, -3.0]\ne: 1.0e3\n",
        ),
        (
            Language::TOML,
            "a = 1.0\nb = 0.0\nc = 1e-400\nd = 1.5\nl = [2.0, -3.0]\ne = 1e3\n",
        ),
        (
            Language::HCL,
            "a = 1.0\nb = 0.0\nc = 1e-400\nd = 1.5\nl = [2.0, -3.0]\ne = 1e3\n",
        ),
    ];
    let mut failures = Vec::new();
    for (from, text) in sources {
        let value = from.parse(text).unwrap();
        for key in ["a", "b", "c", "e"] {
            if !value[key].is_f64() {
                failures.push(format!("{from} reads {key} as {}", value[key]));
            }
        }
        for target in ALL {
            match from.convert(text, target) {
                // XML holds the array's items as repeated elements and
                // reads them back: the floats still are no loss.
                Ok(conversion) if conversion.losses.is_empty() => {}
                other => failures.push(format!("{from} to {target}: {other:?}")),
            }
        }
    }
    // An integer stays an integer.
    if !Language::HCL.parse("n = 1\n").unwrap()["n"].is_u64() {
        failures.push("HCL reads 1 as a float".into());
    }
    report(failures);
}

/// A key with a prefix (an attribute in XML) at the top of a document
/// converted to XML is refused in plain words naming the key as written,
/// as it is nested: never an `@` the user did not write.
#[test]
fn a_prefixed_key_at_the_top_is_refused_naming_the_key_as_written() {
    let mut failures = Vec::new();
    for (source, key) in [
        ("{\"key:colon\": 3, \"nest\": {\"a\": 1}}", "key:colon"),
        ("{\"@x\": 3, \"nest\": {\"a\": 1}}", "@x"),
    ] {
        match Language::JSON.convert(source, Language::XML) {
            Err(error)
                if error
                    .to_string()
                    .contains(&format!("XML keeps {key} only as an attribute"))
                    && error.to_string().ends_with(&format!("(at /{key})"))
                    && (key.starts_with('@') || !error.to_string().contains('@')) => {}
            other => failures.push(format!("{source:?}: {other:?}")),
        }
    }
    report(failures);
}
