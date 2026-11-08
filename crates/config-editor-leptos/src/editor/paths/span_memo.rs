//! Value spans looked up together: placing a schema problem asks where its
//! value and each ancestor is written, and each such lookup reads the whole
//! document in some languages (TOML, HCL). [`with_value_spans`] reads the
//! spans of every pointer the problems will ask for in one pass of the
//! language's [`value_spans`](ConfigLanguage::value_spans), and
//! [`value_span`] answers from them while it runs. Nothing is kept after.

use std::cell::RefCell;
use std::collections::HashMap;

use super::super::language::{ConfigLanguage, Span};

/// The spans read for one text (by address and length: the text borrowed
/// for the whole call).
struct Memo {
    label: &'static str,
    text: (usize, usize),
    spans: HashMap<String, Option<Span>>,
}

thread_local! {
    static MEMO: RefCell<Option<Memo>> = const { RefCell::new(None) };
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
    let spans = behavior.value_spans(source, &wanted);
    let memo = Memo {
        label: behavior.info().label,
        text: (source.as_ptr() as usize, source.len()),
        spans: wanted.into_iter().zip(spans).collect(),
    };
    let previous = MEMO.with(|slot| slot.borrow_mut().replace(memo));
    // Put back on the way out, a panic included.
    struct Restore(Option<Option<Memo>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let previous = self.0.take().flatten();
            let _ = MEMO.try_with(|slot| *slot.borrow_mut() = previous);
        }
    }
    let _restore = Restore(Some(previous));
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
        None => behavior.value_span(source, pointer),
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
