//! What a typed opener pairs with comes from the language's own brackets
//! and quotes ([`ConfigLanguage::config`]), not a fixed set: round 26
//! paired `'` in JSON and HCL, `(` in YAML, JSON and TOML, and `{ [ '` in
//! XML element text.

use super::super::keyboard::{delete_empty_pair, paired_edit, replace_selection};
use super::super::language::ConfigLanguage;
use super::super::odd_syntax::ODD;
use super::Language;

/// `marked` (caret at `|`) after typing `typed` there, as the keys do:
/// the language's own closing first, then the shared pairing.
fn typed_in(behavior: &dyn ConfigLanguage, marked: &str, typed: char) -> String {
    let at = marked.find('|').expect("a caret");
    let text = marked.replacen('|', "", 1);
    let caret = marked[..at].encode_utf16().count();
    let (updated, caret) = if let Some(closing) = behavior.auto_close(&text, caret, typed) {
        let (updated, _) = replace_selection(&text, caret, caret, &format!("{typed}{closing}"));
        (updated, caret + 1)
    } else if let Some((updated, start, _)) = paired_edit(&text, caret, caret, typed, behavior) {
        (updated, start)
    } else {
        replace_selection(&text, caret, caret, &typed.to_string())
    };
    let units: Vec<u16> = updated.encode_utf16().collect();
    format!(
        "{}|{}",
        String::from_utf16_lossy(&units[..caret]),
        String::from_utf16_lossy(&units[caret..])
    )
}

#[test]
fn only_the_languages_own_brackets_and_quotes_pair() {
    let languages: [(&dyn ConfigLanguage, &str); 6] = [
        (Language::YAML.behavior(), "a: |\n"),
        (Language::JSON.behavior(), "{\"a\": |}"),
        (Language::TOML.behavior(), "a = |\n"),
        (Language::HCL.behavior(), "a = |\n"),
        (Language::XML.behavior(), "<a>\n  |\n</a>"),
        (&ODD, "a := |\n"),
    ];
    let mut failures = Vec::new();
    for (behavior, marked) in languages {
        let config = behavior.config();
        for typed in ['(', '[', '{', '<', '"', '\''] {
            let bracket = config.brackets.iter().find(|(open, _)| *open == typed);
            let quote = config.quotes.contains(&typed.to_string().as_str());
            // Element text has no quotes: XML's are those of attributes.
            let text_quote = quote && behavior.markup_delimiters().is_some();
            let expected = match (bracket, quote && !text_quote) {
                (Some((_, close)), _) => marked.replace('|', &format!("{typed}|{close}")),
                (None, true) => marked.replace('|', &format!("{typed}|{typed}")),
                (None, false) => marked.replace('|', &format!("{typed}|")),
            };
            let got = typed_in(behavior, marked, typed);
            if got != expected {
                failures.push(format!(
                    "{} {marked:?} + {typed:?}: {got:?}, expected {expected:?}",
                    behavior.info().label
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn what_round_26_paired_is_written_alone() {
    let cases: [(Language, &str, char, &str); 10] = [
        (Language::JSON, "{\"a\": |}", '\'', "{\"a\": '|}"),
        (Language::JSON, "{\"a\": |}", '(', "{\"a\": (|}"),
        (Language::HCL, "a = |\n", '\'', "a = '|\n"),
        (Language::YAML, "a: |\n", '(', "a: (|\n"),
        (Language::TOML, "a = |\n", '(', "a = (|\n"),
        (Language::XML, "<a>\n  |\n</a>", '{', "<a>\n  {|\n</a>"),
        (Language::XML, "<a>\n  |\n</a>", '[', "<a>\n  [|\n</a>"),
        (Language::XML, "<a>\n  |\n</a>", '\'', "<a>\n  '|\n</a>"),
        // What the languages do pair still does.
        (Language::HCL, "a = max|\n", '(', "a = max(|)\n"),
        (Language::XML, "<a b=|>", '"', "<a b=\"|\">"),
    ];
    for (language, marked, typed, expected) in cases {
        assert_eq!(
            typed_in(language.behavior(), marked, typed),
            expected,
            "{language} {marked:?} + {typed:?}"
        );
    }
}

#[test]
fn backspace_removes_an_empty_pair_of_the_language() {
    let yaml = Language::YAML.behavior();
    assert_eq!(
        delete_empty_pair("value: []", 8, yaml),
        Some(("value: ".to_string(), 7))
    );
    assert_eq!(delete_empty_pair("value: [x]", 8, yaml), None);
    // `()` is no pair in YAML, but is in the odd language.
    assert_eq!(delete_empty_pair("a: ()", 4, yaml), None);
    assert_eq!(
        delete_empty_pair("a := ()", 6, &ODD),
        Some(("a := ".to_string(), 5))
    );
}
