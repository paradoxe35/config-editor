//! The copies of some fields' keys last found in a text written with a key
//! more than once (see `protect::own_value`), shared like the parses: one
//! keystroke checks the text before and after it several times over (its
//! placement, its locks, the glued check, the problems), each looking for
//! the same fields' copies, and finding them reads the whole text twice
//! for every few hundred copies (QA 36: six searches of one text for each
//! keystroke beside a copy).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Range;
use std::rc::Rc;

use super::super::language::ConfigLanguage;
use super::HOLDERS;

/// The value ranges written for each field, one per copy of its key.
type Copies = HashMap<String, Vec<Range<usize>>>;

/// How many searches are kept: the text an edit was made on and the one
/// it made.
const KEPT: usize = 2;

/// A search: the language's label, the text, the fields looked for
/// (sorted) and what was found.
type Entry = (&'static str, String, Vec<String>, Rc<Copies>);

thread_local! {
    /// The latest searches, the last made first.
    static LAST: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// The copies of `pointers` (sorted) in `text`, found by `find` unless they
/// were found for the same text in the same language.
pub(crate) fn every_copy(
    language: &dyn ConfigLanguage,
    text: &str,
    pointers: &[String],
    find: impl FnOnce() -> Copies,
) -> Rc<Copies> {
    let label = language.info().label;
    let cached = LAST.with(|last| {
        let mut last = last.borrow_mut();
        let at = last.iter().position(|(known, written, fields, _)| {
            *known == label && fields == pointers && written == text
        })?;
        let entry = last.remove(at);
        let copies = Rc::clone(&entry.3);
        last.insert(0, entry);
        Some(copies)
    });
    if let Some(copies) = cached {
        return copies;
    }
    let copies = Rc::new(find());
    if HOLDERS.with(Cell::get) > 0 {
        LAST.with(|last| {
            let mut last = last.borrow_mut();
            last.insert(
                0,
                (
                    label,
                    text.to_string(),
                    pointers.to_vec(),
                    Rc::clone(&copies),
                ),
            );
            last.truncate(KEPT);
        });
    }
    copies
}

/// What is kept goes (the last editor unmounted).
pub(super) fn forget() {
    let _ = LAST.try_with(|last| last.borrow_mut().clear());
}
