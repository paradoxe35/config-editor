//! The attributes of an element cut in an Edit-as view, saved without it,
//! and pasted back (`item_sources`: told from the session's first text):
//! written as attributes again, as that text wrote them, not as child
//! elements (QA `E35-1`: a Spring `component-scan` key pasted after a
//! pause lost its `base-package` attribute).

use super::convert::{AttributePaths, attribute_paths};
use crate::editor::item_sources::{self, ItemSource};
use crate::editor::language::{pointer_index, pointer_segments};
use serde_json::Value;

/// `attributes` (the paths `previous` writes as attributes) with those of
/// each element of `value` told pasted back from the session's first
/// text, at the path it is pasted to.
pub(super) fn with_pasted_back(mut attributes: AttributePaths, previous: &str, value: &Value) -> AttributePaths {
    let Some(sources) = item_sources::told(previous, |held| held == value).filter(|sources| sources.holds_origin_items()) else {
        return attributes;
    };
    let first = attribute_paths(&sources.origin);
    for (pointer, source) in &sources.items {
        let ItemSource::Origin(from) = source else { continue };
        let (from, to) = (element_path(from), element_path(pointer));
        for path in &first {
            if let Some(rest) = path.strip_prefix(from.as_slice())
                && !rest.is_empty()
            {
                attributes.insert(to.iter().chain(rest).cloned().collect());
            }
        }
    }
    attributes
}

/// The element names on the way to `pointer`, as attribute paths spell
/// them: no list indices.
fn element_path(pointer: &str) -> Vec<String> {
    pointer_segments(pointer).into_iter().filter(|segment| pointer_index(segment).is_none()).collect()
}
