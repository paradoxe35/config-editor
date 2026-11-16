//! Cases written once, as data, and run in every language: the document is
//! a value, the caret a place in it (a key, a value, a new entry), and each
//! language writes the text itself (its `serialize`, `format_value`) and
//! finds the place in it (its key and value spans). No shared case writes
//! one language's syntax, so a new language, or a change to one, is checked
//! by every case at once.
//!
//! The languages are the five built-in ones and the custom test languages:
//! Odd (a trait implementation with unusual syntax), Dot (a language as a
//! JavaScript host declares it) and DotBare (Dot declaring only comments
//! and a separator). A case runs in the languages it fits: suggestions in
//! those that read the caret's place ([`completing`]), hover in those that
//! find a hovered key ([`hovering`]), colours in those that colour
//! ([`highlighting`]), and a nested place where the language places nested
//! keys (a flat language places top-level ones only). A behaviour that
//! rests on one language's own syntax (comments, block scalars, heredocs,
//! XML attributes) keeps its cases in a list of its own, beside the shared
//! ones. The INI language of the integration tests is not here: those are
//! another crate, and the lib's tests cannot reach it.

pub(super) mod completion;

use crate::editor::paths::FieldPath;
use serde_json::Value;

use super::super::declared_syntax::{DOT, DOT_BARE};
use super::super::language::{Span, pointer_segments};
use super::super::odd_syntax::ODD;
use super::Language;

/// The five built-in languages.
pub(super) const BUILT_IN: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

/// The custom test languages.
pub(super) fn custom() -> [Language; 3] {
    [
        Language::new(&ODD),
        Language::new(&DOT),
        Language::new(&DOT_BARE),
    ]
}

/// Every language a shared case runs in: the built-in ones, then the
/// custom ones.
pub(super) fn every_language() -> Vec<Language> {
    BUILT_IN.into_iter().chain(custom()).collect()
}

/// Every language but those named in `left_out` (by label): the languages
/// a case passes in while it waits for a fix in the others, which an
/// ignored test of their own runs. Only the schema matrices, which need
/// every language and schema validation, use it.
#[cfg(all(
    feature = "schema-validation",
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
pub(super) fn every_language_but(left_out: &[&str]) -> Vec<Language> {
    every_language()
        .into_iter()
        .filter(|language| !left_out.contains(&language.label()))
        .collect()
}

/// The languages a suggestion case fits: those that read where the caret
/// is (`completion_context`), so a key half typed is told from nothing
/// typed. A language keeping the trait's default (Odd, and DotBare, which
/// declares no brackets to read the caret inside one-line values) has no
/// suggestions to check.
pub(super) fn completing() -> Vec<Language> {
    every_language()
        .into_iter()
        .filter(|language| {
            let at = At::NewKey {
                object: "",
                typed: "ab",
            };
            write(*language, &serde_json::json!({"a": 1}), &[], at).is_ok_and(|written| {
                language
                    .behavior()
                    .completion_context(&written.text, written.cursor())
                    .typed
                    == "ab"
            })
        })
        .collect()
}

/// The languages a hover case fits: those whose hover finds the key under
/// the pointer (a language keeping the trait's defaults finds none).
pub(super) fn hovering() -> Vec<Language> {
    let schema = super::super::schema::build_schema(
        r#"{"properties": {"a": {"properties": {"b": {"description": "d"}}}}}"#,
    )
    .expect("a schema");
    let document = serde_json::json!({"a": {"b": 1}});
    every_language()
        .into_iter()
        .filter(|language| {
            text(*language, &document, &[])
                .and_then(|text| Ok((key_place(*language, &text, "/a/b", 1)?, text)))
                .is_ok_and(|((line, column), text)| {
                    super::super::completion::key_hover(
                        &text,
                        line,
                        column,
                        Some(&schema),
                        language.behavior(),
                    )
                    .is_some_and(|hover| hover.description.as_deref() == Some("d"))
                })
        })
        .collect()
}

/// The languages a colouring case fits: those that colour their text
/// (`highlight`); a language keeping the trait's default colours nothing.
pub(super) fn highlighting() -> Vec<Language> {
    every_language()
        .into_iter()
        .filter(|language| {
            text(*language, &serde_json::json!({"a": 1}), &[]).is_ok_and(|text| {
                text.lines()
                    .any(|line| !language.behavior().highlight(line).is_empty())
            })
        })
        .collect()
}

/// Whether `language` writes `value` as a document that reads back as it
/// (TOML has no null, XML no empty list or list of one).
pub(super) fn holds(language: Language, value: &Value) -> bool {
    language
        .serialize(value)
        .is_ok_and(|text| language.parse(&text).is_ok_and(|read| read == *value))
}

/// A key name no case uses, holding the place of an entry being typed.
const SLOT: &str = "slotkeyplaceholder";
/// A value no case uses, holding the place of a value written later.
const SLOT_VALUE: &str = "slotvalueplaceholder";

/// Where the caret is, in the document's terms.
#[derive(Clone, Copy, Debug)]
pub(super) enum At<'a> {
    /// A new entry of the object at `object` (a JSON pointer), after its
    /// last one, with `typed` typed of its key: on a line of its own where
    /// the object is written over lines. A key the language quotes has its
    /// quotes around `typed` (as typing a quote pairs it), and none while
    /// nothing is typed; markup before a name (`<`) is written.
    NewKey { object: &'a str, typed: &'a str },
    /// Where a new entry of the object at `object` goes, as
    /// [`NewKey`](Self::NewKey) with nothing typed, nor the markup that
    /// opens a name (an empty line, the caret at its indentation).
    NewLine { object: &'a str },
    /// Within the key at `pointer`, `offset` characters into its name.
    InKey { pointer: &'a str, offset: usize },
    /// Just after the key at `pointer`.
    KeyEnd(&'a str),
    /// `typed` typed in front of the name of the key at `pointer`, the
    /// caret after it.
    BeforeKey { pointer: &'a str, typed: &'a str },
    /// At the start of the line of the key at `pointer`, `offset`
    /// characters into it (in its indentation).
    LineOfKey { pointer: &'a str, offset: usize },
    /// Within the value at `pointer`, `offset` characters into it.
    InValue { pointer: &'a str, offset: usize },
    /// Where the value at `pointer` is typed, nothing typed of it yet: the
    /// value cut from its place, any quotes too.
    EmptyValue(&'a str),
}

/// A document as one language writes it, with the caret placed.
#[derive(Clone, Debug)]
pub(super) struct Written {
    pub language: Language,
    pub text: String,
    /// The caret's byte offset.
    pub byte: usize,
}

impl Written {
    /// The caret's UTF-16 offset, as the editor counts it.
    pub fn cursor(&self) -> usize {
        self.text[..self.byte].encode_utf16().count()
    }

    /// The text with `|` at the caret, as hand-written cases mark it (the
    /// first `|`: a language writing `|` before the caret, as Odd's lists
    /// do, needs [`cursor`](Self::cursor) instead).
    pub fn marked(&self) -> String {
        assert!(
            !self.text[..self.byte].contains('|'),
            "{}: a `|` before the caret makes the mark ambiguous; take the cursor\n{}",
            self.language,
            self.text
        );
        format!("{}|{}", &self.text[..self.byte], &self.text[self.byte..])
    }

    /// The caret's zero-based line and character column.
    pub fn line_column(&self) -> (usize, usize) {
        let before = &self.text[..self.byte];
        let line = before.matches('\n').count();
        let start = before.rfind('\n').map_or(0, |at| at + 1);
        (line, before[start..].chars().count())
    }
}

/// `document` as `language` writes it, the values at the pointers of
/// `one_line` written on one line (as `format_value` writes a value, or
/// as a markup language writes an element's children), with the caret
/// `at`: `Err` when the language cannot write it or does not place the
/// caret's key.
pub(super) fn write(
    language: Language,
    document: &Value,
    one_line: &[&str],
    at: At<'_>,
) -> Result<Written, String> {
    let caret = |text: String, byte: usize| Written {
        language,
        text,
        byte,
    };
    match at {
        At::NewKey { object, typed } => new_key(language, document, one_line, object, typed),
        At::NewLine { object } => {
            let mut written = new_key(language, document, one_line, object, "")?;
            if let Some((open, _)) = language.behavior().markup_delimiters()
                && written.text[..written.byte].ends_with(open)
            {
                written.byte -= open.len_utf8();
                written.text.remove(written.byte);
            }
            Ok(written)
        }
        At::InKey { pointer, offset } => {
            let text = text(language, document, one_line)?;
            let (start, _) = name_range(language, &text, pointer)?;
            let byte = char_offset(&text, start, offset);
            Ok(caret(text, byte))
        }
        At::KeyEnd(pointer) => {
            let text = text(language, document, one_line)?;
            let (_, end) = name_range(language, &text, pointer)?;
            Ok(caret(text, end))
        }
        At::BeforeKey { pointer, typed } => {
            let mut text = text(language, document, one_line)?;
            let (start, _) = name_range(language, &text, pointer)?;
            text.insert_str(start, typed);
            Ok(caret(text, start + typed.len()))
        }
        At::LineOfKey { pointer, offset } => {
            let text = text(language, document, one_line)?;
            let (start, _) = name_range(language, &text, pointer)?;
            let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
            let byte = char_offset(&text, line_start, offset);
            Ok(caret(text, byte))
        }
        At::InValue { pointer, offset } => {
            let text = text(language, document, one_line)?;
            let span = language
                .behavior()
                .value_span(&text, &FieldPath::from(pointer))
                .ok_or_else(|| unplaced(language, "value", pointer, &text))?;
            let (start, _) = byte_range(&text, span);
            let byte = char_offset(&text, start, offset);
            Ok(caret(text, byte))
        }
        At::EmptyValue(pointer) => {
            let mut held = document.clone();
            *held
                .pointer_mut(pointer)
                .ok_or_else(|| format!("no value at {pointer:?} in {document}"))? =
                Value::String(SLOT_VALUE.to_string());
            let mut text = text(language, &held, one_line)?;
            let (start, end) = quoted(language, &text, SLOT_VALUE)?;
            text.replace_range(start..end, "");
            Ok(caret(text, start))
        }
    }
}

/// The byte range of `placeholder` in `text`, with the quotes it is
/// written in.
fn quoted(language: Language, text: &str, placeholder: &str) -> Result<(usize, usize), String> {
    let at = text
        .find(placeholder)
        .ok_or_else(|| format!("{language}: no {placeholder} in\n{text}"))?;
    let (mut start, mut end) = (at, at + placeholder.len());
    if let (Some(open), Some(close)) = (text[..start].chars().last(), text[end..].chars().next())
        && open == close
        && language
            .behavior()
            .config()
            .quotes
            .contains(&open.to_string().as_str())
    {
        start -= open.len_utf8();
        end += close.len_utf8();
    }
    Ok((start, end))
}

/// `document` as `language` writes it, the values at `one_line` on one
/// line each.
pub(super) fn text(
    language: Language,
    document: &Value,
    one_line: &[&str],
) -> Result<String, String> {
    let behavior = language.behavior();
    let mut held = document.clone();
    let placeholder = |index: usize| format!("onelineplaceholder{index}");
    for (index, pointer) in one_line.iter().enumerate() {
        *held
            .pointer_mut(pointer)
            .ok_or_else(|| format!("no value at {pointer:?} in {document}"))? =
            Value::String(placeholder(index));
    }
    let mut text = behavior
        .serialize(&held, None)
        .map_err(|error| format!("{language} cannot write {held}: {error}"))?;
    for (index, pointer) in one_line.iter().enumerate() {
        let value = document.pointer(pointer).expect("checked above");
        let written = match behavior.markup() {
            Some(_) => behavior
                .one_line_body(value, "", "  ")
                .unwrap_or_else(|| behavior.format_value(value)),
            None => behavior.format_value(value),
        };
        let (start, end) = quoted(language, &text, &placeholder(index))?;
        text.replace_range(start..end, &written);
    }
    Ok(text)
}

/// The byte range of the key name at `pointer` in `text`, inside any
/// quotes or markup the language writes around it.
pub(super) fn name_range(
    language: Language,
    text: &str,
    pointer: &str,
) -> Result<(usize, usize), String> {
    let span = language
        .behavior()
        .key_span(text, &FieldPath::from(pointer))
        .ok_or_else(|| unplaced(language, "key", pointer, text))?;
    let (start, end) = byte_range(text, span);
    let name = pointer_segments(pointer).pop().unwrap_or_default();
    let found = text[start..end]
        .find(name.as_str())
        .or_else(|| text[start..].find(name.as_str()))
        .ok_or_else(|| format!("{language}: the key {name:?} is not at its span\n{text}"))?;
    Ok((start + found, start + found + name.len()))
}

/// The zero-based line and character column `offset` characters into the
/// key name at `pointer`, as hover takes them.
pub(super) fn key_place(
    language: Language,
    text: &str,
    pointer: &str,
    offset: usize,
) -> Result<(usize, usize), String> {
    let (start, _) = name_range(language, text, pointer)?;
    let written = Written {
        language,
        text: text.to_string(),
        byte: char_offset(text, start, offset),
    };
    Ok(written.line_column())
}

/// The zero-based line and character column `offset` characters into
/// `written`, the first time it is written in the value at `pointer`, as
/// hover takes them.
pub(super) fn value_place(
    language: Language,
    text: &str,
    pointer: &str,
    written: &str,
    offset: usize,
) -> Result<(usize, usize), String> {
    let span = language
        .behavior()
        .value_span(text, &FieldPath::from(pointer))
        .ok_or_else(|| unplaced(language, "value", pointer, text))?;
    let (start, _) = byte_range(text, span);
    let at = text[start..]
        .find(written)
        .ok_or_else(|| format!("{language}: no {written:?} at {pointer}\n{text}"))?;
    let place = Written {
        language,
        text: text.to_string(),
        byte: char_offset(text, start + at, offset),
    };
    Ok(place.line_column())
}

/// The key at `pointer` as written: its name with the quotes it is written
/// in, without markup around it.
pub(super) fn key_text(language: Language, text: &str, pointer: &str) -> Result<String, String> {
    let (start, end) = name_range(language, text, pointer)?;
    let quote = |c: Option<char>| {
        c.filter(|c| {
            language
                .behavior()
                .config()
                .quotes
                .contains(&c.to_string().as_str())
        })
    };
    match (
        quote(text[..start].chars().last()),
        quote(text[end..].chars().next()),
    ) {
        (Some(open), Some(close)) if open == close => {
            Ok(text[start - open.len_utf8()..end + close.len_utf8()].to_string())
        }
        _ => Ok(text[start..end].to_string()),
    }
}

/// The byte range of `span` in `text`.
pub(super) fn byte_range(text: &str, span: Span) -> (usize, usize) {
    let line_start: usize = text
        .split_inclusive('\n')
        .take(span.line - 1)
        .map(str::len)
        .sum();
    (
        char_offset(text, line_start, span.start_column - 1),
        char_offset(text, line_start, span.end_column - 1),
    )
}

/// The byte `chars` characters after byte `from` of `text`.
fn char_offset(text: &str, from: usize, chars: usize) -> usize {
    text[from..]
        .char_indices()
        .nth(chars)
        .map_or(text.len(), |(at, _)| from + at)
}

/// The document with a key being typed after the last entry of `object`:
/// written with a placeholder entry there, whose name is then replaced by
/// `typed` and whose value is cut off.
fn new_key(
    language: Language,
    document: &Value,
    one_line: &[&str],
    object: &str,
    typed: &str,
) -> Result<Written, String> {
    let mut with_slot = document.clone();
    let members = with_slot
        .pointer_mut(object)
        .and_then(Value::as_object_mut)
        .ok_or_else(|| format!("no object at {object:?} in {document}"))?;
    members.insert(SLOT.to_string(), Value::String(SLOT_VALUE.to_string()));
    let text = text(language, &with_slot, one_line)?;
    let name_start = match name_range(language, &text, &format!("{object}/{SLOT}")) {
        Ok((start, _)) => start,
        // A language placing no keys of a one-line value: the slot is
        // where its name is written.
        Err(_) if !one_line.is_empty() => text
            .find(SLOT)
            .ok_or_else(|| format!("{language}: no placeholder key\n{text}"))?,
        Err(error) => return Err(error),
    };
    let value = text[name_start..]
        .find(SLOT_VALUE)
        .map(|at| name_start + at)
        .ok_or_else(|| format!("{language}: no placeholder value\n{text}"))?;
    let mut entry_end = value + SLOT_VALUE.len();
    let line_end = |from: usize| text[from..].find('\n').map_or(text.len(), |at| from + at);
    let quotes = language.behavior().config().quotes;
    // The value's closing quote goes with it, and markup closing the entry
    // on its line (`</slot>`) too; a closer or separator of the object
    // after it stays.
    if let Some(quote) = text[entry_end..].chars().next()
        && quotes.contains(&quote.to_string().as_str())
    {
        entry_end += quote.len_utf8();
    }
    if let Some(closing) = text[entry_end..line_end(entry_end)].find(SLOT) {
        entry_end += closing + SLOT.len();
        if text[entry_end..].starts_with('>') {
            entry_end += 1;
        }
    }
    let tail = &text[entry_end..line_end(entry_end)];
    let lead_start = text[..name_start].rfind('\n').map_or(0, |at| at + 1);
    let lead = &text[lead_start..name_start];
    // An opening quote of the key gets its closing one after the caret.
    let quote = lead
        .chars()
        .last()
        .map(String::from)
        .filter(|c| quotes.contains(&c.as_str()))
        .unwrap_or_default();
    // Nothing typed yet: no quote either, the caret alone on its line.
    let (name_start, quote) = if typed.is_empty() {
        (name_start - quote.len(), String::new())
    } else {
        (name_start, quote)
    };
    let mut written = String::with_capacity(text.len());
    written.push_str(&text[..name_start]);
    written.push_str(typed);
    let byte = written.len();
    written.push_str(&quote);
    written.push_str(tail);
    written.push_str(&text[line_end(entry_end)..]);
    Ok(Written {
        language,
        text: written,
        byte,
    })
}

/// What a case that does not apply says: one placing a nested key or value
/// in a language that places top-level ones only (a flat language, as a
/// host declares one without spans).
const NOT_APPLICABLE: &str = "not applicable:";

/// Whether `language` places keys below the top level.
fn places_nested(language: Language) -> bool {
    let document = serde_json::json!({"a": {"b": 1}});
    language.serialize(&document).is_ok_and(|text| {
        language
            .behavior()
            .key_span(&text, &FieldPath::from("/a/b"))
            .is_some()
    })
}

/// Why the `what` (a key or a value) at `pointer` is not found in `text`.
fn unplaced(language: Language, what: &str, pointer: &str, text: &str) -> String {
    if pointer_segments(pointer).len() > 1 && !places_nested(language) {
        format!("{NOT_APPLICABLE} {language} places top-level keys only, not {pointer}")
    } else {
        format!("{language} places no {what} at {pointer}\n{text}")
    }
}

/// Every failure of a case table at once, or nothing. A case that does not
/// apply to a language (see [`NOT_APPLICABLE`]) is no failure.
#[track_caller]
pub(super) fn report(failures: &[String]) {
    let failures: Vec<&String> = failures
        .iter()
        .filter(|failure| !failure.contains(NOT_APPLICABLE))
        .collect();
    assert!(
        failures.is_empty(),
        "{} failing case(s):\n{}",
        failures.len(),
        failures
            .iter()
            .map(|failure| failure.as_str())
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}

/// The places a case names are found in every language: a new key after an
/// object's entries (over lines and on one line), a key and a value.
#[test]
fn every_language_writes_the_places_a_case_names() {
    let document = serde_json::json!({
        "spec": {"name": "web", "port": 80, "meta": {"a": "x"}},
        "top": "t"
    });
    let mut failures = Vec::new();
    for language in every_language() {
        for (one_line, at) in [
            (
                &[][..],
                At::NewKey {
                    object: "/spec",
                    typed: "",
                },
            ),
            (
                &[][..],
                At::NewKey {
                    object: "",
                    typed: "po",
                },
            ),
            (
                &["/spec/meta"][..],
                At::NewKey {
                    object: "/spec/meta",
                    typed: "b",
                },
            ),
            (
                &[][..],
                At::InKey {
                    pointer: "/spec/port",
                    offset: 1,
                },
            ),
            (&[][..], At::KeyEnd("/spec/name")),
            (
                &[][..],
                At::InValue {
                    pointer: "/top",
                    offset: 0,
                },
            ),
        ] {
            match write(language, &document, one_line, at) {
                Ok(written) if written.text.is_char_boundary(written.byte) => {}
                Ok(written) => failures.push(format!("{at:?}: no caret place in {written:?}")),
                Err(error) => failures.push(format!("{at:?}: {error}")),
            }
        }
    }
    report(&failures);
}
