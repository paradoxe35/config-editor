//! A syntax message of a text that may hold secrets, without what it
//! quotes of them.
//!
//! The parser stops on a line no reading can tell is a secret field's or
//! not (a key half typed runs into its value: `<passwo"rd>s3cr3t</password>`
//! reads as one broken name), and any message may quote it, the custom
//! languages' too. So nothing is guessed from the wording: the text is read
//! again with every letter and digit from the start of that line on, and in
//! each placeholder, changed to another, and whatever part of the message
//! changes with them is text it quotes from there, which is hidden. The
//! message's own words stay. A change can also change the wording (`t…`
//! begins `true` in TOML), so the text is read so a few times, each
//! changing every letter and digit differently: a quote changes in every
//! reading, a wording in some, and only what changes in all is hidden.
//! An error made of how that line reads beside the ones before it (a key
//! written twice) goes once it is changed alone, so each word of that line
//! is then changed alike where the lines before write it too, which keeps
//! it (the language's own words elsewhere, `true`, stay).

use std::ops::Range;

use crate::editor::language::ParseError;

/// What a hidden part of a message shows.
const HIDDEN: &str = "…";

/// How far each reading moves every letter and digit along its alphabet
/// (each less than 9, so no digit comes back to itself).
const SHIFTS: [u8; 3] = [1, 2, 3];

/// `message`, the syntax error the parser found on `error_line` (one-based;
/// `None`: anywhere) of `source`, with each part quoting the text from that
/// line on, or the text of a placeholder (`tokens`, byte ranges), replaced
/// by `…`. `reread` gives the error of another text (`None` when it reads).
pub(in crate::editor::diagnostics) fn hide_quoted_text(
    message: &str,
    source: &str,
    error_line: Option<usize>,
    tokens: &[Range<usize>],
    reread: impl Fn(&str) -> Option<ParseError>,
) -> String {
    let from = error_line
        .and_then(|line| line_start(source, line))
        .unwrap_or(0);
    let spans: Vec<Range<usize>> = std::iter::once(from..source.len())
        .chain(tokens.iter().cloned())
        .collect();
    let message: Vec<char> = message.chars().collect();
    if masked(source, &spans, SHIFTS[0]) == source {
        return message.into_iter().collect();
    }
    // When the text from that line on reads once changed, the error is in
    // how it reads beside the lines before (a key written twice): each word
    // of the line also changed alike where the lines before write it keeps
    // it, on the same line, quoting the changed letters. A reading erring
    // elsewhere found another error.
    let beside: Vec<Range<usize>> = spans
        .iter()
        .cloned()
        .chain(earlier_copies(source, from))
        .collect();
    let changed = changes(&message, source, &spans, None, &reread)
        .or_else(|| changes(&message, source, &beside, Some(error_line), &reread))
        // Every reading reads: the error is in the very letters, so all
        // the message copies of the line is hidden.
        .unwrap_or_else(|| copied(&message, line_text(source, from)));
    shown(&message, &widened(&message, &changed))
}

/// Whether each character of `message` changed in every reading of
/// `source` with its `spans` masked that found an error (on `line`, when
/// given); `None` when none found one.
fn changes(
    message: &[char],
    source: &str,
    spans: &[Range<usize>],
    line: Option<Option<usize>>,
    reread: &impl Fn(&str) -> Option<ParseError>,
) -> Option<Vec<bool>> {
    let mut changed: Option<Vec<bool>> = None;
    for shift in SHIFTS {
        let Some(other) = reread(&masked(source, spans, shift))
            .filter(|other| line.is_none_or(|line| other.line == line))
        else {
            continue;
        };
        let here = differing(message, &other.message.chars().collect::<Vec<_>>());
        changed = Some(match changed {
            Some(before) => before
                .iter()
                .zip(&here)
                .map(|(one, two)| *one && *two)
                .collect(),
            None => here,
        });
    }
    changed
}

/// Where the text before byte `from` writes a word (letters and digits)
/// of the line starting there, each written whole.
fn earlier_copies(text: &str, from: usize) -> Vec<Range<usize>> {
    let line: std::collections::HashSet<&str> = words(line_text(text, from))
        .map(|word| &line_text(text, from)[word])
        .collect();
    words(&text[..from])
        .filter(|word| line.contains(&text[word.clone()]))
        .collect()
}

/// The byte ranges of the words (runs of letters and digits) of `text`.
fn words(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    let mut start = None;
    text.char_indices()
        .chain(std::iter::once((text.len(), ' ')))
        .filter_map(
            move |(at, character)| match (character.is_alphanumeric(), start) {
                (true, None) => {
                    start = Some(at);
                    None
                }
                (false, Some(begun)) => {
                    start = None;
                    Some(begun..at)
                }
                _ => None,
            },
        )
}

/// The byte the one-based `line` of `text` starts at.
fn line_start(text: &str, line: usize) -> Option<usize> {
    if line <= 1 {
        return Some(0);
    }
    text.match_indices('\n').nth(line - 2).map(|(at, _)| at + 1)
}

/// The line of `text` starting at byte `from`.
fn line_text(text: &str, from: usize) -> &str {
    let rest = &text[from..];
    rest.split('\n').next().unwrap_or(rest)
}

/// `text` with each letter and digit in `spans` moved `shift` along its
/// alphabet (an ASCII one stays ASCII of its case, a zero stays a zero so
/// a number keeps its shape; any other letter or digit becomes an ASCII
/// one), every other character kept: the text reads as it did up to the
/// same error, or nearly.
fn masked(text: &str, spans: &[Range<usize>], shift: u8) -> String {
    let mut spans = spans.to_vec();
    spans.sort_by_key(|span| span.start);
    let mut spans = spans.into_iter().peekable();
    // The span reaching furthest of those started so far.
    let mut reach = 0;
    text.char_indices()
        .map(|(at, character)| {
            while let Some(span) = spans.next_if(|span| span.start <= at) {
                reach = reach.max(span.end);
            }
            if at < reach {
                moved(character, shift)
            } else {
                character
            }
        })
        .collect()
}

/// `character` moved `shift` along its alphabet, when it is a letter or a
/// digit other than zero.
fn moved(character: char, shift: u8) -> char {
    let rotate =
        |first: u8, count: u8| char::from(first + (character as u8 - first + shift) % count);
    match character {
        'a'..='z' => rotate(b'a', 26),
        'A'..='Z' => rotate(b'A', 26),
        '1'..='9' => rotate(b'1', 9),
        other if other.is_alphanumeric() && !other.is_ascii() => char::from(b'a' + shift),
        other => other,
    }
}

/// Whether each character of `message` differs in `other`, the same
/// error's message in a masked text: position by position when they are
/// as long, else everything between what they start and end with alike.
fn differing(message: &[char], other: &[char]) -> Vec<bool> {
    if message.len() == other.len() {
        return message
            .iter()
            .zip(other)
            .map(|(one, two)| one != two)
            .collect();
    }
    let start = message
        .iter()
        .zip(other)
        .take_while(|(one, two)| one == two)
        .count();
    let end = message.len()
        - message
            .iter()
            .rev()
            .zip(other.iter().rev())
            .take_while(|(one, two)| one == two)
            .count()
            .min(message.len() - start);
    (0..message.len())
        .map(|at| start <= at && at < end)
        .collect()
}

/// Whether each character of `message` is in a part at least two
/// characters long copied from `line` (a window of it around its start:
/// what a message quotes of a line is near where the parser stopped, and
/// the search stays linear).
fn copied(message: &[char], line: &str) -> Vec<bool> {
    let line: Vec<char> = line.chars().take(4 * message.len().max(1)).collect();
    let mut found = vec![false; message.len()];
    for start in 0..message.len() {
        let longest = (0..line.len())
            .map(|from| {
                message[start..]
                    .iter()
                    .zip(&line[from..])
                    .take_while(|(one, two)| one == two)
                    .count()
            })
            .max()
            .unwrap_or(0);
        if longest >= 2 {
            found[start..start + longest].fill(true);
        }
    }
    found
}

/// Characters a quote is set off with: a hidden part never takes them.
fn sets_off(character: char) -> bool {
    matches!(
        character,
        '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}'
    )
}

/// The parts of `message` to hide: its `changed` characters, joined where
/// only other characters of one word lie between, and each grown over the
/// rest of its word, so a quoted secret's characters the masking keeps
/// (punctuation, a zero) are hidden with it.
fn widened(message: &[char], changed: &[bool]) -> Vec<Range<usize>> {
    let in_word = |at: usize| !message[at].is_whitespace() && !sets_off(message[at]);
    let mut joined: Vec<Range<usize>> = Vec::new();
    for at in (0..message.len()).filter(|&at| changed[at]) {
        if joined.last().is_some_and(|last| at < last.end) {
            continue;
        }
        let mut range = at..at + 1;
        while range.start > 0 && in_word(range.start - 1) {
            range.start -= 1;
        }
        while range.end < message.len() && in_word(range.end) {
            range.end += 1;
        }
        match joined.last_mut() {
            Some(last)
                if range.start <= last.end
                    || (last.end..range.start).all(|at| !message[at].is_whitespace()) =>
            {
                last.end = last.end.max(range.end);
            }
            _ => joined.push(range),
        }
    }
    joined
}

/// `message` with each of `hidden` (sorted, apart) shown as `…`.
fn shown(message: &[char], hidden: &[Range<usize>]) -> String {
    let mut out = String::new();
    let mut at = 0;
    for range in hidden {
        out.extend(&message[at..range.start]);
        out.push_str(HIDDEN);
        at = range.end;
    }
    out.extend(&message[at..]);
    out
}
