//! Whether an edit changes only whitespace between tokens: what a text
//! that does not parse can still be told about formatting. Space between
//! two tokens of a line (after `:`, before `=`, inside `[ a ]`), a line's
//! indent where indentation carries no structure, trailing space and blank
//! lines leave every value as it was; space inside a token (a string, a
//! YAML plain value, a key) or an XML text, at its edges too, or inside a
//! locked value written over lines, changes it.

use std::cell::RefCell;

use super::super::super::highlight::Token;
use super::super::super::language::ConfigLanguage;

/// Whether `new` is `old` with whitespace changed only between tokens.
/// Both are whole lines (the edited ones), so each starts a line; in YAML
/// a line keeps its indent. `content` says whether the whitespace between
/// two characters of `old` (indices) is a locked value's content (a string
/// over lines, an XML text), which never changes.
pub(super) fn between_tokens(
    behavior: &dyn ConfigLanguage,
    old: &str,
    new: &str,
    content: &dyn Fn(usize, usize) -> bool,
) -> bool {
    let old_bytes: Vec<usize> = old.char_indices().map(|(byte, _)| byte).collect();
    let old_chars: Vec<char> = old.chars().collect();
    let new_chars: Vec<char> = new.chars().collect();
    let lines: Vec<&str> = old.split('\n').collect();
    let starts: Vec<usize> = std::iter::once(0)
        .chain(old.match_indices('\n').map(|(at, _)| at + 1))
        .collect();
    let tokens = Tokens {
        behavior,
        lines: &lines,
        cache: RefCell::new(vec![None; lines.len()]),
    };
    let (mut at_old, mut at_new) = (0, 0);
    let mut line = 0;
    // The last character before the gap: its line and byte in that line.
    let mut previous: Option<(usize, usize)> = None;
    loop {
        let run_old = run(&old_chars[at_old..]);
        let run_new = run(&new_chars[at_new..]);
        let next_old = at_old + run_old.len();
        if old_chars.get(next_old) != new_chars.get(at_new + run_new.len()) {
            return false;
        }
        line += run_old
            .iter()
            .filter(|character| **character == '\n')
            .count();
        let next = old_bytes
            .get(next_old)
            .map(|byte| (line, byte - starts[line]));
        // Space that is content (an XML text's own, at its edges
        // included), or that would part a name from its markup.
        let as_content = || {
            let before = &old[..old_bytes.get(at_old).copied().unwrap_or(old.len())];
            let after = &old[old_bytes.get(next_old).copied().unwrap_or(old.len())..];
            at_old > 0 && behavior.space_is_content(before, after)
        };
        if run_old != run_new
            && (as_content()
                || content(at_old, next_old)
                || !free(behavior, &tokens, previous, next, run_old, run_new))
        {
            return false;
        }
        if next.is_none() {
            return true;
        }
        previous = next;
        at_old += run_old.len() + 1;
        at_new += run_new.len() + 1;
    }
}

/// The whitespace starting `chars`.
fn run(chars: &[char]) -> &[char] {
    let length = chars
        .iter()
        .take_while(|character| character.is_whitespace())
        .count();
    &chars[..length]
}

/// Whether the gap between `previous` and `next` (none at the ends) may
/// change from `old` to `new`.
fn free(
    behavior: &dyn ConfigLanguage,
    tokens: &Tokens<'_>,
    previous: Option<(usize, usize)>,
    next: Option<(usize, usize)>,
    old: &[char],
    new: &[char],
) -> bool {
    let breaks = |run: &[char]| run.contains(&'\n');
    let (Some(previous), Some(next)) = (previous, next) else {
        // A first line's indent, or the last line's trailing space.
        return next.is_none() || !behavior.indent_is_structure();
    };
    if breaks(old) || breaks(new) {
        // Trailing space, blank lines, and the next line's indent, which in
        // YAML must stay.
        let indent = |run: &[char]| run.iter().rev().take_while(|c| **c != '\n').count();
        return breaks(old) == breaks(new)
            && (!behavior.indent_is_structure() || indent(old) == indent(new));
    }
    // Two tokens of one line, not one token.
    match (tokens.at(previous), tokens.at(next)) {
        (Some(left), Some(right)) => left != right,
        (None, None) => false,
        _ => true,
    }
}

/// The tokens of the old text's lines, highlighted when a gap needs them.
struct Tokens<'a> {
    behavior: &'a dyn ConfigLanguage,
    lines: &'a [&'a str],
    cache: RefCell<Vec<Option<Vec<Token>>>>,
}

impl Tokens<'_> {
    /// The index of the token holding byte `byte` of line `line`, if any.
    fn at(&self, (line, byte): (usize, usize)) -> Option<(usize, usize)> {
        let mut cache = self.cache.borrow_mut();
        let tokens = cache[line].get_or_insert_with(|| self.behavior.highlight(self.lines[line]));
        tokens
            .iter()
            .position(|token| token.start <= byte && byte < token.end)
            .map(|index| (line, index))
    }
}
