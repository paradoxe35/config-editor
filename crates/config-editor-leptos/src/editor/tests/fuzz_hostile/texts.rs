//! Hostile texts in every language: random characters, and near-valid
//! documents broken on purpose (deep nesting, very long lines, huge
//! numbers, mixed line endings and lone CRs, a byte order mark, NUL and
//! control characters, the characters around the surrogate range,
//! combining and joining marks, unclosed strings, comments and heredocs,
//! thousands of small items). Shrinking cuts halves, lines, then
//! characters.

use super::runner::Rng;
use crate::editor::declared_syntax::{DOT, DOT_BARE};
use crate::editor::languages::Language;
use crate::editor::odd_syntax::ODD;

/// Every language a text is read in, by the name a repro gives: the
/// built-ins, a custom language implemented on the trait, and two declared
/// ones (all syntax, and only comments and a separator).
pub(super) const NAMES: [&str; 8] = [
    "YAML", "JSON", "TOML", "HCL", "XML", "ODD", "DOT", "DOT_BARE",
];

/// The language a repro names.
pub(super) fn language(name: &str) -> Language {
    match name {
        "YAML" => Language::YAML,
        "JSON" => Language::JSON,
        "TOML" => Language::TOML,
        "HCL" => Language::HCL,
        "XML" => Language::XML,
        "ODD" => Language::new(&ODD),
        "DOT" => Language::new(&DOT),
        "DOT_BARE" => Language::new(&DOT_BARE),
        _ => panic!("no language {name}"),
    }
}

/// Characters that break readers: controls, NUL, line and paragraph
/// separators, the byte order mark, the code points on each side of the
/// surrogate range, combining and joining marks, right-to-left overrides,
/// the last code point, wide ones.
pub(super) const ODD_CHARACTERS: [&str; 30] = [
    "\0",
    "\u{1}",
    "\u{7f}",
    "\u{85}",
    "\u{b}",
    "\u{c}",
    "\u{1b}",
    "\u{2028}",
    "\u{2029}",
    "\u{feff}",
    "\u{200d}",
    "\u{301}",
    "e\u{301}\u{302}\u{303}",
    "\u{d7ff}",
    "\u{e000}",
    "\u{fffd}",
    "\u{ffff}",
    "\u{10ffff}",
    "\u{202e}",
    "\u{200f}",
    "😀",
    "👨\u{200d}👩\u{200d}👧",
    "中",
    "é",
    "\t",
    "\r",
    "\r\n",
    "\n",
    "\\",
    "\u{a0}",
];

/// Pieces of syntax left open or written oddly, in some language.
const PIECES: [&str; 64] = [
    "[",
    "]",
    "{",
    "}",
    "(",
    ")",
    "<",
    ">",
    "\"",
    "'",
    "`",
    "\"\"\"",
    "'''",
    "#",
    "//",
    "/*",
    "*/",
    "<!--",
    "-->",
    "<![CDATA[",
    "]]>",
    "<?xml version=\"1.0\"?>",
    "<!DOCTYPE a [",
    "&amp",
    "&#x",
    "&#1114112;",
    "<a b='",
    "</",
    "/>",
    "${",
    "%{",
    "<<EOT\n",
    "<<-EOT\n",
    "EOT\n",
    ": ",
    " = ",
    ":=",
    "|",
    "|-\n",
    ">+\n",
    "- ",
    "? ",
    "--- ",
    "...\n",
    "%YAML 1.2\n",
    "&a ",
    "*a",
    "!!binary ",
    "!<tag> ",
    "[[a]]\n",
    "[a.b]\n",
    "a.b.c = ",
    "\\u12",
    "\\x",
    "\\",
    ",",
    ";",
    "@ ",
    "* ",
    "=",
    "\n",
    " ",
    "\t",
    "  - ",
];

/// Numbers past what a reader holds.
const NUMBERS: [&str; 14] = [
    "1e999999",
    "-1e-999999",
    "1.7976931348623157e309",
    "9223372036854775808",
    "-9223372036854775809",
    "18446744073709551616",
    "0x",
    "0xFFFFFFFFFFFFFFFFFFFFFFFF",
    "0b",
    "1_000_000_",
    "-0",
    ".inf",
    "NaN",
    "0.000000000000000000000000000000000000000000000000000000000001",
];

/// A random string of the odd characters, pieces and plain text.
pub(super) fn random_text(rng: &mut Rng, pieces: usize) -> String {
    let mut text = String::new();
    for _ in 0..pieces {
        match rng.below(4) {
            0 => text.push_str(rng.pick(&ODD_CHARACTERS)),
            1 => text.push_str(rng.pick(&PIECES)),
            2 => text.push_str(rng.pick(&["a", "key", "1", "x_1", "web", "true", "null"])),
            _ => {
                // Any code point outside the surrogates.
                let code = rng.below(0x11_0000) as u32;
                text.push(char::from_u32(code).unwrap_or('\u{fffd}'));
            }
        }
    }
    text
}

/// A random scalar, odd ones included.
fn scalar(rng: &mut Rng) -> serde_json::Value {
    use serde_json::json;
    match rng.below(7) {
        0 => json!(rng.below(100_000) as i64 - 50),
        1 => json!(*rng.pick(&[0.5, -3.75, 1e300, 5e-324])),
        2 => json!(rng.chance(50)),
        3 => serde_json::Value::Null,
        4 => json!(*rng.pick(&ODD_CHARACTERS)),
        _ => json!(*rng.pick(&["web", "a b", "x\"y", "line\nnext", "", "8080", "é😀"])),
    }
}

/// A random tree: a map of a few entries, `depth` levels deep.
fn tree(rng: &mut Rng, depth: usize) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for _ in 0..1 + rng.below(4) {
        let key = rng
            .pick(&["name", "port", "a", "x_1", "é", "a b", "k"])
            .to_string();
        let value = match (depth, rng.below(4)) {
            (0, _) | (_, 0 | 1) => scalar(rng),
            (_, 2) => tree(rng, depth - 1),
            _ => serde_json::Value::Array((0..1 + rng.below(3)).map(|_| scalar(rng)).collect()),
        };
        map.insert(key, value);
    }
    serde_json::Value::Object(map)
}

/// A document in `language` to break: a random tree written in it (a
/// custom language writes its own).
pub(super) fn document(rng: &mut Rng, language: Language) -> String {
    for _ in 0..8 {
        let value = tree(rng, 2);
        if let Ok(text) = language.serialize(&value) {
            return text;
        }
    }
    String::new()
}

/// One small item, `index`, in `language`: a key and its value.
fn item(language: &str, index: usize, value: &str) -> String {
    match language {
        "JSON" => format!("  \"k{index}\": {value},\n"),
        "TOML" | "HCL" => format!("k{index} = {value}\n"),
        "XML" => format!("  <k{index}>{value}</k{index}>\n"),
        "ODD" => format!("k{index} := {value}\n"),
        _ => format!("k{index}: {value}\n"),
    }
}

/// `count` small items in `language`, inside its root where it has one.
pub(super) fn many_items(language: &str, count: usize, value: &str) -> String {
    let items: String = (0..count)
        .map(|index| item(language, index, value))
        .collect();
    match language {
        "JSON" => format!("{{\n{}  \"end\": 0\n}}\n", items),
        "XML" => format!("<r>\n{items}</r>\n"),
        _ => items,
    }
}

/// A value nested `depth` levels deep in `language`.
pub(super) fn nested(language: &str, depth: usize) -> String {
    match language {
        "XML" => {
            let open: String = (0..depth).map(|_| "<a>").collect();
            let close: String = (0..depth).map(|_| "</a>").collect();
            format!("<r>{open}x{close}</r>\n")
        }
        "YAML" if depth.is_multiple_of(2) => {
            // Nested by indentation.
            (0..depth)
                .map(|level| format!("{}k:\n", " ".repeat(level)))
                .collect::<String>()
                + &" ".repeat(depth)
                + "1\n"
        }
        "ODD" => format!("a := {}1{}\n", "( ".repeat(depth), " )".repeat(depth)),
        "DOT" | "DOT_BARE" => format!("a: {}1{}\n", "(".repeat(depth), ")".repeat(depth)),
        "JSON" => format!("{{\"a\": {}1{}}}\n", "[".repeat(depth), "]".repeat(depth)),
        "YAML" => format!("a: {}1{}\n", "[".repeat(depth), "]".repeat(depth)),
        _ => format!("a = {}1{}\n", "[".repeat(depth), "]".repeat(depth)),
    }
}

/// The UTF-16 offset of character `index % (characters + 1)` as a byte.
fn byte_at(text: &str, at: u32) -> usize {
    let characters = text.chars().count();
    let index = at as usize % (characters + 1);
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

/// `text` broken a few ways at random.
fn break_up(rng: &mut Rng, mut text: String) -> String {
    for _ in 0..rng.below(8) {
        let at = byte_at(&text, rng.position());
        let piece: String = match rng.below(10) {
            0..=2 => rng.pick(&PIECES).to_string(),
            3 | 4 => rng.pick(&ODD_CHARACTERS).to_string(),
            5 => rng.pick(&NUMBERS).to_string(),
            // A very long run in a line.
            6 => {
                let unit = rng.pick(&["x", "é", "😀", " ", "\"a\" ", "[1], ", "\t"]);
                unit.repeat(500 + rng.below(6000))
            }
            // Nesting left open.
            7 => rng
                .pick(&["[", "{", "(", "<a>", "- ", "  "])
                .repeat(10 + rng.below(300)),
            // A digit string past every number type.
            8 => "9".repeat(20 + rng.below(800)),
            _ => {
                let pieces = 1 + rng.below(6);
                random_text(rng, pieces)
            }
        };
        text.insert_str(at, &piece);
    }
    // Mixed line endings.
    if rng.chance(30) {
        let mut out = String::with_capacity(text.len() + 64);
        for character in text.chars() {
            if character == '\n' {
                out.push_str(rng.pick(&["\n", "\n", "\r\n", "\r"]));
            } else {
                out.push(character);
            }
        }
        text = out;
    }
    if rng.chance(15) {
        text.insert(0, '\u{feff}');
    }
    // Cut off at a character.
    if rng.chance(25) {
        let keep = byte_at(&text, rng.position());
        text.truncate(keep);
    }
    text
}

/// A hostile text in language `name`.
pub(super) fn hostile(rng: &mut Rng, name: &str) -> String {
    let language = language(name);
    match rng.below(10) {
        0 | 1 => {
            let pieces = 1 + rng.below(200);
            random_text(rng, pieces)
        }
        2..=5 => {
            let text = document(rng, language);
            break_up(rng, text)
        }
        6 | 7 => {
            let value = rng.pick(&NUMBERS[..]).to_string();
            let value = match rng.below(3) {
                0 => value,
                1 => "\"é😀\"".to_string(),
                _ => "1".to_string(),
            };
            let text = many_items(name, 200 + rng.below(1800), &value);
            if rng.chance(50) {
                break_up(rng, text)
            } else {
                text
            }
        }
        _ => {
            let text = nested(name, 1 + rng.below(400));
            if rng.chance(50) {
                break_up(rng, text)
            } else {
                text
            }
        }
    }
}

/// Smaller texts than `text`, most cut first: each half, each line, then
/// (for short texts) each character, and long runs of one character made
/// shorter.
pub(super) fn smaller(text: &str) -> Vec<String> {
    let characters: Vec<char> = text.chars().collect();
    let count = characters.len();
    let mut out = Vec::new();
    let cut = |from: usize, to: usize| -> String {
        characters[..from].iter().chain(&characters[to..]).collect()
    };
    // Chunks of halving size.
    let mut chunk = count / 2;
    while chunk >= 8 {
        let mut from = 0;
        while from < count {
            out.push(cut(from, (from + chunk).min(count)));
            from += chunk;
        }
        chunk /= 2;
        if out.len() > 64 {
            break;
        }
    }
    // Each line.
    let mut start = 0;
    let mut lines = 0;
    for (index, character) in characters.iter().enumerate() {
        if *character == '\n' {
            out.push(cut(start, index + 1));
            start = index + 1;
            lines += 1;
            if lines > 400 {
                break;
            }
        }
    }
    if start < count && lines <= 400 {
        out.push(cut(start, count));
    }
    if count <= 300 {
        for index in 0..count {
            out.push(cut(index, index + 1));
        }
    }
    out
}

/// `text` for a failure's detail: whole when short, else its ends.
pub(super) fn shown(text: &str) -> String {
    if text.chars().count() <= 200 {
        format!("{text:?}")
    } else {
        let head: String = text.chars().take(80).collect();
        let tail: String = {
            let mut tail: Vec<char> = text.chars().rev().take(80).collect();
            tail.reverse();
            tail.into_iter().collect()
        };
        format!("{head:?} … {tail:?} ({} characters)", text.chars().count())
    }
}

/// `text` as a Rust literal for a repro: whole when short; a long run of
/// one piece written as a `repeat`.
pub(super) fn literal(text: &str) -> String {
    format!("{text:?}")
}
