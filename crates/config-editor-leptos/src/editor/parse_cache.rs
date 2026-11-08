//! The last document parsed, shared by everything that reads the same text
//! in the same language: at a pause in typing, validation, the
//! placeholders' check and the placeholders' fields all read the one text
//! (and an Edit-as switch reads back the text it wrote, then validates
//! it), so one parse serves them all instead of one each.
//!
//! One entry, kept only while an editor is mounted ([`hold`] on mount,
//! [`release`] on unmount), so closed editors leave no document behind
//! and code outside an editor (the tools) keeps nothing.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use serde_json::Value;

use super::language::{ConfigLanguage, ParseError};

/// A parse: the language's label (its identity), the text and the result.
type Entry = (&'static str, String, Rc<Result<Value, ParseError>>);

thread_local! {
    static LAST: RefCell<Option<Entry>> = const { RefCell::new(None) };
    /// The editors mounted.
    static HOLDERS: Cell<usize> = const { Cell::new(0) };
}

/// `text` parsed as `language`, from the last parse when it was of the same
/// text in the same language.
pub(crate) fn parsed(language: &dyn ConfigLanguage, text: &str) -> Rc<Result<Value, ParseError>> {
    let label = language.info().label;
    let cached = LAST.with(|last| {
        last.borrow()
            .as_ref()
            .filter(|(known, written, _)| *known == label && written == text)
            .map(|(_, _, result)| Rc::clone(result))
    });
    if let Some(result) = cached {
        return result;
    }
    let result = Rc::new(language.parse(text));
    if HOLDERS.with(Cell::get) == 0 {
        return result;
    }
    LAST.with(|last| *last.borrow_mut() = Some((label, text.to_string(), Rc::clone(&result))));
    result
}

/// [`parsed`], as an owned document.
pub(crate) fn parse(language: &dyn ConfigLanguage, text: &str) -> Result<Value, ParseError> {
    (*parsed(language, text)).clone()
}

/// An editor mounted: parses are kept.
pub(crate) fn hold() {
    HOLDERS.with(|holders| holders.set(holders.get() + 1));
}

/// An editor unmounted: the kept parse goes (the editors still mounted
/// parse again once).
pub(crate) fn release() {
    let _ = HOLDERS.try_with(|holders| holders.set(holders.get().saturating_sub(1)));
    let _ = LAST.try_with(|last| last.borrow_mut().take());
}
