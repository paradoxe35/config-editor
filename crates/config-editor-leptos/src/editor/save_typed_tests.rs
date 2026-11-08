//! Numbers and booleans typed in a spelling of their own in another view
//! (`2.50`, `2e3`, `007`, `+6`, `False`): the host's type decides what the
//! text is, and the host keeps the spelling where it reads it the same.

use super::Language;
use super::bridge::Bridge;
use super::codec;
use super::save_matrix::{ALL, from_every_view, report};
use serde_json::Value;

/// One host per language holding a float, an integer and a boolean.
fn hosts() -> [(Language, &'static str); 5] {
    [
        (Language::YAML, "f: 1.50 # f\ni: 5\nb: true\nn: x\n"),
        (
            Language::JSON,
            "{\n  \"f\": 1.50,\n  \"i\": 5,\n  \"b\": true,\n  \"n\": \"x\"\n}\n",
        ),
        (Language::TOML, "f = 1.50 # f\ni = 5\nb = true\nn = \"x\"\n"),
        (Language::HCL, "f = 1.50 # f\ni = 5\nb = true\nn = \"x\"\n"),
        (
            Language::XML,
            "<c>\n  <f>1.50</f> <!-- f -->\n  <i>5</i>\n  <b>true</b>\n  <n>x</n>\n</c>\n",
        ),
    ]
}

/// `text` with the value at `pointer` replaced by `typed`.
fn typed_over(language: Language, text: &str, pointer: &str, typed: &str) -> Option<String> {
    let span = language.behavior().value_span(text, pointer)?;
    let range = codec::byte_range(text, span)?;
    let mut typed_text = text.to_string();
    typed_text.replace_range(range, typed);
    Some(typed_text)
}

/// The value's pointer in the host and every view: an XML host holds
/// everything under its root element.
fn in_host(host: Language, key: &str) -> String {
    if host == Language::XML {
        format!("/c/{key}")
    } else {
        format!("/{key}")
    }
}

#[test]
fn numbers_and_booleans_typed_in_their_own_spelling_keep_the_host_type_and_spelling() {
    let cases: [(&str, &str, Value); 11] = [
        ("f", "2.50", 2.5.into()),
        ("f", "0.10", 0.1.into()),
        ("f", "2e3", 2000.0.into()),
        ("f", "2.0e3", 2000.0.into()),
        ("f", "2", 2.0.into()),
        ("i", "007", 7.into()),
        ("i", "+6", 6.into()),
        ("i", "6", 6.into()),
        ("b", "False", false.into()),
        ("b", "TRUE", true.into()),
        ("b", "false", false.into()),
    ];
    let mut failures = Vec::new();
    let mut tried = 0;
    for (host, host_text) in hosts() {
        for edit in ALL.into_iter().filter(|edit| *edit != host) {
            let bridge = Bridge::new(host, edit, None).unwrap();
            let shown = bridge.to_edit(host_text).unwrap().text;
            for (key, typed, expected) in &cases {
                let view_pointer = in_host(host, key);
                let Some(edited) = typed_over(edit, &shown, &view_pointer, typed) else {
                    failures.push(format!("{host} via {edit}: no {view_pointer} in\n{shown}"));
                    continue;
                };
                // Spellings the view itself cannot read are not typed there.
                let Ok(read) = codec::read(edit.behavior(), &edited, None) else {
                    continue;
                };
                let typed_value = read.pointer(&view_pointer);
                if edit != Language::XML
                    && !typed_value.is_some_and(|value| value.is_number() || value.is_boolean())
                {
                    continue;
                }
                tried += 1;
                let case = format!("{host} host via {edit}: {key} typed {typed}");
                let saved = match bridge.to_host(&edited, host_text) {
                    Ok(saved) => saved.text,
                    Err(error) => {
                        failures.push(format!("{case}: waits: {error:?}"));
                        continue;
                    }
                };
                let pointer = in_host(host, key);
                let value = codec::read(host.behavior(), &saved, None)
                    .ok()
                    .and_then(|value| value.pointer(&pointer).cloned());
                // The host's type where the view cannot tell: a float stays
                // a float (`2` typed in XML or HCL is 2.0); a view that can
                // writes what was typed.
                let typed_integer = Value::from(2);
                let expected = if *typed == "2" && !matches!(edit.label(), "XML" | "HCL") {
                    &typed_integer
                } else {
                    expected
                };
                // HCL holds no whole float, and XML holds `1.50` as text.
                let same = match (&value, expected) {
                    (Some(Value::Number(saved)), Value::Number(expected)) => {
                        saved.as_f64() == expected.as_f64()
                            && (saved.is_f64() == expected.is_f64()
                                || matches!(host.label(), "HCL" | "XML"))
                    }
                    (Some(Value::String(saved)), Value::Number(expected))
                        if host == Language::XML =>
                    {
                        saved.parse::<f64>().ok() == expected.as_f64()
                    }
                    (Some(saved), expected) => saved == expected,
                    _ => false,
                };
                if !same {
                    failures.push(format!("{case}: saved {value:?}, not {expected}:\n{saved}"));
                    continue;
                }
                // The spelling, where the host reads it as the same value.
                let spelled = host
                    .behavior()
                    .value_span(&saved, &pointer)
                    .and_then(|span| codec::byte_range(&saved, span))
                    .map(|range| saved[range].trim().to_string());
                if expected.is_number()
                    && host.behavior().scalar_reads_as(typed, expected)
                    && spelled.as_deref() != Some(*typed)
                {
                    failures.push(format!("{case}: written {spelled:?}:\n{saved}"));
                }
                // The comment on the float's line stays.
                if !saved.contains(if host == Language::XML {
                    "<!-- f -->"
                } else {
                    "# f"
                }) && host != Language::JSON
                {
                    failures.push(format!("{case}: lost the comment:\n{saved}"));
                }
            }
        }
    }
    assert!(tried > 100, "only {tried} cases tried");
    report(failures);
}

/// A YAML string tagged `!!str` stays plain when edited to text that would
/// otherwise read as a number or boolean, in a block and a flow list.
#[test]
fn a_yaml_str_tagged_value_edited_from_every_view_stays_plain() {
    let mut failures = Vec::new();
    let host = "s: !!str 1  # c\nl: [!!str 1, x]\nn: 1\n";
    from_every_view(
        &mut failures,
        "!!str",
        Language::YAML,
        host,
        &|value| {
            value["s"] = "2".into();
            value["l"][0] = "true".into();
        },
        "s: !!str 2  # c\nl: [!!str true, x]\nn: 1\n",
    );
    report(failures);
}
