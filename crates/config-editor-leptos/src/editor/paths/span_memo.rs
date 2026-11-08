//! Spans looked up together: placing problems asks where each one's key and
//! value (and each ancestor's) is written, and each lookup alone reads the
//! whole document. [`with_spans`] reads the spans of every pointer the
//! problems will ask for in one pass of the language's
//! [`key_spans`](ConfigLanguage::key_spans) and
//! [`value_spans`](ConfigLanguage::value_spans), and [`key_span`] and
//! [`value_span`] answer from them while it runs. Readings a language makes
//! of the whole text (its scan, its lines) are shared the same way
//! ([`batch_cached`]). Several texts can be read at once (the host's and the
//! one on screen); nothing is kept after.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::super::language::{ConfigLanguage, Span};
use super::super::work_count::{Work, note};

/// A text by address and length: it is borrowed for the whole batch.
type TextKey = (usize, usize);

fn text_key(source: &str) -> TextKey {
    (source.as_ptr() as usize, source.len())
}

/// The spans and readings of one text, while [`with_spans`] runs for it.
struct Batch {
    label: &'static str,
    text: TextKey,
    keys: HashMap<String, Option<Span>>,
    values: HashMap<String, Option<Span>>,
    /// What the language read of the whole text, one value per type.
    scratch: Vec<Rc<dyn Any>>,
}

thread_local! {
    static BATCHES: RefCell<Vec<Batch>> = const { RefCell::new(Vec::new()) };
}

/// Whether a [`with_spans`] for `source` is running.
pub(crate) fn in_batch(source: &str) -> bool {
    let key = text_key(source);
    BATCHES.with(|batches| batches.borrow().iter().any(|batch| batch.text == key))
}

/// `make()`, a reading of all of `source` (the lines of its keys, say):
/// made once while [`with_spans`] runs for that text, so the spans asked one
/// by one inside it share it; made each time outside one. Each type is one
/// reading: a language wraps what it keeps in a type of its own. Nothing is
/// kept after.
pub(crate) fn batch_cached<T: 'static>(source: &str, make: impl FnOnce() -> T) -> Rc<T> {
    batch_cached_as(Work::DocumentRead, source, make)
}

/// [`batch_cached`], counted as `work`.
pub(crate) fn batch_cached_as<T: 'static>(
    work: Work,
    source: &str,
    make: impl FnOnce() -> T,
) -> Rc<T> {
    let key = text_key(source);
    let cached = BATCHES.with(|batches| {
        batches
            .borrow()
            .iter()
            .filter(|batch| batch.text == key)
            .flat_map(|batch| batch.scratch.iter())
            .find_map(|value| value.clone().downcast::<T>().ok())
    });
    if let Some(value) = cached {
        return value;
    }
    note(work);
    // Made with nothing borrowed: a reading may ask for another one.
    let made = Rc::new(make());
    BATCHES.with(|batches| {
        if let Some(batch) = batches
            .borrow_mut()
            .iter_mut()
            .find(|batch| batch.text == key)
        {
            batch.scratch.push(made.clone());
        }
    });
    made
}

/// Each of `pointers` with its ancestors and the spellings a language
/// writing lone items without `/0` finds it by, sorted, once each.
fn wanted(pointers: &[String]) -> Vec<String> {
    let mut wanted: Vec<String> = Vec::new();
    for pointer in pointers {
        let lone = pointer.replace("/0/", "/");
        let lone = lone.strip_suffix("/0").unwrap_or(&lone).to_string();
        for spelling in [pointer.as_str(), lone.as_str()] {
            let mut end = spelling.len();
            while end > 0 {
                wanted.push(spelling[..end].to_string());
                end = spelling[..end].rfind('/').unwrap_or(0);
            }
        }
    }
    wanted.sort_unstable();
    wanted.dedup();
    wanted
}

/// Runs `work` with the key and value spans of `pointers` (each one's
/// ancestors and spellings too) in `source` read at once; [`key_span`] and
/// [`value_span`] calls for that text inside it answer from them instead
/// of reading the document again.
pub(crate) fn with_spans<T>(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointers: &[String],
    work: impl FnOnce() -> T,
) -> T {
    let wanted = wanted(pointers);
    let text = text_key(source);
    let label = behavior.info().label;
    BATCHES.with(|batches| {
        batches.borrow_mut().push(Batch {
            label,
            text,
            keys: HashMap::new(),
            values: HashMap::new(),
            scratch: Vec::new(),
        });
    });
    // Taken off on the way out, a panic included.
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            let _ = BATCHES.try_with(|batches| batches.borrow_mut().pop());
        }
    }
    let _pop = Pop;
    if !wanted.is_empty() {
        note(Work::DocumentRead);
        let values = behavior.value_spans(source, &wanted);
        let keys = behavior.key_spans(source, &wanted);
        BATCHES.with(|batches| {
            if let Some(batch) = batches.borrow_mut().last_mut() {
                batch.values = wanted.iter().cloned().zip(values).collect();
                batch.keys = wanted.into_iter().zip(keys).collect();
            }
        });
    }
    work()
}

/// The span read for `pointer` in `source` by the running batch, if it read
/// it (`Some(None)`: it is written nowhere).
fn known(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
    keys: bool,
) -> Option<Option<Span>> {
    let (label, text) = (behavior.info().label, text_key(source));
    BATCHES.with(|batches| {
        batches
            .borrow()
            .iter()
            .rev()
            .find(|batch| batch.label == label && batch.text == text)
            .and_then(|batch| {
                let spans = if keys { &batch.keys } else { &batch.values };
                spans.get(pointer).copied()
            })
    })
}

/// `behavior.value_span(source, pointer)`, from the spans [`with_spans`]
/// read when it holds them.
pub(crate) fn value_span(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
) -> Option<Span> {
    known(behavior, source, pointer, false).unwrap_or_else(|| {
        note(Work::DocumentRead);
        behavior.value_span(source, pointer)
    })
}

/// `behavior.key_span(source, pointer)`, from the spans [`with_spans`] read
/// when it holds them.
pub(crate) fn key_span(behavior: &dyn ConfigLanguage, source: &str, pointer: &str) -> Option<Span> {
    known(behavior, source, pointer, true).unwrap_or_else(|| {
        note(Work::DocumentRead);
        behavior.key_span(source, pointer)
    })
}

#[cfg(all(test, feature = "toml"))]
mod tests {
    use super::super::super::languages::Language;
    use super::{key_span, value_span, with_spans};

    /// The spans read at once are the ones read one by one.
    #[test]
    fn spans_read_at_once_agree_with_one_by_one() {
        let text = "[a]\nb = 1\nc = { d = 2 }\n[e.f]\ng = \"h\"\n";
        let toml = Language::TOML.behavior();
        let pointers: Vec<String> = ["/a/b", "/a/c/d", "/e/f/g", "/missing", "/a"]
            .into_iter()
            .map(String::from)
            .collect();
        let alone: Vec<_> = pointers
            .iter()
            .map(|pointer| (toml.key_span(text, pointer), toml.value_span(text, pointer)))
            .collect();
        let batched = with_spans(toml, text, &pointers, || {
            pointers
                .iter()
                .map(|pointer| {
                    (
                        key_span(toml, text, pointer),
                        value_span(toml, text, pointer),
                    )
                })
                .collect::<Vec<_>>()
        });
        assert_eq!(batched, alone);
    }
}
