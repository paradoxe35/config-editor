//! The value a token is written in when its field's key is written more
//! than once (a line moved, cut and pasted, or typed into a block that
//! holds the same key: the text does not parse). The language finds one of
//! them by the field's path; which one it finds is its own affair, so a
//! token in another one has no value written for it, and was taken as
//! plain text (typing beside it reached the host glued) or kept its whole
//! line (no way back). Every copy is found in a stand-in of the text with
//! each copy's key renamed apart (letters changed for others, so every
//! byte stays where it is): each token is judged in the value it is
//! written in, however many copies there are (see `copies`).
//!
//! A check judging many placeholders at once (an edit of the whole text)
//! looks for the copies of all their fields together ([`with_own_values`]),
//! so the work is a few readings of the text, never a few for each
//! placeholder (QA 34: Ctrl+A Tab on the large sample pasted four times
//! read the text thousands of times and hung the page). The copies past
//! the one the language finds are read only when a lookup needs one: a
//! token in that copy (most are) costs no stand-in (QA 35: one line moved
//! read every copy of every field near it, 1.5 times slower). What a
//! search finds is kept for the text (`parse_cache::every_copy`): each
//! check of one keystroke looks for the same copies of the same text (QA
//! 36: six searches of it for each keystroke beside a copy).

mod copies;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use super::super::codec::scalar_range;
use super::super::language::ConfigLanguage;
use super::super::parse_cache::every_copy;
use super::super::paths::{in_batch, with_spans, written_value_span};

/// The value ranges written for each field, one per copy of its key.
type Copies = HashMap<String, Vec<Range<usize>>>;

/// A text by address and length, and its language: borrowed while
/// [`with_own_values`] runs for it.
type TextKey = (usize, usize, &'static str);

thread_local! {
    static SEARCHES: RefCell<Vec<(TextKey, Rc<RefCell<Search>>)>> =
        const { RefCell::new(Vec::new()) };
}

fn text_key(behavior: &dyn ConfigLanguage, text: &str) -> TextKey {
    (text.as_ptr() as usize, text.len(), behavior.info().label)
}

/// Runs `work` with the values written for each of `pointers` in `text`,
/// every copy of their keys, looked for together: [`own_value`] calls for
/// those fields of that text inside it share the readings.
pub(crate) fn with_own_values<T>(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointers: &[String],
    work: impl FnOnce() -> T,
) -> T {
    let search = Rc::new(RefCell::new(Search::new(pointers)));
    SEARCHES.with(|searches| {
        searches
            .borrow_mut()
            .push((text_key(behavior, text), search))
    });
    // Taken off on the way out, a panic included.
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            let _ = SEARCHES.try_with(|searches| searches.borrow_mut().pop());
        }
    }
    let _pop = Pop;
    work()
}

/// The bytes of the value written for the field at `pointer` that holds
/// the token at bytes `token` of `text`: in whichever copy of the field's
/// key it is written. (The stand-ins only rename keys: the value's bytes
/// read the same in `text`.)
pub(crate) fn own_value(
    behavior: &dyn ConfigLanguage,
    text: &str,
    pointer: &str,
    token: &Range<usize>,
) -> Option<Range<usize>> {
    let key = text_key(behavior, text);
    let running = SEARCHES.with(|searches| {
        searches
            .borrow()
            .iter()
            .rev()
            .find(|(text, search)| *text == key && search.borrow().wants(pointer))
            .map(|(_, search)| search.clone())
    });
    match running {
        Some(search) => search
            .borrow_mut()
            .value_holding(behavior, text, pointer, token),
        None => Search::new(&[pointer.to_string()]).value_holding(behavior, text, pointer, token),
    }
}

/// How far a [`Search`] has read.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Read {
    Nothing,
    /// The copy the language finds of each field.
    Found,
    /// Every copy.
    Every,
}

/// The copies of some fields' keys in a text, read as far as a lookup
/// needs: the copy the language finds first, every copy after.
struct Search {
    /// The fields looked for (sorted).
    pointers: Vec<String>,
    found: Copies,
    read: Read,
}

impl Search {
    fn new(pointers: &[String]) -> Self {
        let mut pointers = pointers.to_vec();
        pointers.sort_unstable();
        pointers.dedup();
        Search {
            pointers,
            found: Copies::new(),
            read: Read::Nothing,
        }
    }

    fn wants(&self, pointer: &str) -> bool {
        self.pointers
            .binary_search_by(|each| each.as_str().cmp(pointer))
            .is_ok()
    }

    /// The value of `pointer` (one of the fields looked for) that holds
    /// bytes `token`, reading further until one does or every copy is
    /// read.
    fn value_holding(
        &mut self,
        behavior: &dyn ConfigLanguage,
        text: &str,
        pointer: &str,
        token: &Range<usize>,
    ) -> Option<Range<usize>> {
        let holds = |range: &&Range<usize>| range.start <= token.start && token.end <= range.end;
        loop {
            let held = self
                .found
                .get(pointer)
                .and_then(|ranges| ranges.iter().find(holds));
            if let Some(range) = held {
                return Some(range.clone());
            }
            match self.read {
                Read::Nothing => {
                    self.found = found_copies(behavior, text, &self.pointers);
                    self.read = Read::Found;
                }
                Read::Found => {
                    // The same text's copies are looked for by each check
                    // of an edit: found once.
                    let pointers = &self.pointers;
                    self.found = (*every_copy(behavior, text, pointers, || {
                        copies::every_copy(behavior, text, pointers)
                    }))
                    .clone();
                    self.read = Read::Every;
                }
                Read::Every => return None,
            }
        }
    }
}

/// The value of each of `pointers` in the copy of its key the language
/// finds: read in the batch its caller holds for the text, else one
/// reading for all (a single field's lookups are its caller's).
fn found_copies(behavior: &dyn ConfigLanguage, text: &str, pointers: &[String]) -> Copies {
    let read = || {
        pointers
            .iter()
            .filter_map(|pointer| {
                let range = written_value_span(behavior, text, pointer)
                    .and_then(|span| scalar_range(behavior, text, span))?;
                Some((pointer.clone(), vec![range]))
            })
            .collect()
    };
    if pointers.len() == 1 || in_batch(text) {
        read()
    } else {
        with_spans(behavior, text, pointers, read)
    }
}
