//! A value under the pointer is the value the parsed document has there,
//! however it is written: with spaces (YAML's `mode: very fast`,
//! a TOML string), or with punctuation inside its quotes (`"a: b"`).

use super::key_hover;
use crate::editor::languages::Language;
use crate::editor::schema::build_schema;

const SCHEMA: &str = r#"{"properties": {
    "mode": {"enum": ["very fast", "slow"],
             "x-enum-descriptions": ["Fast, with a gap", "Careful"]},
    "url": {"enum": ["a: b", "c"], "x-enum-descriptions": ["With a colon", "Plain"]}
}}"#;

/// The value label and its description the card shows at `column` of
/// zero-based `line`.
fn value_shown(language: Language, text: &str, line: usize, column: usize) -> Option<String> {
    let bundle = build_schema(SCHEMA).expect("a schema");
    let hover = key_hover(text, line, column, Some(&bundle), language.behavior())?;
    let value = hover.value?;
    Some(format!(
        "{}={}",
        value.label,
        value.doc.description.unwrap_or_default()
    ))
}

#[test]
fn a_value_with_spaces_or_punctuation_is_one_value() {
    let cases: Vec<(Language, &str, usize, usize, &str)> = vec![
        #[cfg(feature = "yaml")]
        (
            Language::YAML,
            "mode: very fast\n",
            0,
            12,
            "very fast=Fast, with a gap",
        ),
        #[cfg(feature = "yaml")]
        (
            Language::YAML,
            "mode: very fast\n",
            0,
            6,
            "very fast=Fast, with a gap",
        ),
        #[cfg(feature = "yaml")]
        (Language::YAML, "url: \"a: b\"\n", 0, 9, "a: b=With a colon"),
        #[cfg(feature = "json")]
        (
            Language::JSON,
            "{\"url\": \"a: b\"}\n",
            0,
            12,
            "a: b=With a colon",
        ),
        #[cfg(feature = "json")]
        (
            Language::JSON,
            "{\"mode\": \"very fast\"}\n",
            0,
            16,
            "very fast=Fast, with a gap",
        ),
        #[cfg(feature = "toml")]
        (
            Language::TOML,
            "mode = \"very fast\"\n",
            0,
            14,
            "very fast=Fast, with a gap",
        ),
    ];
    let failures: Vec<String> = cases
        .into_iter()
        .filter_map(|(language, text, line, column, wanted)| {
            let shown = value_shown(language, text, line, column);
            (shown.as_deref() != Some(wanted))
                .then(|| format!("{language} {text:?} at {line}:{column}: {shown:?}"))
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Over a value the schema does not document, hover names nothing; over
/// a word of a comment beside a value, nothing either.
#[cfg(feature = "yaml")]
#[test]
fn an_undocumented_value_or_a_comment_shows_no_value() {
    assert_eq!(value_shown(Language::YAML, "mode: other\n", 0, 7), None);
    assert_eq!(
        value_shown(Language::YAML, "mode: slow # very fast\n", 0, 18),
        None
    );
}
