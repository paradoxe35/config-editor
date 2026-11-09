//! The `'static` texts and lists a registered language's configuration is
//! made of, each kept once: languages live as long as the page, so their
//! texts are never freed, but registering a label again (on every mount of
//! a component, say) reuses the ones an earlier registration made instead
//! of leaking new copies (QA 27: 5000 registrations grew memory by 2 MB).

use std::cell::RefCell;
use std::collections::HashSet;

thread_local! {
    static TEXTS: RefCell<HashSet<&'static str>> = RefCell::new(HashSet::new());
    static TEXT_LISTS: RefCell<HashSet<&'static [&'static str]>> = RefCell::new(HashSet::new());
    static PAIRS: RefCell<HashSet<&'static [(char, char)]>> = RefCell::new(HashSet::new());
}

/// `text`, kept once.
pub(super) fn text(text: &str) -> &'static str {
    TEXTS.with(|texts| {
        let mut texts = texts.borrow_mut();
        if let Some(known) = texts.get(text) {
            return *known;
        }
        let kept: &'static str = Box::leak(text.to_owned().into_boxed_str());
        texts.insert(kept);
        kept
    })
}

/// A list of texts, kept once.
pub(super) fn texts<S: AsRef<str>>(items: &[S]) -> &'static [&'static str] {
    let items: Vec<&'static str> = items.iter().map(|item| text(item.as_ref())).collect();
    TEXT_LISTS.with(|lists| {
        let mut lists = lists.borrow_mut();
        if let Some(known) = lists.get(items.as_slice()) {
            return *known;
        }
        let kept: &'static [&'static str] = Box::leak(items.into_boxed_slice());
        lists.insert(kept);
        kept
    })
}

/// A list of `[open, close]` pairs, kept once.
pub(super) fn pairs(items: Vec<(char, char)>) -> &'static [(char, char)] {
    PAIRS.with(|lists| {
        let mut lists = lists.borrow_mut();
        if let Some(known) = lists.get(items.as_slice()) {
            return *known;
        }
        let kept: &'static [(char, char)] = Box::leak(items.into_boxed_slice());
        lists.insert(kept);
        kept
    })
}
