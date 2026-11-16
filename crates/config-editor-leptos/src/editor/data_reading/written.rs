//! The schema-free reading of a value written once: an XML element or
//! HCL block written once is that value, and also the only item of a list
//! of it (`Repeats::lone_item`), so a pointer through `…/0` names it.

use crate::editor::codec::written_inline;
use crate::editor::language::{ConfigLanguage, Repetition, pointer_index};
use crate::editor::paths::{FieldPath, memo_value_span};

/// `pointer` as `language` writes it in `source` where it names a value
/// written once through its list's index: without the `/0` after each
/// field the language writes as a lone item. An index anywhere else (on a
/// scalar, an attribute, an object of labelled blocks, another index, the
/// document) is only an index, and so is one into a list written as one
/// (`ea = []`). `None` when nothing is left out.
pub(crate) fn lone_spelling(
    language: &dyn ConfigLanguage,
    source: &str,
    pointer: &str,
) -> Option<String> {
    if language.repetition() == Repetition::None || !pointer.contains("/0") {
        return None;
    }
    let mut lone = String::new();
    // The item just left out: an index on it names nothing (an element or
    // block is no list).
    let mut left_out = false;
    for segment in pointer.split('/').skip(1) {
        if left_out && pointer_index(segment).is_some() {
            return None;
        }
        if segment == "0" && !lone.is_empty() {
            if memo_value_span(language, source, &lone)
                .is_some_and(|span| written_inline(language, source, span))
            {
                return None;
            }
            if language.lone_item(source, &FieldPath::from(&lone)) {
                left_out = true;
                continue;
            }
        }
        left_out = false;
        lone.push('/');
        lone.push_str(segment);
    }
    (lone != pointer).then_some(lone)
}
