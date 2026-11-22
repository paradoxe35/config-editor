//! What a language reads off a whole text besides its parse (its tokens,
//! its scan from the top, the spans of its keys, its tags), shared by every
//! feature that asks for the same text: one keystroke's suggestions, their
//! read-only check, the hover and the problems' spans each read the text
//! a keystroke made.
//!
//! A reading is found by its type, a key of the caller's (a caret, whether
//! spans were recorded) and the text itself, compared whole: two texts
//! never share a reading, however alike. Each read that is not found reads
//! the whole text and is counted as such ([`Work::ScannedBytes`]), so work
//! that reads the text once per item shows in the counts wherever it
//! starts.
//!
//! The latest readings of each type are kept, mounted editor or not (a
//! tool asks for one text's spans one by one, and each reading is a pass
//! over the text); the last editor unmounted drops them ([`forget`]), so
//! a closed editor's text does not stay.

#![cfg_attr(
    not(any(feature = "yaml", feature = "hcl", feature = "xml")),
    allow(dead_code, reason = "only YAML, HCL and XML keep readings")
)]

use std::any::{Any, TypeId};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use super::super::work_count::{Work, note_many};

/// How many readings of each type are kept: a keystroke reads its text
/// at the caret and at the line it is on, and a hover at its own place.
const KEPT: usize = 2;

/// One reading: its type, the text it read, the caller's key and the
/// reading.
struct Entry {
    kind: TypeId,
    text: Arc<str>,
    key: Box<dyn Any>,
    value: Rc<dyn Any>,
}

thread_local! {
    /// The latest readings, the last read first.
    static READINGS: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
}

/// The reading of type `T` of `text` at `key`, when one is kept.
pub(crate) fn kept<T: 'static, K: PartialEq + 'static>(text: &str, key: &K) -> Option<Rc<T>> {
    let kind = TypeId::of::<T>();
    READINGS.with(|readings| {
        let mut readings = readings.borrow_mut();
        let at = readings.iter().position(|entry| {
            entry.kind == kind && entry.key.downcast_ref::<K>() == Some(key) && *entry.text == *text
        })?;
        let entry = readings.remove(at);
        let value = Rc::clone(&entry.value).downcast::<T>().ok();
        readings.insert(0, entry);
        value
    })
}

/// The reading of type `T` of `text` at `key`: the one kept, else
/// `make()`, counted as a read of the whole text and kept.
pub(crate) fn reading<T: 'static, K: PartialEq + 'static>(
    text: &str,
    key: K,
    make: impl FnOnce() -> T,
) -> Rc<T> {
    if let Some(value) = kept::<T, K>(text, &key) {
        return value;
    }
    note_many(Work::ScannedBytes, text.len());
    // Made with nothing borrowed: a reading may ask for another one.
    let value = Rc::new(make());
    keep(text, key, Rc::clone(&value));
    value
}

/// Keeps `value` as the reading of type `T` of `text` at `key`.
fn keep<T: 'static, K: PartialEq + 'static>(text: &str, key: K, value: Rc<T>) {
    let kind = TypeId::of::<T>();
    // One copy of a text for all its readings, the editor's when it is
    // its document.
    let text = super::key(text);
    READINGS.with(|readings| {
        let mut readings = readings.borrow_mut();
        let entry = Entry {
            kind,
            text,
            key: Box::new(key),
            value,
        };
        readings.insert(0, entry);
        let mut of_kind = 0;
        readings.retain(|entry| {
            of_kind += usize::from(entry.kind == kind);
            entry.kind != kind || of_kind <= KEPT
        });
    });
}

/// Gives each reading's text to `adopt` (which may share another copy).
pub(super) fn adopt(adopt: &dyn Fn(&mut Arc<str>)) {
    READINGS.with(|readings| {
        readings
            .borrow_mut()
            .iter_mut()
            .for_each(|entry| adopt(&mut entry.text));
    });
}

/// The copy of `text` a reading keeps, when one does.
pub(super) fn shared(text: &str) -> Option<Arc<str>> {
    READINGS.with(|readings| {
        readings
            .try_borrow()
            .ok()?
            .iter()
            .find(|entry| *entry.text == *text)
            .map(|entry| Arc::clone(&entry.text))
    })
}

/// Drops every reading (the last editor unmounted; a test measuring
/// the work of one call from a cold start).
pub(crate) fn forget() {
    let _ = READINGS.try_with(|readings| readings.borrow_mut().clear());
}

/// Whether any reading of type `T` is kept.
#[cfg(all(test, any(feature = "hcl", feature = "xml")))]
pub(crate) fn any_kept<T: 'static>() -> bool {
    let kind = TypeId::of::<T>();
    READINGS.with(|readings| readings.borrow().iter().any(|entry| entry.kind == kind))
}

#[cfg(test)]
mod tests {
    use super::{forget, kept, reading};
    use crate::editor::work_count::{Work, counted};

    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Length(usize);

    #[test]
    fn a_reading_is_made_once_per_text_and_key() {
        forget();
        let text = "a = 1\nb = 2\n";
        let (first, scanned) = counted(Work::ScannedBytes, || {
            reading(text, 3usize, || Length(text.len()))
        });
        assert_eq!((*first, scanned), (Length(12), 12));
        let (again, scanned) = counted(Work::ScannedBytes, || reading(text, 3usize, || Length(0)));
        assert_eq!((*again, scanned), (Length(12), 0));
        // Another key, another reading.
        assert_eq!(*reading(text, 4usize, || Length(1)), Length(1));
        forget();
    }

    /// Texts that hash alike or have the same length never share a
    /// reading: the text is compared whole.
    #[test]
    fn texts_of_the_same_length_never_share_a_reading() {
        forget();
        let _ = reading("abc", (), || Length(1));
        assert_eq!(kept::<Length, ()>("abd", &()), None);
        assert_eq!(*reading("abd", (), || Length(2)), Length(2));
        forget();
    }

    #[test]
    fn only_the_latest_readings_of_a_type_are_kept() {
        forget();
        for text in ["a", "b", "c"] {
            let _ = reading(text, (), || Length(text.len()));
        }
        assert_eq!(kept::<Length, ()>("a", &()), None);
        assert!(kept::<Length, ()>("b", &()).is_some());
        assert!(kept::<Length, ()>("c", &()).is_some());
        forget();
    }
}
