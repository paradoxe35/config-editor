//! Problems with secret placeholders: a token the host did not give, in a
//! secret field, stands for no secret; one of the host's tokens outside its
//! own field stands for another field's secret, so it is plain text there,
//! as is one of a field's in a comment and one of a comment's in a field.
//! All are flagged where they are written.

use serde_json::Value;

use super::super::diagnostics::{Diagnostic, Severity};
use super::super::language::{ConfigLanguage, Repetition};
use super::super::parse_cache;
use super::super::paths::{FieldPath, path_at};
use super::copied::copied_placeholders;
use super::marks::marks_at;
use super::moved_off::is_own_field;
use super::texts;
use super::tokens::{Known, SecretMatcher, held_in_fields, in_comment};
use crate::EditorStrings;
use std::collections::HashMap;
use std::ops::Range;

/// A warning on each placeholder of `text` that is not real: one of the
/// host's `known` tokens out of its own field anywhere, and a token the host
/// did not give in a field `schema` marks secret.
///
/// Read from the text alone (not the placeholders' places on screen, which
/// move with every key): one parse says whether each known token is held in
/// its own field as often as it is written, and which field holds each
/// other token; only a token the parse cannot place (written in a comment
/// or a key too, or in several fields) is looked up by path. While the text
/// does not parse, the tokens at `followed` (UTF-16 ranges of the real
/// placeholders, followed through the edits) stay real, as they do on
/// screen, until it parses again.
pub(crate) fn unknown_placeholders(
    behavior: &dyn ConfigLanguage,
    text: &str,
    matcher: &SecretMatcher,
    known: &[Known],
    followed: &[Range<usize>],
    schema: Option<&Value>,
    strings: &EditorStrings,
) -> Vec<Diagnostic> {
    let found = matcher.find(text);
    if found.is_empty() {
        return Vec::new();
    }
    let repeats = behavior.repetition() != Repetition::None;
    let parsed = parse_cache::parsed(behavior, text);
    let document = parsed.as_ref().as_ref().ok();
    let mut written: HashMap<&str, usize> = HashMap::new();
    for range in &found {
        *written.entry(&text[range.clone()]).or_default() += 1;
    }
    let held = document
        .as_ref()
        .map(|document| held_in_fields(document, known))
        .unwrap_or_default();
    // Known tokens the document holds in their fields as often as written.
    let settled = |token: &str| document.is_some() && held.get(token) == written.get(token);
    // The fields the document's strings hold each token in, in the order
    // written: a token written exactly as often as they hold it is, at its
    // nth place in the text, in the field holding it the nth time.
    let holders = document
        .as_ref()
        .map(|document| matcher.holders(document))
        .unwrap_or_default();
    // Fields given a token by a reference to another (an alias), where it
    // is not written.
    let copied = document
        .map(|document| {
            copied_placeholders(behavior, text, document, known, &written, &holders, strings)
        })
        .unwrap_or_default();
    let mut checked: HashMap<&str, bool> = HashMap::new();
    // Each known token's fields, with how often the host wrote it in each
    // (the first is the one a moved token is said to come from), and how
    // often the host wrote it in comments.
    let mut left: HashMap<&str, Vec<(&str, usize)>> = HashMap::new();
    let mut in_comments: HashMap<&str, usize> = HashMap::new();
    for known in known {
        match &known.field {
            Some(field) => left
                .entry(known.token.as_str())
                .or_default()
                .push((field.as_str(), known.count)),
            None => *in_comments.entry(known.token.as_str()).or_default() += known.count,
        }
    }
    let is_known = |token: &str| left.contains_key(token) || in_comments.contains_key(token);
    // The tokens needing a verdict, with their one-based line, the byte
    // their line starts at and their UTF-16 offset (the tokens come in
    // order: their lines are counted in one pass).
    let mut lines = LineCounter::new(text);
    let mut open = Vec::new();
    for range in &found {
        let token = &text[range.clone()];
        if is_known(token) {
            if *checked.entry(token).or_insert_with(|| settled(token)) {
                continue;
            }
        } else if schema.is_none() {
            continue;
        }
        let (line, line_start, at) = lines.advance(range.start);
        if is_known(token)
            && document.is_none()
            && followed
                .binary_search_by_key(&at, |real| real.start)
                .is_ok()
        {
            continue;
        }
        open.push((range, line, line_start, at));
    }
    if open.is_empty() {
        return copied;
    }
    // Which are written in a comment: there only a token the host wrote in
    // a comment is real, and no field is looked up.
    let comments = behavior.comments(text);
    let commented: Vec<bool> = open
        .iter()
        .map(|(range, ..)| in_comment(&comments, range.start))
        .collect();
    let mut outside: HashMap<&str, usize> = HashMap::new();
    for ((range, ..), commented) in open.iter().zip(&commented) {
        if !commented {
            *outside.entry(&text[(*range).clone()]).or_default() += 1;
        }
    }
    // What the parse cannot place (a text that does not parse, mid-typing
    // or a document pasted twice; a token also written in a key) is looked
    // up token by token, each a read of the whole text: past a few, those
    // verdicts wait until the parse can place them.
    let mut placed: HashMap<&str, usize> = HashMap::new();
    let fields: Vec<Option<FieldPath>> = open
        .iter()
        .zip(&commented)
        .map(|((range, ..), commented)| {
            if *commented {
                return None;
            }
            let token = &text[(*range).clone()];
            let pointers = holders.get(token)?;
            if Some(&pointers.len()) != outside.get(token) {
                return None;
            }
            let nth = placed.entry(token).or_default();
            let pointer = pointers.get(*nth)?;
            *nth += 1;
            Some(FieldPath::from_pointer(pointer))
        })
        .collect();
    let look_up = fields
        .iter()
        .zip(&commented)
        .filter(|(field, commented)| field.is_none() && !**commented)
        .count()
        <= MAX_LOOKUPS;
    let mut problems = Vec::new();
    for (((range, line, line_start, at), field), commented) in
        open.into_iter().zip(fields).zip(commented)
    {
        let token = &text[range.clone()];
        let owner = left
            .get(token)
            .and_then(|fields| fields.first())
            .map(|(field, _)| *field);
        let message = if commented {
            // In a comment: real while the host wrote it in a comment as
            // often; a field's token there stands for that field's secret.
            if let Some(count) = in_comments.get_mut(token).filter(|count| **count > 0) {
                *count -= 1;
                continue;
            }
            match owner {
                Some(owner) => texts::placeholder_in_comment(strings, owner),
                None => continue,
            }
        } else {
            let Some(path) =
                field.or_else(|| look_up.then(|| path_at(behavior, text, at)).flatten())
            else {
                continue;
            };
            let pointer = path.to_pointer();
            match owner {
                Some(owner) => {
                    // In its own field (as often as the host wrote it
                    // there): real.
                    if let Some(slot) = left.get_mut(token).and_then(|fields| {
                        fields.iter_mut().find(|(field, count)| {
                            *count > 0 && is_own_field(document, field, &pointer)
                        })
                    }) {
                        slot.1 -= 1;
                        continue;
                    }
                    texts::moved_placeholder(strings, &pointer, owner)
                }
                None if in_comments.contains_key(token) => {
                    texts::comment_placeholder(strings, &pointer)
                }
                None if schema
                    .is_some_and(|schema| marks_at(schema, path.segments(), repeats).1) =>
                {
                    texts::unknown_placeholder(strings, &pointer)
                }
                None => continue,
            }
        };
        let column = text[line_start..range.start].chars().count() + 1;
        problems.push(Diagnostic {
            line: Some(line),
            column: Some(column),
            end_column: Some(column + token.chars().count()),
            message,
            severity: Severity::Warning,
            ..Diagnostic::default()
        });
    }
    problems.extend(copied);
    problems
}

/// Placeholders looked up by path at most in one check; past that, their
/// verdicts wait until the parse can place them.
const MAX_LOOKUPS: usize = 24;

/// Places in a text visited in order: each step reads only the text since
/// the last one, so a walk over the whole text costs one pass.
struct LineCounter<'a> {
    text: &'a str,
    byte: usize,
    /// One-based line of `byte`, and the byte its line starts at.
    line: usize,
    line_start: usize,
    /// UTF-16 offset of `byte`.
    units: usize,
}

impl<'a> LineCounter<'a> {
    fn new(text: &'a str) -> Self {
        LineCounter {
            text,
            byte: 0,
            line: 1,
            line_start: 0,
            units: 0,
        }
    }

    /// The one-based line of byte `to` (not before the last one visited),
    /// the byte its line starts at, and its UTF-16 offset.
    fn advance(&mut self, to: usize) -> (usize, usize, usize) {
        let to = to.max(self.byte);
        let passed = &self.text[self.byte..to];
        self.units += passed.encode_utf16().count();
        if let Some(last) = passed.rfind('\n') {
            self.line += passed.matches('\n').count();
            self.line_start = self.byte + last + 1;
        }
        self.byte = to;
        (self.line, self.line_start, self.units)
    }
}
