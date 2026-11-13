//! The last document parsed, shared by everything that reads the same text
//! in the same language: at a pause in typing, validation, the
//! placeholders' check and the placeholders' fields all read the one text
//! (and an Edit-as switch reads back the text it wrote, then validates
//! it), so one parse serves them all instead of one each. Likewise the
//! lines of its multi-line string bodies, which after each keystroke the
//! view, the folds, the comments and the multi-line constructs all read,
//! and what the caret is on, which the suggestions read; and the path of
//! the line typed on while something is locked (see `protect::reach`),
//! which a keystroke's check and its locks both ask, and the next keystroke
//! on the line too (typing in a scalar leaves the line's path as it was).
//!
//! Two parses (an Edit-as view's text and the host's, which each pause
//! reads in turn) and one entry of each other reading, kept only while an editor is mounted ([`hold`] on mount,
//! [`release`] on unmount), so closed editors leave no document behind
//! and code outside an editor (the tools) keeps nothing.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use serde_json::Value;

use super::language::{CompletionContext, ConfigLanguage, Lexical, ParseError};
use super::work_count::{Work, note, note_many};

/// How many parses are kept.
const PARSES_KEPT: usize = 2;

/// A parse: the language's label (its identity), the text and the result.
type Entry = (&'static str, String, Rc<Result<Value, ParseError>>);

/// String bodies: the language's label, the text and its body lines.
type Bodies = (&'static str, String, Rc<Vec<Range<usize>>>);

/// A reading at the caret: the language's label, the text, the caret and
/// what was read there.
type Place<T> = RefCell<Option<(&'static str, String, usize, T)>>;

/// A line's path: the language's label, the text, the line's first UTF-16
/// offset and the path.
type LinePath = (&'static str, String, usize, Option<String>);

/// How many line paths are kept: the text an edit was made on and the one
/// it made.
const LINE_PATHS_KEPT: usize = 2;

thread_local! {
    /// The latest parses, the last read first.
    static LAST: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    static BODIES: RefCell<Option<Bodies>> = const { RefCell::new(None) };
    static CONTEXT: Place<CompletionContext> = const { RefCell::new(None) };
    static LEXICAL: Place<Lexical> = const { RefCell::new(None) };
    static LINE_PATHS: RefCell<Vec<LinePath>> = const { RefCell::new(Vec::new()) };
    /// The editors mounted.
    static HOLDERS: Cell<usize> = const { Cell::new(0) };
}

/// `text` parsed as `language`, from the last parse when it was of the same
/// text in the same language.
pub(crate) fn parsed(language: &dyn ConfigLanguage, text: &str) -> Rc<Result<Value, ParseError>> {
    let label = language.info().label;
    let cached = LAST.with(|last| {
        let mut last = last.borrow_mut();
        let at = last
            .iter()
            .position(|(known, written, _)| *known == label && written == text)?;
        let entry = last.remove(at);
        let result = Rc::clone(&entry.2);
        last.insert(0, entry);
        Some(result)
    });
    if let Some(result) = cached {
        return result;
    }
    note_many(Work::ParsedBytes, text.len());
    let result = Rc::new(language.parse(text));
    if HOLDERS.with(Cell::get) == 0 {
        return result;
    }
    LAST.with(|last| {
        let mut last = last.borrow_mut();
        last.insert(0, (label, text.to_string(), Rc::clone(&result)));
        last.truncate(PARSES_KEPT);
    });
    result
}

/// The lines of `text`'s multi-line string bodies in `language` (see
/// [`ConfigLanguage::string_body_lines`]), from the last reading when it was
/// of the same text in the same language.
pub(crate) fn string_bodies(
    language: &(impl ConfigLanguage + ?Sized),
    text: &str,
) -> Rc<Vec<Range<usize>>> {
    let label = language.info().label;
    let cached = BODIES.with(|last| {
        last.borrow()
            .as_ref()
            .filter(|(known, written, _)| *known == label && written == text)
            .map(|(_, _, bodies)| Rc::clone(bodies))
    });
    if let Some(bodies) = cached {
        return bodies;
    }
    note(Work::StringBodies);
    let bodies = Rc::new(language.string_body_lines(text));
    if HOLDERS.with(Cell::get) > 0 {
        BODIES
            .with(|last| *last.borrow_mut() = Some((label, text.to_string(), Rc::clone(&bodies))));
    }
    bodies
}

/// What the caret at UTF-16 `cursor` of `text` is on, for the suggestions
/// (see [`ConfigLanguage::completion_context`]), from the last reading when
/// it was of the same place: the suggestions and what protection makes of
/// them both ask after each keystroke.
pub(crate) fn completion_context(
    language: &dyn ConfigLanguage,
    text: &str,
    cursor: usize,
) -> CompletionContext {
    let read = || {
        note(Work::CompletionContext);
        language.completion_context(text, cursor)
    };
    at_place(&CONTEXT, language, text, cursor, read)
}

/// Whether the caret at UTF-16 `cursor` of `text` is in a string or a
/// comment (see [`ConfigLanguage::lexical_state`]), from the last reading
/// when it was of the same place: the suggestions, what protection makes of
/// them and the keys typing pairs ask more than once.
pub(crate) fn lexical_state(language: &dyn ConfigLanguage, text: &str, cursor: usize) -> Lexical {
    let read = || {
        note(Work::LexicalState);
        language.lexical_state(text, cursor)
    };
    at_place(&LEXICAL, language, text, cursor, read)
}

/// The path of the line of `text` starting at UTF-16 `line`, read by `read`
/// unless it was read or told (see [`tell_line_path`]) for the same text.
#[cfg_attr(
    not(feature = "protect"),
    allow(dead_code, reason = "only the lock check asks")
)]
pub(crate) fn line_path(
    language: &dyn ConfigLanguage,
    text: &str,
    line: usize,
    read: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let label = language.info().label;
    let known = LINE_PATHS.with(|kept| {
        kept.borrow()
            .iter()
            .find(|(known, written, at, _)| *known == label && *at == line && written == text)
            .map(|(_, _, _, path)| path.clone())
    });
    if let Some(path) = known {
        return path;
    }
    let path = read();
    tell_line_path(language, text, line, path.clone());
    path
}

/// That the line of `text` starting at UTF-16 `line` writes `path` (an edit
/// changing one scalar on it left its path as it was).
#[cfg_attr(
    not(feature = "protect"),
    allow(dead_code, reason = "only the lock check tells")
)]
pub(crate) fn tell_line_path(
    language: &dyn ConfigLanguage,
    text: &str,
    line: usize,
    path: Option<String>,
) {
    if HOLDERS.with(Cell::get) == 0 {
        return;
    }
    let label = language.info().label;
    LINE_PATHS.with(|kept| {
        let mut kept = kept.borrow_mut();
        kept.retain(|(known, written, at, _)| !(*known == label && *at == line && written == text));
        kept.insert(0, (label, text.to_string(), line, path));
        kept.truncate(LINE_PATHS_KEPT);
    });
}

/// `read()`, what `language` reads at `cursor` of `text`, kept in `slot`.
fn at_place<T: Clone + 'static>(
    slot: &'static std::thread::LocalKey<Place<T>>,
    language: &dyn ConfigLanguage,
    text: &str,
    cursor: usize,
    read: impl FnOnce() -> T,
) -> T {
    let label = language.info().label;
    let cached = slot.with(|last| {
        last.borrow()
            .as_ref()
            .filter(|(known, written, at, _)| *known == label && *at == cursor && written == text)
            .map(|(_, _, _, kept)| kept.clone())
    });
    if let Some(kept) = cached {
        return kept;
    }
    let made = read();
    if HOLDERS.with(Cell::get) > 0 {
        slot.with(|last| {
            *last.borrow_mut() = Some((label, text.to_string(), cursor, made.clone()))
        });
    }
    made
}

/// [`parsed`], as an owned document.
pub(crate) fn parse(language: &dyn ConfigLanguage, text: &str) -> Result<Value, ParseError> {
    (*parsed(language, text)).clone()
}

/// An editor mounted: parses are kept.
pub(crate) fn hold() {
    HOLDERS.with(|holders| holders.set(holders.get() + 1));
}

/// An editor unmounted: what is kept goes (the editors still mounted read
/// it again once).
pub(crate) fn release() {
    let last = HOLDERS
        .try_with(|holders| {
            holders.set(holders.get().saturating_sub(1));
            holders.get() == 0
        })
        .unwrap_or(false);
    // The last editor gone: what the languages keep goes too.
    if last {
        for language in super::Language::ALL {
            language.behavior().forget_kept();
        }
    }
    let _ = LAST.try_with(|last| last.borrow_mut().clear());
    let _ = BODIES.try_with(|last| last.borrow_mut().take());
    let _ = CONTEXT.try_with(|last| last.borrow_mut().take());
    let _ = LEXICAL.try_with(|last| last.borrow_mut().take());
    let _ = LINE_PATHS.try_with(|kept| kept.borrow_mut().clear());
}
