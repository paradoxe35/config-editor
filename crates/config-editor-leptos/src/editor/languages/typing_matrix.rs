//! Typing text character by character in every language, with what the
//! editor writes as it goes: pairs, closers typed over, lines re-indented,
//! and markup a character finishes (XML's `</` writes the end tag, an
//! attribute quote closes before `/>`, HCL's `"${` writes its `}`). Each
//! language, custom ones too, types its own quoted strings and bracketed
//! values whole; what a language's own syntax finishes is written by hand.

use super::super::keyboard::{is_closer, paired_edit, replace_selection};
use super::super::keys::complete::{completed, reindented};
use super::Language;
use super::harness::{At, every_language, report, write};
use serde_json::json;

/// Types `typed` at the `|` of `marked`, as the editor's keys do with
/// auto-closing on; returns the text with the caret marked.
fn type_in(language: Language, marked: &str, typed: &str) -> String {
    let behavior = language.behavior();
    let at = marked.find('|').expect("a caret");
    let mut text = marked.replacen('|', "", 1);
    let mut caret = marked[..at].encode_utf16().count();
    // Characters of closers the editor wrote, and whether each is of a
    // completion (any character typed over) or not (only closing brackets
    // and quotes are).
    let mut closers: Vec<(usize, bool)> = Vec::new();
    let units = |text: &str| text.encode_utf16().collect::<Vec<u16>>();
    for character in typed.chars() {
        let next = units(&text).get(caret).copied();
        let over = closers.iter().any(|(at, completion)| {
            *at == caret && (*completion || is_closer(behavior, character))
        });
        // A markup language's `>` moves over the `>` ahead.
        let markup_end = character == '>'
            && behavior.closing_markup_len("</a>").is_some()
            && behavior.lexical_state(&text, caret).quote.is_none();
        if (over || markup_end) && next == Some(character as u16) {
            closers.retain(|(at, _)| *at != caret);
            caret += 1;
            continue;
        }
        let before_len = units(&text).len();
        let made = |change: super::super::edit::Change| {
            let updated = change.apply(&text).expect("made on the text");
            let end = change
                .hull()
                .map_or(0, |(_, written)| usize::from(written.end()));
            let at = updated[..end].encode_utf16().count();
            (updated, at)
        };
        let (updated, new_caret, closed, completion) =
            if let Some(change) = reindented(behavior, &text, caret, character) {
                let (updated, at) = made(change);
                (updated, at, 0, false)
            } else if let Some(done) = completed(behavior, &text, caret, character) {
                let (updated, _) = made(done.change);
                (updated, done.caret, done.len, true)
            } else if let Some(closing) = behavior.auto_close(&text, caret, character) {
                let (updated, _) =
                    replace_selection(&text, caret, caret, &format!("{character}{closing}"));
                (updated, caret + 1, closing.encode_utf16().count(), false)
            } else if let Some((change, start, _)) =
                paired_edit(&text, caret, caret, character, behavior)
            {
                (made(change).0, start, 1, false)
            } else {
                let (updated, at) = replace_selection(&text, caret, caret, &character.to_string());
                (updated, at, 0, false)
            };
        let grown = units(&updated).len() - before_len;
        for (at, _) in &mut closers {
            if *at >= caret {
                *at += grown;
            }
        }
        closers.extend((new_caret..new_caret + closed).map(|at| (at, completion)));
        text = updated;
        caret = new_caret;
    }
    let units = units(&text);
    format!(
        "{}|{}",
        String::from_utf16_lossy(&units[..caret]),
        String::from_utf16_lossy(&units[caret..])
    )
}

#[test]
fn typed_characters_finish_what_they_begin() {
    let mut failures = Vec::new();
    for (language, marked, typed, expected) in [
        // XML: `</` writes the end tag of the open element.
        (
            Language::XML,
            "<r>\n  <a>x|\n</r>\n",
            "</",
            "<r>\n  <a>x</|a>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <a>\n    <b/>\n    |\n</r>\n",
            "</",
            "<r>\n  <a>\n    <b/>\n  </|a>\n</r>\n",
        ),
        (
            Language::XML,
            "<r>\n  <a>1</a>\n  |",
            "</",
            "<r>\n  <a>1</a>\n</|r>",
        ),
        (
            Language::XML,
            "<r>\n  <a>1</a>\n  |",
            "</r>",
            "<r>\n  <a>1</a>\n</r>|",
        ),
        // Typed whole, the end tag is typed over, not doubled.
        (
            Language::XML,
            "<r>\n  <a>x|\n</r>\n",
            "</a>",
            "<r>\n  <a>x</a>|\n</r>\n",
        ),
        (Language::XML, "<r><!-- |", "</", "<r><!-- </|"),
        // XML: an attribute quote closes before the tag's `/>`.
        (Language::XML, "<r|/>\n", " a=\"1", "<r a=\"1|\"/>\n"),
        (Language::XML, "<r|/>\n", " a=\"1\"", "<r a=\"1\"|/>\n"),
        (Language::XML, "<r|>\n", " a=\"1\"", "<r a=\"1\"|>\n"),
        // HCL: `${` and `%{` in a string write their `}`.
        (Language::HCL, "a = |\n", "\"${var.x", "a = \"${var.x|}\"\n"),
        (
            Language::HCL,
            "a = |\n",
            "\"${var.x}\"",
            "a = \"${var.x}\"|\n",
        ),
        (Language::HCL, "a = |\n", "\"%{if x", "a = \"%{if x|}\"\n"),
        (Language::HCL, "a = |\n", "\"$${", "a = \"$${|\"\n"),
        (Language::HCL, "a = {|\n", "", "a = {|\n"),
        (Language::HCL, "a = |\n", "${", "a = ${|}\n"),
        // Elsewhere `</` and `${` are text.
        (Language::YAML, "a: |\n", "\"${</", "a: \"${</|\"\n"),
        (Language::JSON, "{\"a\": |}", "\"${</", "{\"a\": \"${</|\"}"),
        (Language::TOML, "a = |\n", "\"${</", "a = \"${</|\"\n"),
    ] {
        let found = type_in(language, marked, typed);
        if found != expected {
            failures.push(format!(
                "{language} {marked:?} typing {typed:?}: {found:?}, expected {expected:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A quoted string or a bracketed value typed whole where a value goes
/// leaves exactly what was typed: each closer the editor wrote is typed
/// over, never doubled or left behind. In each language's own quotes and
/// list and map brackets, custom languages too.
#[test]
fn a_value_typed_whole_is_written_as_typed() {
    let mut failures = Vec::new();
    for language in every_language() {
        let config = language.behavior().config();
        let written = match write(
            language,
            &json!({"a": "", "b": 1}),
            &[],
            At::EmptyValue("/a"),
        ) {
            Ok(written) => written,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let mut typed: Vec<String> = config
            .quotes
            .iter()
            .filter(|_| language.behavior().markup().is_none())
            .map(|quote| format!("{quote}a b{quote}"))
            .collect();
        for (open, close) in [config.list_brackets, config.map_brackets]
            .into_iter()
            .flatten()
        {
            typed.push(format!("{open}{close}"));
        }
        let marked = written.marked();
        for typed in typed {
            let expected = marked.replacen('|', &format!("{typed}|"), 1);
            let found = type_in(language, &marked, &typed);
            if found != expected {
                failures.push(format!(
                    "{language} {marked:?} typing {typed:?}: {found:?}, expected {expected:?}"
                ));
            }
        }
    }
    report(&failures);
}
