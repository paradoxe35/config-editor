//! Documents to edit: the commented hosts the save properties use (every
//! shape saves must keep, in every language) and random trees written in
//! each language by the editor's own conversion.

use serde_json::{Map, Value, json};

use super::runner::Rng;
use crate::editor::languages::Language;
use crate::editor::save_properties::hosts;

pub(super) const LANGUAGES: [Language; 5] = [
    Language::YAML,
    Language::JSON,
    Language::TOML,
    Language::HCL,
    Language::XML,
];

const KEYS: [&str; 10] = [
    "name", "port", "host", "mode", "web", "tls", "allow", "a", "b", "x_1",
];

const STRINGS: [&str; 10] = [
    "web",
    "a b",
    "Port 80",
    "x\"y",
    "é",
    "line\nnext",
    "",
    "true",
    "8080",
    "😀 ok",
];

/// A random scalar of any type.
pub(super) fn random_scalar(rng: &mut Rng) -> Value {
    match rng.below(6) {
        0 => json!(rng.below(100_000) as i64 - 50),
        1 => json!(*rng.pick(&[0.5, 1.25, -3.75, 100.0, 1e-3])),
        2 => json!(rng.chance(50)),
        3 | 4 => json!(*rng.pick(&STRINGS)),
        _ => json!(*rng.pick(&["port", "host", "web"])),
    }
}

/// A random tree: a map of up to `width` entries, `depth` levels deep.
pub(super) fn random_value(rng: &mut Rng, depth: usize) -> Value {
    let mut map = Map::new();
    for _ in 0..1 + rng.below(4) {
        let key = rng.pick(&KEYS).to_string();
        let value = if depth == 0 {
            random_scalar(rng)
        } else {
            match rng.below(5) {
                0 | 1 => random_scalar(rng),
                2 => random_value(rng, depth - 1),
                3 => Value::Array((0..1 + rng.below(3)).map(|_| random_scalar(rng)).collect()),
                _ => Value::Array(
                    (0..1 + rng.below(3))
                        .map(|_| random_value(rng, depth - 1))
                        .collect(),
                ),
            }
        };
        map.insert(key, value);
    }
    Value::Object(map)
}

/// A document in `language` to edit: one of the commented hosts written
/// in it, or a random tree converted to it.
pub(super) fn document(rng: &mut Rng, language: Language) -> String {
    if rng.chance(50) {
        let written: Vec<String> = hosts()
            .into_iter()
            .filter(|(host, _)| *host == language)
            .map(|(_, text)| text.to_string())
            .collect();
        if !written.is_empty() {
            return rng.pick(&written).clone();
        }
    }
    for _ in 0..8 {
        let value = random_value(rng, 2);
        let json = serde_json::to_string_pretty(&value).expect("a tree") + "\n";
        if language == Language::JSON {
            return json;
        }
        if let Ok(conversion) = Language::JSON.convert(&json, language) {
            return conversion.text;
        }
    }
    hosts()
        .into_iter()
        .find(|(host, _)| *host == language)
        .map(|(_, text)| text.to_string())
        .expect("a host in every language")
}

/// `text` without line `line` (from 0), for shrinking a document.
pub(super) fn without_line(text: &str, line: usize) -> Option<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    (line < lines.len()).then(|| {
        lines
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != line)
            .map(|(_, line)| *line)
            .collect()
    })
}

/// Smaller documents still parsing in `language`: a line, or two lines
/// (an opener and its closer), left out.
pub(super) fn smaller_documents(language: Language, text: &str) -> Vec<String> {
    let count = text.split_inclusive('\n').count();
    let mut out = Vec::new();
    for line in 0..count {
        if let Some(smaller) = without_line(text, line)
            && language.parse(&smaller).is_ok()
        {
            out.push(smaller);
        }
    }
    for first in 0..count {
        for last in first + 1..count.min(first + 12) {
            let lines: Vec<&str> = text.split_inclusive('\n').collect();
            let smaller: String = lines
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != first && *index != last)
                .map(|(_, line)| *line)
                .collect();
            if language.parse(&smaller).is_ok() {
                out.push(smaller);
            }
        }
    }
    out
}

/// UTF-16 units in `text`.
pub(super) fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

/// The UTF-16 offset a raw position stands for in `text`: on a character
/// boundary, at most the text's end.
pub(super) fn offset(text: &str, at: u32) -> usize {
    let characters = text.chars().count();
    let index = at as usize % (characters + 1);
    text.chars().take(index).map(char::len_utf16).sum()
}

/// The (start, end) UTF-16 selection two raw positions stand for.
pub(super) fn selection(text: &str, at: u32, to: u32) -> (usize, usize) {
    let (a, b) = (offset(text, at), offset(text, to));
    (a.min(b), a.max(b))
}

/// The zero-based line holding UTF-16 `offset`.
pub(super) fn line_of(text: &str, offset: usize) -> usize {
    let mut units = 0;
    let mut line = 0;
    for character in text.chars() {
        if units >= offset {
            break;
        }
        units += character.len_utf16();
        if character == '\n' {
            line += 1;
        }
    }
    line
}

/// The text written as a Rust string literal.
pub(super) fn literal(text: &str) -> String {
    format!("{text:?}")
}
