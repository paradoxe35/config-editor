//! Texts a `pattern` matches, read off the pattern itself: plain letters
//! and digits meet `^[a-z]+$` but no `^x`, so a value of that schema (a
//! list item `minItems` asks for above all) needs text made from what the
//! pattern spells. Each is only a candidate: the fill keeps the ones the
//! schema decides it takes.

use crate::editor::pattern_engine::{Class, Hir, HirKind, parse_pattern};

/// Longest text written for a pattern: past it the least text the pattern
/// asks for is too long to be a useful value to type over.
const MAX_LENGTH: usize = 256;

/// Up to `more` (at least one) distinct texts matching `pattern`, the
/// shortest first: each class's next character (small letters, capitals,
/// digits first), each repetition its least count, the first branch of a
/// choice; none for a pattern the engine cannot read or one asking text
/// longer than [`MAX_LENGTH`]. Assertions (`^`, `$`, `\b`) write nothing,
/// so a text may still miss a word boundary: the caller decides.
pub(super) fn pattern_texts(pattern: &str, more: usize) -> Vec<String> {
    let Some(hir) = parse_pattern(pattern) else {
        return Vec::new();
    };
    let mut found: Vec<String> = Vec::new();
    for variant in 0..more.max(1) {
        let mut text = String::new();
        if write(&hir, variant, &mut text) && !found.contains(&text) {
            found.push(text);
        }
    }
    found
}

/// Writes a text `hir` matches onto `text`, each class taking its
/// `variant`-th character; `false` when it matches none or too long a one.
fn write(hir: &Hir, variant: usize, text: &mut String) -> bool {
    match hir.kind() {
        HirKind::Empty | HirKind::Look(_) => true,
        HirKind::Literal(literal) => match std::str::from_utf8(&literal.0) {
            Ok(literal) => push(text, literal),
            Err(_) => false,
        },
        HirKind::Class(class) => match character(class, variant) {
            Some(character) => push(text, character.encode_utf8(&mut [0; 4])),
            None => false,
        },
        HirKind::Repetition(repetition) => (0..repetition.min)
            .all(|_| write(&repetition.sub, variant, text) && text.len() <= MAX_LENGTH),
        HirKind::Capture(capture) => write(&capture.sub, variant, text),
        HirKind::Concat(parts) => parts.iter().all(|part| write(part, variant, text)),
        // The first branch that writes text: an empty class is none.
        HirKind::Alternation(branches) => branches.iter().any(|branch| {
            let mut written = text.clone();
            let wrote = write(branch, variant, &mut written);
            if wrote {
                *text = written;
            }
            wrote
        }),
    }
}

fn push(text: &mut String, more: &str) -> bool {
    text.push_str(more);
    text.len() <= MAX_LENGTH
}

/// The `variant`-th character `class` takes, cycling: those of `a`–`z`,
/// `A`–`Z` and `0`–`9` it takes, else the first of each of its ranges
/// that is no control character; `None` for a class taking none.
fn character(class: &Class, variant: usize) -> Option<char> {
    let Class::Unicode(class) = class else {
        return None;
    };
    let takes = |character: char| {
        class
            .ranges()
            .iter()
            .any(|range| range.start() <= character && character <= range.end())
    };
    let mut plain: Vec<char> = ('a'..='z')
        .chain('A'..='Z')
        .chain('0'..='9')
        .filter(|&character| takes(character))
        .collect();
    if plain.is_empty() {
        plain = class
            .ranges()
            .iter()
            .filter_map(|range| (range.start()..=range.end()).find(|c| !c.is_control()))
            .collect();
    }
    (!plain.is_empty()).then(|| plain[variant % plain.len()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::completion::pattern_decides;

    /// Each text matches its pattern, and patterns with a choice of
    /// characters give distinct ones.
    #[test]
    fn texts_match_the_pattern_they_are_read_off() {
        let patterns = [
            "^x",
            "^[0-9]{2}-[A-F]+$",
            r"^(alpha|beta)\.v\d$",
            "^.{2,}$",
            "[0-9]",
            r"^\w+@[a-z]+\.(com|org)$",
            "^(?:[^a-z]|q)$",
            "^é+ß?$",
        ];
        for pattern in patterns {
            let texts = pattern_texts(pattern, 3);
            assert!(!texts.is_empty(), "{pattern}: no text");
            for text in &texts {
                assert_eq!(
                    pattern_decides(pattern, text),
                    Some(true),
                    "{pattern}: {text:?}"
                );
            }
        }
        assert_eq!(pattern_texts("^[ab]$", 3), ["a", "b"]);
        // Unreadable, impossible, or too long: none.
        assert!(pattern_texts("(", 2).is_empty());
        assert!(pattern_texts("^[^\\s\\S]$", 2).is_empty());
        assert!(pattern_texts("^a{1000}$", 2).is_empty());
    }
}
