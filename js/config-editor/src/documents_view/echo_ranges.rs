//! A listed document's text taken as an echo, or not (see `echoing`):
//! what its ranges and host diagnostics become.
//!
//! By path: the native tests (`tests/echo_ranges.rs`) load this file too.

use config_editor_leptos::{Document, TextRange};

use crate::given_ranges::{Again, stay};
use crate::remap::{moved_between, reads_as};

/// The listed `document`'s text was read as an echo, `applies` or not
/// (see `Echo::Apply`), the document being `current`; `again`, what
/// became of its ranges listed again as given last. A text that does not
/// apply keeps `current`, and its ranges and diagnostics (in the listed
/// text's places) move to it, but for ranges that stay where its editor
/// moved them. Returns those, when the list read them in the listed text
/// (the next list compares with them).
pub(crate) fn take(
    document: &mut Document,
    current: &str,
    again: Option<Again>,
    applies: bool,
) -> Option<Vec<TextRange>> {
    // The ranges given last, with a text that does not replace the
    // document's (an echo, or that text as the editor reads it), stay
    // where its editor moved them (see `given_ranges`).
    let replaces = applies && !reads_as(&document.value, current);
    let mut kept = None;
    let ranges_stay = match again {
        Some(Again::Stayed) => true,
        Some(Again::Read(ranges)) if stay(true, replaces) => {
            document.read_only_ranges.clone_from(&ranges);
            kept = Some(ranges);
            true
        }
        _ => false,
    };
    if !applies {
        keep(document, current, ranges_stay);
    }
    kept
}

/// The listed `document`'s text is an echo: it keeps `current`, and the
/// ranges listed with it (in the echoed text) move to `current`, but for
/// ranges that stay where the editor has them (`ranges_stay`).
fn keep(document: &mut Document, current: &str, ranges_stay: bool) {
    if document.value == current {
        return;
    }
    let ranges = !ranges_stay && !document.read_only_ranges.is_empty();
    if ranges || !document.host_diagnostics.is_empty() {
        let (ranges, diagnostics) = moved_between(
            &document.value,
            current,
            ranges.then(|| std::mem::take(&mut document.read_only_ranges)),
            Some(std::mem::take(&mut document.host_diagnostics)),
        );
        if let Some(ranges) = ranges {
            document.read_only_ranges = ranges;
        }
        document.host_diagnostics = diagnostics.unwrap_or_default();
    }
    current.clone_into(&mut document.value);
}

#[cfg(test)]
#[path = "echo_ranges/tests.rs"]
mod tests;
