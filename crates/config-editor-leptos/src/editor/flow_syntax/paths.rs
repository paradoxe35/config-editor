//! What a path names inside a container written between brackets: each
//! container on the way walked once for its entries (where each item
//! starts, each key and where its value starts), and, while a batch runs
//! for the text, kept for every other path looked up in it. Looking up
//! each of many fields of one list from its start (a lock on every item's
//! field) passed every item before each: time growing with the square of
//! the items.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::super::language::SchemaPathSegment;
use super::super::work_count::{Work, note};
use super::Flow;

/// The entries of one container, in order, as a walk from its opener
/// finds them (byte offsets in the code walked, or in the whole text when
/// kept for it).
enum Entries {
    /// Where each item starts, past the spaces before it: the last may be
    /// a closer or the end of the code, where no item is.
    List(Vec<usize>),
    /// Each entry's key (unquoted), where it is written, and where its
    /// value starts (past its separator); up to the first that is no
    /// entry.
    Map(Vec<(String, usize, usize, usize)>),
}

/// The containers of one text walked so far, by the language read (its
/// label), where their opener is in the text and whether they were walked
/// as a list.
#[derive(Default)]
pub(crate) struct Walked(RefCell<HashMap<Place, Rc<Entries>>>);

/// A container's language label, opener byte, and whether it is a list.
type Place = (&'static str, usize, bool);

/// Where the code walked is in the text whose containers `Walked` keeps.
#[derive(Clone, Copy)]
pub(crate) struct Kept<'k> {
    pub(crate) walked: &'k Walked,
    /// The language the text is read in (its label).
    pub(crate) language: &'static str,
    /// The text's offset of the code's first byte.
    pub(crate) base: usize,
}

impl Flow {
    /// The byte range in `code` of what `path` names inside the container
    /// `code` starts with (spanning lines if needed): the value, or with
    /// `key` the key.
    #[cfg(test)]
    pub(crate) fn path_span(
        &self,
        code: &str,
        path: &[SchemaPathSegment],
        key: bool,
    ) -> Option<(usize, usize)> {
        self.path_span_in(code, path, key, None)
    }

    /// [`Flow::path_span`], each container's entries read from `kept`
    /// when walked before (and kept there).
    pub(crate) fn path_span_in(
        &self,
        code: &str,
        path: &[SchemaPathSegment],
        key: bool,
        kept: Option<Kept<'_>>,
    ) -> Option<(usize, usize)> {
        let start = code.len() - code.trim_start().len();
        self.span_from(code, start, path, key, kept)
    }

    fn span_from(
        &self,
        code: &str,
        at: usize,
        path: &[SchemaPathSegment],
        key: bool,
        kept: Option<Kept<'_>>,
    ) -> Option<(usize, usize)> {
        let at = self.skip_space(code, at, true);
        let Some((segment, rest)) = path.split_first() else {
            return Some((at, self.value_end(code, at)));
        };
        let open = code[at..].chars().next()?;
        let is = |pair: Option<(char, char)>| pair.is_some_and(|(opener, _)| opener == open);
        let list = match segment {
            SchemaPathSegment::ArrayItem(_) if is(self.list()) => true,
            SchemaPathSegment::Key(_) if is(self.map()) => false,
            _ => return None,
        };
        let entries = self.entries_at(code, at, open, list, kept);
        // The entries are kept at their places in the whole text.
        let base = kept.map_or(0, |kept| kept.base);
        match (segment, &*entries) {
            (SchemaPathSegment::ArrayItem(index), Entries::List(starts)) => {
                let position = *starts.get(*index)? - base;
                match code[position..].chars().next() {
                    None => None,
                    Some(next) if self.is_closer(next) => None,
                    Some(_) => self.span_from(code, position, rest, key, kept),
                }
            }
            (SchemaPathSegment::Key(wanted), Entries::Map(keys)) => {
                let (_, key_start, key_end, value) =
                    keys.iter().find(|entry| entry.0 == *wanted)?;
                if rest.is_empty() && key {
                    return Some((key_start - base, key_end - base));
                }
                self.span_from(code, value - base, rest, key, kept)
            }
            _ => None,
        }
    }

    /// The entries of the container opened by `open` at `at`: kept from
    /// an earlier walk, else walked (and kept); at their places in the
    /// whole text when kept.
    fn entries_at(
        &self,
        code: &str,
        at: usize,
        open: char,
        list: bool,
        kept: Option<Kept<'_>>,
    ) -> Rc<Entries> {
        let base = kept.map_or(0, |kept| kept.base);
        let walk = || {
            Rc::new(if list {
                self.list_entries(code, at + open.len_utf8(), base)
            } else {
                self.map_entries(code, at + open.len_utf8(), base)
            })
        };
        let Some(kept) = kept else {
            return walk();
        };
        let place = (kept.language, kept.base + at, list);
        if let Some(entries) = kept.walked.0.borrow().get(&place) {
            return Rc::clone(entries);
        }
        let entries = walk();
        kept.walked
            .0
            .borrow_mut()
            .insert(place, Rc::clone(&entries));
        entries
    }

    /// The items of a list whose first starts at `position` or after, at
    /// their places plus `base`.
    fn list_entries(&self, code: &str, mut position: usize, base: usize) -> Entries {
        let mut starts = Vec::new();
        loop {
            note(Work::FlowEntry);
            position = self.skip_space(code, position, true);
            starts.push(base + position);
            position = self.value_end(code, position);
            position = self.skip_space(code, position, true);
            match code[position..].chars().next() {
                Some(next) if self.is_separator(next) => position += next.len_utf8(),
                _ => return Entries::List(starts),
            }
        }
    }

    /// The entries of a map whose first starts at `position` or after, at
    /// their places plus `base`.
    fn map_entries(&self, code: &str, mut position: usize, base: usize) -> Entries {
        let mut keys = Vec::new();
        loop {
            note(Work::FlowEntry);
            position = self.skip_space(code, position, true);
            while let Some(next) = code[position..].chars().next()
                && self.is_separator(next)
            {
                position = self.skip_space(code, position + next.len_utf8(), true);
            }
            let key_start = position;
            let Some(first) = code[position..].chars().next() else {
                return Entries::Map(keys);
            };
            if self.is_closer(first) {
                return Entries::Map(keys);
            }
            let key_end = if self.quote(first).is_some() {
                self.quote_end(code, position)
            } else {
                let rest_code = &code[position..];
                position
                    + rest_code
                        .char_indices()
                        .find(|(offset, character)| {
                            character.is_whitespace()
                                || self.is_separator(*character)
                                || self.is_closer(*character)
                                || self.starts_key_separator(&rest_code[*offset..])
                        })
                        .map_or(rest_code.len(), |(offset, _)| offset)
            };
            let separator = self.skip_space(code, key_end, false);
            let Some(length) = self.key_separator_len(&code[separator..]) else {
                return Entries::Map(keys);
            };
            let value = separator + length;
            let key_text = self.strip_quotes(&code[key_start..key_end]);
            keys.push((key_text, base + key_start, base + key_end, base + value));
            position = self.value_end(code, self.skip_space(code, value, true));
        }
    }
}
