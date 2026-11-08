//! Value spans looked up together: placing a schema problem asks where its
//! value and each ancestor is written, and each such lookup reads the whole
//! document in some languages (TOML, HCL). [`with_value_spans`] reads the
//! spans of every pointer the problems will ask for in one pass of the
//! language's [`value_spans`](ConfigLanguage::value_spans), and
//! [`value_span`] answers from them while it runs. Nothing is kept after.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use super::super::language::{ConfigLanguage, Span};
use super::super::work_count::{Work, note};

/// The spans read for one text (by address and length: the text borrowed
/// for the whole call).
struct Memo {
    label: &'static str,
    text: (usize, usize),
    spans: HashMap<String, Option<Span>>,
}

/// What a language read of the text once for the whole call (see
/// [`batch_cached`]).
#[cfg_attr(
    not(any(feature = "yaml", feature = "json")),
    allow(dead_code, reason = "only YAML and JSON share a read")
)]
struct Scratch {
    text: (usize, usize),
    value: Option<Rc<dyn Any>>,
}

thread_local! {
    static MEMO: RefCell<Option<Memo>> = const { RefCell::new(None) };
    static SCRATCH: RefCell<Option<Scratch>> = const { RefCell::new(None) };
}

/// `make()`, a reading of all of `source` (the lines of its keys, say):
/// made once while [`with_value_spans`] runs for that text, so the spans
/// asked one by one inside it (keys as well as values) share it; made each
/// time outside one. Nothing is kept after.
#[cfg(any(feature = "yaml", feature = "json"))]
pub(crate) fn batch_cached<T: 'static>(source: &str, make: impl FnOnce() -> T) -> Rc<T> {
    let key = (source.as_ptr() as usize, source.len());
    let cached = SCRATCH.with(|slot| {
        slot.borrow()
            .as_ref()
            .filter(|scratch| scratch.text == key)
            .and_then(|scratch| scratch.value.clone())
            .and_then(|value| value.downcast::<T>().ok())
    });
    if let Some(value) = cached {
        return value;
    }
    note(Work::DocumentRead);
    let made = Rc::new(make());
    SCRATCH.with(|slot| {
        if let Some(scratch) = slot
            .borrow_mut()
            .as_mut()
            .filter(|scratch| scratch.text == key)
        {
            scratch.value = Some(made.clone());
        }
    });
    made
}

/// Runs `work` with the spans of `pointers` (and each one's ancestors) in
/// `source` read at once; [`value_span`] calls for that text inside it read
/// them instead of the document.
#[cfg_attr(
    not(feature = "schema-validation"),
    allow(dead_code, reason = "schema problems are what read spans at once")
)]
pub(crate) fn with_value_spans<T>(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointers: &[String],
    work: impl FnOnce() -> T,
) -> T {
    let mut wanted: Vec<String> = Vec::new();
    for pointer in pointers {
        let mut end = pointer.len();
        while end > 0 {
            wanted.push(pointer[..end].to_string());
            end = pointer[..end].rfind('/').unwrap_or(0);
        }
    }
    wanted.sort_unstable();
    wanted.dedup();
    let text = (source.as_ptr() as usize, source.len());
    let previous_scratch =
        SCRATCH.with(|slot| slot.borrow_mut().replace(Scratch { text, value: None }));
    note(Work::DocumentRead);
    let spans = behavior.value_spans(source, &wanted);
    let memo = Memo {
        label: behavior.info().label,
        text,
        spans: wanted.into_iter().zip(spans).collect(),
    };
    let previous = MEMO.with(|slot| slot.borrow_mut().replace(memo));
    // Put back on the way out, a panic included.
    struct Restore(Option<(Option<Memo>, Option<Scratch>)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            if let Some((memo, scratch)) = self.0.take() {
                let _ = MEMO.try_with(|slot| *slot.borrow_mut() = memo);
                let _ = SCRATCH.try_with(|slot| *slot.borrow_mut() = scratch);
            }
        }
    }
    let _restore = Restore(Some((previous, previous_scratch)));
    work()
}

/// `behavior.value_span(source, pointer)`, from the spans
/// [`with_value_spans`] read when it holds them.
pub(crate) fn value_span(
    behavior: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
) -> Option<Span> {
    let known = MEMO.with(|slot| {
        slot.borrow().as_ref().and_then(|memo| {
            (memo.label == behavior.info().label
                && memo.text == (source.as_ptr() as usize, source.len()))
                .then(|| memo.spans.get(pointer).copied())
                .flatten()
        })
    });
    match known {
        Some(span) => span,
        None => {
            note(Work::DocumentRead);
            behavior.value_span(source, pointer)
        }
    }
}

#[cfg(all(test, feature = "toml"))]
mod tests {
    use super::super::super::languages::Language;
    use super::{value_span, with_value_spans};

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
            .map(|pointer| toml.value_span(text, pointer))
            .collect();
        let memoized = with_value_spans(toml, text, &pointers, || {
            pointers
                .iter()
                .map(|pointer| value_span(toml, text, pointer))
                .collect::<Vec<_>>()
        });
        assert_eq!(memoized, alone);
    }
}
