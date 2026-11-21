//! Whether a language writes a document, asked once per save: a save
//! renders its value whole (for what the host reads back), then each
//! patch checks the host can express the value it patches to, which is
//! most often the same value over the same text. Inside a save
//! ([`sharing`]) the last answer is kept with what it was asked of (the
//! language, the value, the text written over and what the save told),
//! so the same question is not answered by writing the document again.

use std::cell::{Cell, RefCell};

use serde_json::Value;

use super::super::item_sources::SaveTold;
use super::super::language::ConfigLanguage;

/// What a write was asked of, and whether it wrote.
struct Asked {
    language: &'static str,
    value: Value,
    previous: Option<String>,
    told: (usize, bool, usize),
    wrote: bool,
}

thread_local! {
    /// How many saves are under way on this thread (a save may convert
    /// inside another's step).
    static SHARING: Cell<usize> = const { Cell::new(0) };
    static LAST: RefCell<Option<Asked>> = const { RefCell::new(None) };
}

/// Runs `work` (a save) with the last write's answer kept for its steps.
pub(crate) fn sharing<T>(work: impl FnOnce() -> T) -> T {
    SHARING.with(|sharing| sharing.set(sharing.get() + 1));
    let out = work();
    SHARING.with(|sharing| {
        sharing.set(sharing.get() - 1);
        if sharing.get() == 0 {
            LAST.with(|last| last.borrow_mut().take());
        }
    });
    out
}

/// Notes that `language` wrote `value` over `previous` told `told`
/// (`wrote`), or refused to.
pub(super) fn note(
    language: &dyn ConfigLanguage,
    value: &Value,
    previous: Option<&str>,
    told: SaveTold<'_>,
    wrote: bool,
) {
    if SHARING.with(Cell::get) == 0 {
        return;
    }
    LAST.with(|last| {
        *last.borrow_mut() = Some(Asked {
            language: language.info().label,
            value: value.clone(),
            previous: previous.map(str::to_string),
            told: told.identity(),
            wrote,
        });
    });
}

/// Whether `language` writes `value` over `previous` told `told`: the
/// answer kept when it was asked the same, else a write of it.
pub(super) fn writes(
    language: &dyn ConfigLanguage,
    value: &Value,
    previous: Option<&str>,
    told: SaveTold<'_>,
) -> bool {
    let known = LAST.with(|last| {
        last.borrow().as_ref().and_then(|asked| {
            (asked.language == language.info().label
                && asked.told == told.identity()
                && asked.previous.as_deref() == previous
                && asked.value == *value)
                .then_some(asked.wrote)
        })
    });
    if let Some(wrote) = known {
        return wrote;
    }
    let wrote = language.write_document_over(value, previous, told).is_ok();
    note(language, value, previous, told, wrote);
    wrote
}
