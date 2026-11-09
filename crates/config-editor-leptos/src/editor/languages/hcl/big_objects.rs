//! Objects of many entries parsed in groups of entries.
//!
//! `hcl-edit` checks each key of an object against every key before it, so
//! one object of thousands of entries (a big `locals` map) took time
//! growing with the square of its entries. Each such object is cut out of
//! the text where its entries start (a key at the start of a line, or after
//! a comma, as the token scan finds them) and parsed `GROUP` entries at a
//! time; the text around it is parsed with a stand-in in its place. When a
//! part does not parse (an error, or a cut that is not between entries),
//! the error is found without parsing such an object whole (see
//! `super::big_errors`).

use std::cell::Cell;

use hcl_edit::expr::{Expression, Object, ObjectKey};
use hcl_edit::structure::Body;

use crate::editor::work_count::{Work, note};

use super::tokens::{Kind, tokenize};

/// Entries parsed together.
const GROUP: usize = 64;

/// Stands in for an object parsed in groups, followed by the number of its
/// groups in [`Parsed::groups`]: a key no document writes.
const STAND_IN: &str = "\u{0}groups\u{0}";

/// A text parsed: its body, and the groups of each object of many entries
/// cut out of it (each group a body `v = { … }` of its own).
pub(super) struct Parsed {
    pub(super) body: Body,
    groups: Vec<Vec<Parsed>>,
    /// Where each object parsed in groups is in the text.
    pub(super) objects: Vec<Big>,
    /// Whether a key two groups of one object both write is read as the
    /// whole parse reads it (the later value in the earlier place): once
    /// the text is known to parse whole.
    repeats: Cell<bool>,
}

impl Parsed {
    /// The groups `object` stands in for, when it is a stand-in.
    pub(super) fn groups_of(&self, object: &Object) -> Option<&[Parsed]> {
        if self.groups.is_empty() || object.len() != 1 {
            return None;
        }
        let (ObjectKey::Expression(Expression::String(key)), _) = object.iter().next()? else {
            return None;
        };
        let index: usize = key.as_str().strip_prefix(STAND_IN)?.parse().ok()?;
        self.groups.get(index).map(Vec::as_slice)
    }

    /// Every group of every object parsed in groups.
    #[cfg(test)]
    pub(super) fn all_groups(&self) -> impl Iterator<Item = &Parsed> {
        self.groups.iter().flatten()
    }

    /// The groups of the object `object` (in [`Parsed::objects`]).
    pub(super) fn groups(&self, object: usize) -> &[Parsed] {
        &self.groups[object]
    }

    /// Whether a key written in two groups reads as in the whole parse.
    pub(super) fn repeats(&self) -> bool {
        self.repeats.get()
    }

    /// Reads a key written in two groups as the whole parse does, the
    /// text being known to parse whole.
    pub(super) fn allow_repeats(&self) {
        self.repeats.set(true);
    }

    /// The object of a group (`v = { … }`).
    pub(super) fn object(&self) -> Option<&Object> {
        self.body
            .iter()
            .find_map(|structure| structure.as_attribute())
            .and_then(|attribute| attribute.value.as_object())
    }
}

/// `text` parsed, heredocs and whole floats read (see
/// `super::heredoc::read_heredocs`); with `grouping`, each object of many
/// entries in groups.
pub(super) fn parse(text: &str, grouping: bool) -> Result<Parsed, hcl::Error> {
    let objects = if grouping { big_objects(text) } else { Vec::new() };
    if objects.is_empty() {
        return whole(text);
    }
    match grouped(text, objects) {
        Ok(parsed) => Ok(parsed),
        Err(failed) => match super::big_errors::located(text, &failed.objects, |object, group| failed.groups[object][group].as_ref()) {
            super::big_errors::Located::Error(error) => Err(error),
            _ => whole_of_big(text),
        },
    }
}

/// `text` parsed as written, no object in groups.
fn whole(text: &str) -> Result<Parsed, hcl::Error> {
    let mut body: Body = text.parse()?;
    super::heredoc::read_heredocs(&mut body);
    Ok(Parsed { body, groups: Vec::new(), objects: Vec::new(), repeats: Cell::new(false) })
}

/// [`whole`] for a text holding an object of many entries, which the
/// parser reads in time growing with the square of its entries: only when
/// nothing else tells what the text reads.
pub(super) fn whole_of_big(text: &str) -> Result<Parsed, hcl::Error> {
    note(Work::UngroupedParse);
    whole(text)
}

/// A text whose objects of many entries did not all parse in groups: each
/// group parsed (`None` where it does not parse).
struct Failed {
    objects: Vec<Big>,
    groups: Vec<Vec<Option<Parsed>>>,
}

/// `text` parsed with each of `objects` in groups; when a part does not
/// parse, every group parsed or not.
fn grouped(text: &str, objects: Vec<Big>) -> Result<Parsed, Failed> {
    let mut skeleton = String::with_capacity(text.len());
    let mut groups = Vec::with_capacity(objects.len());
    let mut at = 0;
    for (index, object) in objects.iter().enumerate() {
        skeleton.push_str(&text[at..=object.open]);
        skeleton.push_str(&format!("\"\\u0000groups\\u0000{index}\" = null "));
        at = object.close;
        let parts: Vec<Option<Parsed>> = object
            .groups()
            .map(|(start, end)| {
                parse(&format!("v = {{\n{}\n}}\n", &text[start..end]), true)
                    .ok()
                    .filter(|group| group.body.len() == 1 && group.object().is_some())
            })
            .collect();
        groups.push(parts);
    }
    skeleton.push_str(&text[at..]);
    let body = match groups.iter().flatten().all(Option::is_some) {
        true => skeleton.parse::<Body>().ok(),
        false => None,
    };
    let Some(mut body) = body else {
        return Err(Failed { objects, groups });
    };
    super::heredoc::read_heredocs(&mut body);
    let groups = groups.into_iter().map(|parts| parts.into_iter().flatten().collect()).collect();
    Ok(Parsed { body, groups, objects, repeats: Cell::new(false) })
}

/// An object literal of many entries: where its braces are and where each
/// entry starts (bytes).
pub(super) struct Big {
    open: usize,
    pub(super) close: usize,
    pub(super) starts: Vec<usize>,
}

impl Big {
    /// Where each group of entries starts and ends (bytes).
    pub(super) fn groups(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.starts.chunks(GROUP).enumerate().map(|(chunk, starts)| {
            let end = self.starts.get((chunk + 1) * GROUP).copied().unwrap_or(self.close);
            (starts[0], end)
        })
    }
}

/// What a bracket open while the tokens are walked holds.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Holds {
    /// A body: the top level or a block's.
    Body,
    /// An object literal that is a value of the data (an attribute's, an
    /// entry's or an item's, written alone).
    Object,
    /// A list literal that is a value of the data.
    List,
    /// Anything else: a call, parentheses, a `for`, an object or list
    /// inside an expression (read as the expression's text).
    Other,
}

struct Open {
    at: usize,
    holds: Holds,
    /// Where each entry of an object starts.
    starts: Vec<usize>,
    /// Tokens of the entry being read, at this level.
    entry_tokens: usize,
    /// A `,` was the last token at this level: an entry starts next.
    after_comma: bool,
}

/// The object literals of `text` that are values of its data with more
/// than two groups of entries, but none inside another (its groups hold
/// it).
fn big_objects(text: &str) -> Vec<Big> {
    let tokens: Vec<_> = tokenize(text).into_iter().filter(|token| token.kind != Kind::Comment).collect();
    let mut open: Vec<Open> = Vec::new();
    let mut found: Vec<Big> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        let closing = matches!(token.kind, Kind::Punct('}' | ']' | ')'));
        if let Some(level) = open.last_mut()
            && level.holds == Holds::Object
            && !closing
        {
            if level.starts.is_empty() || level.after_comma || token.nl_before {
                level.starts.push(token.start);
                level.entry_tokens = 0;
            }
            level.entry_tokens += 1;
        }
        let parent = open.last().map_or(Holds::Body, |level| level.holds);
        // A value of the data starts here: the first token after an
        // attribute's `key =` or an entry's `key =`/`key:`, or an item of a
        // data list.
        let previous = |back: usize| index.checked_sub(back).map(|at| &tokens[at]);
        let value_start = match parent {
            Holds::Body => {
                previous(1).is_some_and(|equals| equals.kind == Kind::Punct('='))
                    && previous(2).is_some_and(|key| key.kind == Kind::Ident && key.nl_before)
            }
            Holds::Object => {
                open.last().is_some_and(|level| level.entry_tokens == 3)
                    && previous(1).is_some_and(|separator| matches!(separator.kind, Kind::Punct('=' | ':')))
            }
            Holds::List => previous(1).is_some_and(|before| matches!(before.kind, Kind::Punct('[' | ','))),
            Holds::Other => false,
        };
        let starts_for = || {
            tokens.get(index + 1).is_some_and(|next| next.kind == Kind::Ident && &text[next.start..next.end] == "for")
        };
        let holds = match token.kind {
            Kind::Punct('{') if value_start && !starts_for() => Some(Holds::Object),
            // A block's body: after its type and labels, in a body.
            Kind::Punct('{') if parent == Holds::Body && previous(1).is_some_and(|label| matches!(label.kind, Kind::Ident | Kind::Str { .. })) => Some(Holds::Body),
            Kind::Punct('[') if value_start && !starts_for() => Some(Holds::List),
            Kind::Punct('{' | '[' | '(') => Some(Holds::Other),
            _ => None,
        };
        if let Some(holds) = holds {
            if let Some(level) = open.last_mut() {
                level.after_comma = false;
            }
            open.push(Open { at: token.start, holds, starts: Vec::new(), entry_tokens: 0, after_comma: false });
            continue;
        }
        if closing {
            let Some(level) = open.pop() else {
                continue;
            };
            // Written alone: the value ends with the object.
            let alone = tokens
                .get(index + 1)
                .is_none_or(|next| next.nl_before || matches!(next.kind, Kind::Punct(',' | '}' | ']')));
            if level.holds == Holds::Object && token.kind == Kind::Punct('}') && alone && level.starts.len() > 2 * GROUP {
                found.push(Big { open: level.at, close: token.start, starts: level.starts });
            }
        }
        if let Some(level) = open.last_mut() {
            level.after_comma = token.kind == Kind::Punct(',');
        }
    }
    found.sort_by_key(|big| big.open);
    let mut outermost: Vec<Big> = Vec::new();
    for big in found {
        if outermost.last().is_none_or(|last| big.open > last.close) {
            outermost.push(big);
        }
    }
    outermost
}
