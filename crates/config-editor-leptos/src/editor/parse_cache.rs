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
//! And the documents read the way a schema expects (see [`conformed`]),
//! which an Edit-as save reads in several of its steps, and the copies of
//! fields' keys written more than once (see `copies`), which a keystroke's
//! checks each look for. And what a language reads off a whole text
//! besides its parse (see [`reading`]).
//!
//! Two parses (an Edit-as view's text and the host's, which each pause
//! reads in turn), three documents read as a schema expects and one entry
//! of each other reading, kept only while an editor is mounted ([`hold`] on mount,
//! [`release`] on unmount), so closed editors leave no document behind
//! and code outside an editor (the tools) keeps nothing.

use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;

use serde_json::Value;

use super::language::{CompletionContext, ConfigLanguage, Lexical, ParseError};
use super::text::TextSize;
use super::work_count::{Work, note, note_many};

mod conformed;
mod readings;
pub(crate) use conformed::conformed;
#[cfg(all(test, any(feature = "hcl", feature = "xml")))]
pub(crate) use readings::any_kept;
#[cfg(all(
    test,
    feature = "yaml",
    feature = "json",
    feature = "toml",
    feature = "hcl",
    feature = "xml"
))]
pub(crate) use readings::forget as forget_readings;
#[cfg(any(feature = "yaml", feature = "hcl", feature = "xml"))]
pub(crate) use readings::reading;

/// How many parses are kept.
const PARSES_KEPT: usize = 2;

/// A parse: the language's label (its identity), the text and the result.
type Entry = (&'static str, Arc<str>, Rc<Result<Value, ParseError>>);

/// String bodies: the language's label, the text and its body lines.
type Bodies = (&'static str, Arc<str>, Rc<Vec<Range<usize>>>);

/// A reading at the caret: the language's label, the text, the caret and
/// what was read there.
type Place<T> = RefCell<Option<(&'static str, Arc<str>, usize, T)>>;

/// A line's path: the language's label, the text, the line's first UTF-16
/// offset and the path.
type LinePath = (&'static str, Arc<str>, TextSize, Option<String>);

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
    /// The document an editor wrote last, and the host's text it stands
    /// for (see [`document`]).
    static DOCUMENT: RefCell<[Option<Arc<str>>; 2]> = const { RefCell::new([None, None]) };
}

/// `text`, the document an editor wrote, while one is mounted: what is
/// kept for that text shares the editor's copy instead of making one (see
/// [`key`]).
pub(crate) fn document(text: &Arc<str>) {
    known(0, text);
}

/// `text`, the host's text an editor holds (another than the document
/// while editing as another language): as [`document`].
pub(crate) fn host_text(text: &Arc<str>) {
    known(1, text);
}

fn known(slot: usize, text: &Arc<str>) {
    if HOLDERS.with(Cell::get) == 0 {
        return;
    }
    DOCUMENT.with(|known| known.borrow_mut()[slot] = Some(Arc::clone(text)));
    adopt(text);
}

/// The keys that are `text` take that copy (read before the editor held
/// it: its host text, parsed to be shown in another language).
pub(crate) fn adopt(text: &Arc<str>) {
    if HOLDERS.with(Cell::get) == 0 {
        return;
    }
    let adopt = |held: &mut Arc<str>| {
        if !Arc::ptr_eq(held, text) && **held == **text {
            *held = Arc::clone(text);
        }
    };
    LAST.with(|last| {
        last.borrow_mut()
            .iter_mut()
            .for_each(|entry| adopt(&mut entry.1))
    });
    BODIES.with(|last| {
        last.borrow_mut()
            .iter_mut()
            .for_each(|entry| adopt(&mut entry.1))
    });
    LINE_PATHS.with(|kept| {
        kept.borrow_mut()
            .iter_mut()
            .for_each(|entry| adopt(&mut entry.1))
    });
    CONTEXT.with(|last| {
        last.borrow_mut()
            .iter_mut()
            .for_each(|entry| adopt(&mut entry.1))
    });
    LEXICAL.with(|last| {
        last.borrow_mut()
            .iter_mut()
            .for_each(|entry| adopt(&mut entry.1))
    });
    readings::adopt(&adopt);
    conformed::adopt(&adopt);
}

/// `text` as a key of what is kept: the editor's document, or a text kept
/// already, when it is that text (compared), else a copy. One copy of a
/// text, however many readings of it are kept.
pub(super) fn key(text: &str) -> Arc<str> {
    let same = |held: &Arc<str>| (**held == *text).then(|| Arc::clone(held));
    let found = DOCUMENT
        .with(|known| known.try_borrow().ok()?.iter().flatten().find_map(same))
        .or_else(|| {
            LAST.with(|last| {
                last.try_borrow()
                    .ok()?
                    .iter()
                    .find_map(|entry| same(&entry.1))
            })
        })
        .or_else(|| {
            BODIES.with(|last| {
                last.try_borrow()
                    .ok()?
                    .as_ref()
                    .and_then(|entry| same(&entry.1))
            })
        })
        .or_else(|| {
            LINE_PATHS.with(|kept| {
                kept.try_borrow()
                    .ok()?
                    .iter()
                    .find_map(|entry| same(&entry.1))
            })
        })
        .or_else(|| {
            CONTEXT.with(|last| {
                last.try_borrow()
                    .ok()?
                    .as_ref()
                    .and_then(|entry| same(&entry.1))
            })
        })
        .or_else(|| {
            LEXICAL.with(|last| {
                last.try_borrow()
                    .ok()?
                    .as_ref()
                    .and_then(|entry| same(&entry.1))
            })
        })
        .or_else(|| readings::shared(text))
        .or_else(|| conformed::shared(text));
    found.unwrap_or_else(|| Arc::from(text))
}

/// `text` parsed as `language`, from the last parse when it was of the same
/// text in the same language.
pub(crate) fn parsed(language: &dyn ConfigLanguage, text: &str) -> Rc<Result<Value, ParseError>> {
    let label = language.info().label;
    let cached = LAST.with(|last| {
        let mut last = last.borrow_mut();
        let at = last
            .iter()
            .position(|(known, written, _)| *known == label && **written == *text)?;
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
        last.insert(0, (label, key(text), Rc::clone(&result)));
        last.truncate(PARSES_KEPT);
    });
    result
}

/// Whether `text` parses as `language`, when one of the last parses was of
/// it: read without parsing.
pub(crate) fn known_to_parse(language: &dyn ConfigLanguage, text: &str) -> Option<bool> {
    let label = language.info().label;
    LAST.with(|last| {
        last.borrow()
            .iter()
            .find(|(known, written, _)| *known == label && **written == *text)
            .map(|(_, _, result)| result.is_ok())
    })
}

/// The parse of `text` as `language`, when one of the last parses was of
/// it: read without parsing.
pub(crate) fn kept(
    language: &dyn ConfigLanguage,
    text: &str,
) -> Option<Rc<Result<Value, ParseError>>> {
    let label = language.info().label;
    LAST.with(|last| {
        last.borrow()
            .iter()
            .find(|(known, written, _)| *known == label && **written == *text)
            .map(|(_, _, result)| Rc::clone(result))
    })
}

/// The lines of `text`'s multi-line string bodies in `language` (see
/// [`Text::string_body_lines`]), from the last reading when it was
/// of the same text in the same language.
pub(crate) fn string_bodies(language: &dyn ConfigLanguage, text: &str) -> Rc<Vec<Range<usize>>> {
    let label = language.info().label;
    let cached = BODIES.with(|last| {
        last.borrow()
            .as_ref()
            .filter(|(known, written, _)| *known == label && **written == *text)
            .map(|(_, _, bodies)| Rc::clone(bodies))
    });
    if let Some(bodies) = cached {
        return bodies;
    }
    note(Work::StringBodies);
    let bodies = Rc::new(language.string_body_lines(text));
    if HOLDERS.with(Cell::get) > 0 {
        BODIES.with(|last| *last.borrow_mut() = Some((label, key(text), Rc::clone(&bodies))));
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
/// comment (see [`Text::lexical_state`]), from the last reading
/// when it was of the same place: the suggestions, what protection makes of
/// them and the keys typing pairs ask more than once.
pub(crate) fn lexical_state(language: &dyn ConfigLanguage, text: &str, cursor: usize) -> Lexical {
    let read = || {
        note(Work::LexicalState);
        language.lexical_state(text, cursor)
    };
    at_place(&LEXICAL, language, text, cursor, read)
}

/// The path of the line of `text` starting at byte `line`, read by `read`
/// unless it was read or told (see [`tell_line_path`]) for the same text.
#[cfg_attr(
    not(feature = "protect"),
    allow(dead_code, reason = "only the lock check asks")
)]
pub(crate) fn line_path(
    language: &dyn ConfigLanguage,
    text: &str,
    line: TextSize,
    read: impl FnOnce() -> Option<String>,
) -> Option<String> {
    let label = language.info().label;
    let known = LINE_PATHS.with(|kept| {
        kept.borrow()
            .iter()
            .find(|(known, written, at, _)| *known == label && *at == line && **written == *text)
            .map(|(_, _, _, path)| path.clone())
    });
    if let Some(path) = known {
        return path;
    }
    let path = read();
    tell_line_path(language, text, line, path.clone());
    path
}

/// That the line of `text` starting at byte `line` writes `path` (an edit
/// changing one scalar on it left its path as it was).
#[cfg_attr(
    not(feature = "protect"),
    allow(dead_code, reason = "only the lock check tells")
)]
pub(crate) fn tell_line_path(
    language: &dyn ConfigLanguage,
    text: &str,
    line: TextSize,
    path: Option<String>,
) {
    if HOLDERS.with(Cell::get) == 0 {
        return;
    }
    let label = language.info().label;
    LINE_PATHS.with(|kept| {
        let mut kept = kept.borrow_mut();
        kept.retain(|(known, written, at, _)| {
            !(*known == label && *at == line && **written == *text)
        });
        kept.insert(0, (label, key(text), line, path));
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
            .filter(|(known, written, at, _)| {
                *known == label && *at == cursor && **written == *text
            })
            .map(|(_, _, _, kept)| kept.clone())
    });
    if let Some(kept) = cached {
        return kept;
    }
    let made = read();
    if HOLDERS.with(Cell::get) > 0 {
        slot.with(|last| *last.borrow_mut() = Some((label, key(text), cursor, made.clone())));
    }
    made
}

/// `text` parsed as `language` for a reader beside the editors (the
/// documents view's key names): the parse an editor keeps of it when there
/// is one, else parsed on the side, so the editors' own parses stay kept.
pub(crate) fn kept_or_aside(
    language: super::Language,
    text: &str,
) -> Rc<Result<Value, ParseError>> {
    let behavior = language.behavior();
    kept(behavior, text).unwrap_or_else(|| {
        note_many(Work::ParsedBytes, text.len());
        Rc::new(behavior.parse(text))
    })
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
    if last {
        readings::forget();
    }
    let _ = LAST.try_with(|last| last.borrow_mut().clear());
    let _ = BODIES.try_with(|last| last.borrow_mut().take());
    let _ = CONTEXT.try_with(|last| last.borrow_mut().take());
    let _ = LEXICAL.try_with(|last| last.borrow_mut().take());
    let _ = LINE_PATHS.try_with(|kept| kept.borrow_mut().clear());
    let _ = DOCUMENT.try_with(|known| *known.borrow_mut() = [None, None]);
    conformed::forget();
}
