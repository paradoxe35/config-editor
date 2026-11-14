//! `readOnlyRanges` a host gives again unchanged, with no new text: a
//! host passing all its options to `update` again (a framework's props),
//! or listing every document again with `{ ...document, value }` after an
//! edit. The editor has moved those ranges with each edit since; reading
//! them again, at the places they name, in the edited text would move the
//! lock onto other text (a line added above a locked one) and free the
//! locked value, or throw (a `rangeOf` result's index and line forms no
//! longer agree there). So ranges equal to the last ones given (the same
//! JSON) keep where the editor has them while the text stays the one it
//! holds: the text it already holds, an echo of one it sent, or one older
//! than the typing (see [`stay`]: one rule for a `ConfigEditor` and each
//! document of a `ConfigDocuments`); ranges that differ, or come with a
//! new text that replaces the document, or with `rangesFor`, are read in
//! the text they are given for, as always.

use std::collections::HashMap;

use config_editor_leptos::TextRange;
use wasm_bindgen::JsValue;

/// What a list of ranges is compared by: its JSON. `None` when it has none
/// (a cycle, a value JSON cannot write): it then equals no other.
fn print(list: &JsValue) -> Option<String> {
    js_sys::JSON::stringify(list)
        .ok()
        .and_then(|text| text.as_string())
}

/// Whether ranges given again (`again`: equal to the last ones given, no
/// `rangesFor`) stay where the editor moved them: unless the text they
/// come with replaces the document (`replaces`). A text the document
/// holds, an echo of one it sent or one older than the typing keeps them,
/// whatever order the echoes come back in. Whether it replaces is known
/// before the text is taken (`Echoes::replaces`), so ranges that stay are
/// not read in it at all: not checked against a text they were not given
/// for (QA 34, HO34-1: an echo warned of columns past a line's end).
pub(crate) fn stay(again: bool, replaces: bool) -> bool {
    again && !replaces
}

/// What became of a document's ranges listed again as given last.
pub(crate) enum Again {
    /// Its text does not replace the one its editor holds (that text, an
    /// echo): they are the ones its editor was given, which it moved with
    /// each edit since. Not in the listed text's places, they are not moved
    /// from it to the document (QA 34: moved as if they were, a lock whose
    /// line ended past the end of the line that took its place above it
    /// was given again on that line).
    Stayed,
    /// Its text replaces the one its editor holds: they were read in it.
    /// These are the ones its editor was given, which stay if that text
    /// turns out not to replace its own after all.
    Read(Vec<TextRange>),
}

/// The ranges a `ConfigEditor` was given last.
pub(crate) struct Given(std::cell::RefCell<Option<String>>);

impl Given {
    pub(crate) fn new(list: &JsValue) -> Self {
        Self(std::cell::RefCell::new(print(list)))
    }

    /// Whether `list` are the ranges given last.
    pub(crate) fn same(&self, list: &JsValue) -> bool {
        let print = print(list);
        print.is_some() && *self.0.borrow() == print
    }

    /// `list` are given (once every option is checked).
    pub(crate) fn remember(&self, list: &JsValue) {
        *self.0.borrow_mut() = print(list);
    }
}

/// Each listed document's ranges as given (their JSON) and as read, from
/// the last list, and from the list being read (which replaces them once
/// it is read whole).
#[derive(Default)]
pub(crate) struct Listed {
    last: HashMap<String, (String, Vec<TextRange>)>,
    read: HashMap<String, (String, Vec<TextRange>)>,
}

impl Listed {
    /// Document `id`'s ranges, given as `list`, when they are the ones
    /// given last: as its editor was given them (it has moved them since).
    pub(crate) fn again(&self, id: &str, list: &JsValue) -> Option<Vec<TextRange>> {
        let print = print(list)?;
        let (known, ranges) = self.last.get(id)?;
        (*known == print).then(|| ranges.clone())
    }

    /// Document `id`'s ranges, given as `list`, read as `ranges`.
    pub(crate) fn read(&mut self, id: &str, list: &JsValue, ranges: Vec<TextRange>) {
        match print(list) {
            Some(print) => {
                self.read.insert(id.to_string(), (print, ranges));
            }
            None => {
                self.read.remove(id);
            }
        }
    }

    /// Document `id` keeps `ranges`, those its editor was given before: the
    /// text the list gave it did not replace its own (see [`stay`]).
    pub(crate) fn kept(&mut self, id: &str, ranges: Vec<TextRange>) {
        if let Some((_, last)) = self.last.get_mut(id) {
            *last = ranges;
        }
    }

    /// The list is read whole: what it read is kept for the next one.
    pub(crate) fn finish(&mut self) {
        self.last = std::mem::take(&mut self.read);
    }

    /// The list could not be read: the last one's ranges stay.
    pub(crate) fn abandon(&mut self) {
        self.read.clear();
    }
}
