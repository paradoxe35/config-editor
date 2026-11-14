//! The attributes of an element cut in an Edit-as view, saved without it,
//! and pasted back (`item_sources`: told from the session's first text),
//! or pasted into another element in the same save (QA `E36-2`): written
//! as attributes again, as the host wrote them, not as child elements
//! (QA `E35-1`: a Spring `component-scan` key pasted after a pause lost
//! its `base-package` attribute).

use super::convert::{AttributePaths, attribute_paths};
use crate::editor::item_sources::{self, ItemSource};
use crate::editor::language::{pointer_index, pointer_segments};
use serde_json::Value;

/// `attributes` (the paths `previous` writes as attributes) with those of
/// each element of `value` told pasted back from the session's first
/// text, or moved from another element of `previous`, at the path it is
/// pasted to.
pub(super) fn with_pasted_back(mut attributes: AttributePaths, previous: &str, value: &Value) -> AttributePaths {
    let Some(sources) = item_sources::told(previous, |held| held == value) else {
        return attributes;
    };
    let first = sources.holds_origin_items().then(|| attribute_paths(&sources.origin));
    let before = attributes.clone();
    for (pointer, source) in &sources.items {
        let (from, to) = (element_path(source.pointer()), element_path(pointer));
        let held = match source {
            ItemSource::Origin(_) => first.as_ref(),
            ItemSource::Previous(_) => (from != to).then_some(&before),
        };
        let Some(held) = held else { continue };
        for path in held {
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
