//! Where a text holding objects of many entries breaks, without parsing
//! such an object whole (see `super::big_objects`: the parser compares
//! each key with every key before it).
//!
//! The parser reads a text from its start and stops at the first error.
//! A group of entries that parses alone can hold that error only through
//! a key another group writes again: every other group is left out
//! (its lines blanked, so each place keeps its line and column) and the
//! rest parsed whole, where the parser stops where it stops in the text.
//! Kept are the groups that do not parse alone, with their neighbours (a
//! cut between them may split an entry), every group writing a key another
//! one writes, and every group with a key not told from its text (an
//! expression); a group that does not parse alone with such a key, or
//! one whose key's spelling is not plain, leaves it to the whole parse.

use std::collections::HashMap;

use hcl_edit::expr::{Expression, ObjectKey};

use super::big_objects::{Big, Parsed};
use super::tokens::{Kind, tokenize};

/// What the text with the groups that cannot be at fault left out says.
pub(super) enum Located {
    /// It does not parse: the text's error, at its place.
    Error(hcl::Error),
    /// It parses: so does the text (no key is written twice in error).
    Parses,
    /// A key could not be told: only the whole parse says.
    Unknown,
}

/// The first error of `text`, whose `objects` were parsed in groups:
/// `group(object, index)` is that group parsed, `None` when it does not
/// parse alone.
pub(super) fn located<'p>(
    text: &str,
    objects: &[Big],
    group: impl Fn(usize, usize) -> Option<&'p Parsed>,
) -> Located {
    let mut kept = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        let ranges: Vec<(usize, usize)> = object.groups().collect();
        let Some(keep) = kept_groups(text, object, &ranges, |at| group(index, at)) else {
            return Located::Unknown;
        };
        kept.push((ranges, keep));
    }
    if kept.iter().all(|(_, keep)| keep.iter().all(|kept| *kept)) {
        // Nothing to leave out: the whole parse is all there is.
        return Located::Unknown;
    }
    match super::big_objects::parse(&blanked(text, objects, &kept), false) {
        Err(error) => Located::Error(error),
        Ok(_) => Located::Parses,
    }
}

/// Which groups of `object` (at `ranges`) to keep; `None` when a key of a
/// group that does not parse alone cannot be told.
fn kept_groups<'p>(
    text: &str,
    object: &Big,
    ranges: &[(usize, usize)],
    group: impl Fn(usize) -> Option<&'p Parsed>,
) -> Option<Vec<bool>> {
    let mut keep = vec![false; ranges.len()];
    let mut writers: HashMap<String, Vec<usize>> = HashMap::new();
    for (at, range) in ranges.iter().enumerate() {
        match group(at).and_then(Parsed::object) {
            Some(parsed) => {
                for (key, _) in parsed.iter() {
                    match parsed_key(key) {
                        Some(key) => writers.entry(key).or_default().push(at),
                        None => keep[at] = true,
                    }
                }
            }
            None => {
                let last = (at + 1).min(ranges.len() - 1);
                keep[at.saturating_sub(1)..=last].fill(true);
                for key in written_keys(text, object, *range)? {
                    writers.entry(key).or_default().push(at);
                }
            }
        }
    }
    for groups in writers.values() {
        if groups.iter().any(|at| *at != groups[0]) {
            for at in groups {
                keep[*at] = true;
            }
        }
    }
    Some(keep)
}

/// A key parsed, as the parser compares it with others (an identifier, or
/// a string by its value); `None` for any other expression.
fn parsed_key(key: &ObjectKey) -> Option<String> {
    match key {
        ObjectKey::Ident(ident) => Some(format!("i:{}", ident.as_str())),
        ObjectKey::Expression(Expression::String(text)) => Some(format!("s:{}", text.as_str())),
        ObjectKey::Expression(_) => None,
    }
}

/// The keys the entries of a group that does not parse alone start with,
/// read as [`parsed_key`] reads them: an entry starting with anything else
/// (a stray character, a template, parentheses) can only repeat a key of a
/// kept group, or is where the parser stops. `None` when a string key
/// holds an escape, whose value only the parser tells.
fn written_keys(text: &str, object: &Big, (start, end): (usize, usize)) -> Option<Vec<String>> {
    let tokens = tokenize(&text[start..end]);
    let mut keys = Vec::new();
    let mut tokens = tokens.iter().peekable();
    for entry in object.starts.iter().filter(|at| (start..end).contains(*at)) {
        let offset = entry - start;
        while tokens.next_if(|token| token.start < offset).is_some() {}
        let Some(token) = tokens.peek().filter(|token| token.start == offset) else {
            continue;
        };
        let written = &text[start + token.start..start + token.end];
        match token.kind {
            Kind::Ident => keys.push(format!("i:{written}")),
            Kind::Str { closed: true } if written.contains('\\') => return None,
            Kind::Str { closed: true } if !written.contains("${") && !written.contains("%{") => {
                keys.push(format!("s:{}", &written[1..written.len() - 1]));
            }
            _ => {}
        }
    }
    Some(keys)
}

/// An object's groups (where each starts and ends) and which are kept.
type Kept = (Vec<(usize, usize)>, Vec<bool>);

/// `text` with the lines of each group not kept blanked: from its first
/// entry starting a line to the last place starting a line before the next
/// group (whole lines of whole entries, so what is left parses as it did
/// and every place keeps its line and column).
fn blanked(text: &str, objects: &[Big], kept: &[Kept]) -> String {
    let starts_line = |at: usize| text[..at].trim_end_matches([' ', '\t']).ends_with('\n');
    let mut bytes = text.as_bytes().to_vec();
    for (object, (ranges, keep)) in objects.iter().zip(kept) {
        for (&(start, end), _) in ranges.iter().zip(keep).filter(|(_, kept)| !**kept) {
            let mut lines = object
                .starts
                .iter()
                .copied()
                .filter(|at| (start..end).contains(at))
                .chain([end])
                .filter(|at| starts_line(*at));
            let Some(from) = lines.next() else {
                continue;
            };
            let to = lines.next_back().unwrap_or(from);
            for byte in &mut bytes[from..to] {
                if *byte != b'\n' {
                    *byte = b' ';
                }
            }
        }
    }
    String::from_utf8(bytes).expect("only whole characters are blanked")
}
